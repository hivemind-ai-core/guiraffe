//! DataUi adapter for the animated expanding list.

use gpui_kit::{App, ElementId, Entity, IntoElement, Pixels, RenderOnce, Subscription, Window, px};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};
use guiraffe_motion::{Animated, Spring, Timing};

use super::{ExpandingList, ExpandingListChange};
use crate::{disclosure_data, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[3],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[3]),
            render: |node, _, _, _| ExpandingDocument::new(node).into_any_element(),
        }],
    );
}

#[derive(IntoElement)]
struct ExpandingDocument {
    key: ElementId,
    items: Value,
    value: Option<Option<usize>>,
    body_height: Pixels,
    focusable: Option<bool>,
    on_change: Option<EventHandler>,
}

impl ExpandingDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let height = node
            .prop(2)
            .as_f64()
            .unwrap_or(100.)
            .clamp(0., f64::from(f32::MAX));
        Self {
            key: node.state_id(),
            items: node.prop(0).clone(),
            value: node.is_set(1).then(|| disclosure_data::index(node.prop(1))),
            body_height: px(height as f32),
            focusable: node.prop(3).as_bool(),
            on_change: node.handler(0),
        }
    }
}

struct ExpandingState {
    list: Entity<ExpandingList>,
    items: Value,
    value: Option<Option<usize>>,
    handler: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for ExpandingDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let list =
                ExpandingList::new(disclosure_data::items(&self.items), self.body_height, cx);
            let subscription = cx.subscribe_in(
                &list,
                window,
                |state: &mut ExpandingState, _, event: &ExpandingListChange, window, cx| {
                    if let Some(handler) = &state.handler {
                        handler(
                            event
                                .0
                                .map_or(Value::Null, |index| Value::Num(index as f64)),
                            window,
                            cx,
                        );
                    }
                },
            );
            ExpandingState {
                list,
                items: self.items.clone(),
                value: None,
                handler: None,
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handler = self.on_change;
            let items_changed = state.items != self.items;
            state.list.update(cx, |list, cx| {
                if items_changed {
                    state.items = self.items;
                    let open = list.open.take();
                    list.items = disclosure_data::items(&state.items);
                    list.progress =
                        vec![Animated::new(0., Timing::Spring(Spring::SNAPPY)); list.items.len()];
                    list.sync_open(self.value.unwrap_or(open), cx);
                    cx.notify();
                }
                if self.value != state.value || items_changed {
                    if let Some(value) = self.value {
                        list.sync_open(value, cx);
                    }
                    state.value = self.value;
                }
                if list.body_height != self.body_height {
                    list.body_height = self.body_height;
                    cx.notify();
                }
                if list.focusable != self.focusable {
                    list.set_focusable(self.focusable, cx);
                }
            });
            state.list.clone()
        })
    }
}
