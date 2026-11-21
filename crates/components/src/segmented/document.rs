//! DataUi adapter for native segmented selection.

use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
};
use guiraffe_core::{
    Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{SegmentedControl, SegmentedEvent};
use crate::widget_specs;

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| SegmentedDocument::new(node).into_any_element(),
        }],
    );
}

fn items(values: &Value) -> impl Iterator<Item = SharedString> + '_ {
    (0..values.list_len().unwrap_or_default())
        .filter_map(|index| values.list_get(index).and_then(Value::as_str))
        .map(|item| item.to_owned().into())
}

#[derive(IntoElement)]
struct SegmentedDocument {
    key: ElementId,
    items: Value,
    selection: Option<usize>,
    size: Option<Size>,
    full_width: bool,
    disabled: bool,
    focusable: Option<bool>,
    handler: Option<EventHandler>,
}

impl SegmentedDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            items: node.prop(0).clone(),
            selection: node
                .prop(1)
                .as_f64()
                .filter(|i| (0.0..=f64::from(u32::MAX)).contains(i) && i.fract() == 0.0)
                .map(|i| i as usize),
            size: node.size(2),
            full_width: node.prop(3).as_bool().unwrap_or_default(),
            disabled: node.prop(4).as_bool().unwrap_or_default(),
            focusable: node.prop(5).as_bool(),
            handler: node.handler(0),
        }
    }
}

struct SegmentedState {
    control: Entity<SegmentedControl>,
    items: Value,
    handler: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for SegmentedDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let control = SegmentedControl::new(items(&self.items), cx);
            let subscription = cx.subscribe_in(
                &control,
                window,
                |state: &mut SegmentedState, _, event, window, cx| {
                    let SegmentedEvent::Changed(index) = *event;
                    if let Some(handler) = &state.handler {
                        handler(Value::Num(index as f64), window, cx);
                    }
                },
            );
            SegmentedState {
                control,
                items: self.items.clone(),
                handler: None,
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handler = self.handler;
            state.control.update(cx, |control, cx| {
                if self.items != state.items {
                    state.items = self.items;
                    control.set_items(items(&state.items), cx);
                }
                if let Some(selection) = self.selection {
                    let selection = selection.min(control.items().len().saturating_sub(1));
                    if selection != control.selected {
                        control.selected = selection;
                        cx.notify();
                    }
                }
                control.set_size(self.size, cx);
                control.set_full_width(self.full_width, cx);
                control.set_disabled(self.disabled, cx);
                control.set_focusable(self.focusable, cx);
            });
            state.control.clone()
        })
    }
}
