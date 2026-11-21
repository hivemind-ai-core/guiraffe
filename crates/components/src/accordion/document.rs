//! DataUi adapter for native multi-item disclosure controls.

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};
use guiraffe_motion::{Animated, Spring, Timing};

use super::{Accordion, AccordionChange, AccordionOptions, AccordionVariant, ChevronPosition};
use crate::{disclosure_data, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[2],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[2]),
            render: |node, _, _, _| AccordionDocument::new(node).into_any_element(),
        }],
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Options {
    variant: AccordionVariant,
    chevron: ChevronPosition,
    multiple: bool,
    focusable: Option<bool>,
}

impl Options {
    fn native(self) -> AccordionOptions {
        AccordionOptions {
            variant: self.variant,
            chevron: self.chevron,
            multiple: self.multiple,
            focusable: self.focusable,
        }
    }
}

#[derive(IntoElement)]
struct AccordionDocument {
    key: ElementId,
    items: Value,
    value: Option<Vec<usize>>,
    default_open: Vec<usize>,
    options: Options,
    on_change: Option<EventHandler>,
}

impl AccordionDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            items: node.prop(0).clone(),
            value: node
                .prop(1)
                .list_len()
                .map(|_| disclosure_data::indices(node.prop(1))),
            default_open: disclosure_data::indices(node.prop(2)),
            options: Options {
                variant: match node.prop(3).as_str() {
                    Some("separated") => AccordionVariant::Separated,
                    Some("filled") => AccordionVariant::Filled,
                    _ => AccordionVariant::Contained,
                },
                chevron: if node.prop(4).as_str() == Some("start") {
                    ChevronPosition::Start
                } else {
                    ChevronPosition::End
                },
                multiple: node.prop(5).as_bool().unwrap_or_default(),
                focusable: node.prop(6).as_bool(),
            },
            on_change: node.handler(0),
        }
    }
}

struct AccordionState {
    accordion: Entity<Accordion>,
    items: Value,
    options: Options,
    value: Option<Vec<usize>>,
    handler: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for AccordionDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let accordion = Accordion::new(
                disclosure_data::items(&self.items),
                self.options.native(),
                self.value.clone().unwrap_or(self.default_open),
                cx,
            );
            let subscription = cx.subscribe_in(
                &accordion,
                window,
                |state: &mut AccordionState, _, event: &AccordionChange, window, cx| {
                    if let Some(handler) = &state.handler {
                        handler(
                            Value::List(
                                event
                                    .0
                                    .iter()
                                    .map(|index| Value::Num(*index as f64))
                                    .collect::<Vec<_>>()
                                    .into(),
                            ),
                            window,
                            cx,
                        );
                    }
                },
            );
            AccordionState {
                accordion,
                items: self.items.clone(),
                options: self.options,
                value: None,
                handler: None,
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handler = self.on_change;
            let items_changed = state.items != self.items;
            state.accordion.update(cx, |accordion, cx| {
                if items_changed {
                    state.items = self.items;
                    accordion.items = disclosure_data::items(&state.items);
                    accordion.progress = vec![
                        Animated::new(0., Timing::Spring(Spring::SNAPPY));
                        accordion.items.len()
                    ];
                }
                if state.options != self.options || items_changed {
                    state.options = self.options;
                    accordion.set_options(state.options.native(), cx);
                }
                if self.value != state.value || items_changed {
                    state.value = self.value;
                    accordion.set_controlled(state.value.clone(), cx);
                }
            });
            state.accordion.clone()
        })
    }
}
