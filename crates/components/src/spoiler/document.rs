//! Document adapter for the native spoiler.

use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
};
use guiraffe_core::{
    ColorName, Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
};
use guiraffe_motion::{Easing, Timing};

use super::{Spoiler, SpoilerChange};
use crate::{Text, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::SPOILER,
            schema: guiraffe_core::widget_schema!(&widget_specs::SPOILER),
            render: |node, _, _, _| DocumentSpoiler::new(node).into_any_element(),
        }],
    );
}

#[derive(IntoElement)]
struct DocumentSpoiler {
    key: ElementId,
    text: SharedString,
    expanded: Option<bool>,
    collapsed_lines: usize,
    size: Option<Size>,
    color: Option<ColorName>,
    focusable: Option<bool>,
    timing: Option<Timing>,
    on_change: Option<EventHandler>,
}

impl DocumentSpoiler {
    fn new(node: &dyn WidgetNode) -> Self {
        let timing = node.prop(6).as_f64().map(|duration| {
            let easing = match node.prop(7).as_str() {
                Some("linear") => Easing::Linear,
                Some("ease-in") => Easing::EaseIn,
                Some("ease-out") => Easing::EaseOut,
                Some("ease-in-out") => Easing::EaseInOut,
                Some("emphasized") => Easing::Emphasized,
                _ => Easing::Ease,
            };
            Timing::tween(std::time::Duration::from_millis(duration as u64), easing)
        });
        Self {
            key: node.state_id(),
            text: node.prop(0).as_str().unwrap_or_default().to_owned().into(),
            expanded: node.prop(1).as_bool(),
            collapsed_lines: node.prop(2).as_f64().unwrap_or(3.).clamp(1., 4096.) as usize,
            size: node.size(3),
            color: node.color(4),
            focusable: node.prop(5).as_bool(),
            timing,
            on_change: node.handler(0),
        }
    }
}

struct State {
    spoiler: Entity<Spoiler>,
    text: Option<SharedString>,
    expanded: Option<bool>,
    on_change: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for DocumentSpoiler {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let spoiler = Spoiler::new(cx);
            let subscription = cx.subscribe_in(
                &spoiler,
                window,
                |state: &mut State, _, event: &SpoilerChange, window, cx| {
                    if let Some(handler) = state.on_change.clone() {
                        handler(event.0.into(), window, cx);
                    }
                },
            );
            State {
                spoiler,
                text: None,
                expanded: None,
                on_change: None,
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.on_change = self.on_change;
            state.spoiler.update(cx, |spoiler, cx| {
                if state.text.as_ref() != Some(&self.text) {
                    let text = self.text.clone();
                    spoiler.set_content(move |_, _| Text::new(text.clone()).into_any_element(), cx);
                    state.text = Some(self.text);
                }
                spoiler.set_collapsed_lines(self.collapsed_lines, cx);
                spoiler.set_size(self.size, cx);
                spoiler.set_color(self.color, cx);
                spoiler.set_focusable(self.focusable, cx);
                if self.timing != spoiler.timing {
                    spoiler.timing = self.timing;
                }
                if state.expanded != self.expanded {
                    if let Some(expanded) = self.expanded {
                        spoiler.sync_expanded(expanded, cx);
                    }
                    state.expanded = self.expanded;
                }
            });
            state.spoiler.clone()
        })
    }
}
