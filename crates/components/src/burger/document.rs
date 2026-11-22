//! DataUi adapter for the native navigation toggle.

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use guiraffe_core::{
    ColorName, Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::Burger;
use crate::widget_specs;

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[1],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
            render: |node, _, _, _| BurgerDocument::new(node).into_any_element(),
        }],
    );
}

#[derive(IntoElement)]
struct BurgerDocument {
    key: ElementId,
    id: String,
    value: Option<bool>,
    animate: bool,
    focusable: Option<bool>,
    labels: (SharedString, SharedString),
    size: Option<Size>,
    color: Option<ColorName>,
    change: Option<EventHandler>,
}

impl BurgerDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let text = |i, default| match node.prop(i) {
            Value::Str(text) => text.clone(),
            _ => SharedString::from(default),
        };
        Self {
            key: node.state_id(),
            id: node.element_id().to_string(),
            value: node.prop(0).as_bool(),
            animate: node.prop(1).as_bool().unwrap_or(true),
            size: node.size(2),
            color: node.color(3),
            focusable: node.prop(4).as_bool(),
            labels: (text(5, "Open navigation"), text(6, "Close navigation")),
            change: node.handler(0),
        }
    }
}

impl RenderOnce for BurgerDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let control = window
            .use_keyed_state(self.key, cx, |_, cx| {
                let control = Burger::new(cx);
                control.update(cx, |control, cx| {
                    control.id = self.id.clone();
                    control.element_id = format!("gr-burger:{}", control.id).into();
                    if let Some(value) = self.value {
                        control.set_open_instant(value, cx);
                    }
                });
                control
            })
            .read(cx)
            .clone();
        control.update(cx, |control, cx| {
            if let Some(value) = self.value
                && control.is_open() != value
            {
                if self.animate {
                    control.set_open(value, cx);
                } else {
                    control.set_open_instant(value, cx);
                }
            }
            control.set_size(self.size, cx);
            control.set_color(self.color, cx);
            control.set_focusable(self.focusable, cx);
            if control.labels != self.labels {
                let (closed, open) = self.labels;
                control.set_labels(closed, open, cx);
            }
            if let Some(change) = self.change {
                control.on_change(move |open, window, cx| {
                    change(open.into(), window, cx);
                });
            } else {
                control.on_change = None;
            }
        });
        control
    }
}
