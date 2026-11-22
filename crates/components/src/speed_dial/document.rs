//! Document adapter for the native animated action dial.

use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
    assets::IconName,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{DialAction, DialArc, DialLayout, SpeedDial, SpeedDialEvent};
use crate::widget_specs;

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::SPEED_DIAL,
            schema: guiraffe_core::widget_schema!(&widget_specs::SPEED_DIAL),
            render: |node, _, _, _| DocumentDial::new(node).into_any_element(),
        }],
    );
}

fn named_icon(path: Option<&str>) -> IconName {
    path.and_then(|path| {
        IconName::ALL
            .iter()
            .copied()
            .find(|icon| icon.path().as_ref() == path)
    })
    .unwrap_or(IconName::Plus)
}

fn actions(value: &Value) -> Vec<DialAction> {
    let icon = ["icon".into()];
    let label = ["label".into()];
    (0..value.list_len().unwrap_or_default())
        .filter_map(|index| {
            let item = value.list_get(index)?;
            Some(DialAction {
                icon: named_icon(item.field(&icon).as_str()),
                label: item.field(&label).as_str()?.to_owned().into(),
            })
        })
        .collect()
}

#[derive(IntoElement)]
struct DocumentDial {
    key: ElementId,
    primary: SharedString,
    label: SharedString,
    actions: Value,
    layout: DialLayout,
    open: Option<bool>,
    focusable: Option<bool>,
    show_labels: bool,
    handlers: [Option<EventHandler>; 2],
}

impl DocumentDial {
    fn new(node: &dyn WidgetNode) -> Self {
        let number = |index, default| {
            node.prop(index)
                .as_f64()
                .map_or(default, |value| value as f32)
        };
        let spacing = number(4, 48.);
        let layout = match node.prop(3).as_str() {
            Some("horizontal") => DialLayout::Horizontal { spacing },
            Some("radial") => DialLayout::Radial {
                radius: number(5, 64.),
                start_degrees: number(6, 270.),
                arc: if node.prop(7).as_str() == Some("half") {
                    DialArc::Half
                } else {
                    DialArc::Quarter
                },
                clockwise: node.prop(8).as_bool().unwrap_or(true),
            },
            _ => DialLayout::Vertical { spacing },
        };
        Self {
            key: node.state_id(),
            primary: node
                .prop(0)
                .as_str()
                .unwrap_or("icons/plus.svg")
                .to_owned()
                .into(),
            label: node
                .prop(1)
                .as_str()
                .unwrap_or("More actions")
                .to_owned()
                .into(),
            actions: node.prop(2).clone(),
            layout,
            open: node.prop(9).as_bool(),
            focusable: node.prop(10).as_bool(),
            show_labels: node.prop(11).as_bool().unwrap_or(false),
            handlers: [node.handler(0), node.handler(1)],
        }
    }
}

struct State {
    dial: Entity<SpeedDial>,
    actions: Value,
    layout: DialLayout,
    primary: SharedString,
    open: Option<bool>,
    handlers: [Option<EventHandler>; 2],
    _subscription: Subscription,
}

impl RenderOnce for DocumentDial {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let dial = SpeedDial::new(
                named_icon(Some(&self.primary)),
                self.label.clone(),
                actions(&self.actions),
                self.layout,
                cx,
            );
            let subscription = cx.subscribe_in(
                &dial,
                window,
                |state: &mut State, _, event: &SpeedDialEvent, window, cx| {
                    let (index, payload) = match *event {
                        SpeedDialEvent::Open(open) => (0, Value::Bool(open)),
                        SpeedDialEvent::Action(index) => (1, Value::Num(index as f64)),
                    };
                    if let Some(handler) = state.handlers[index].clone() {
                        handler(payload, window, cx);
                    }
                },
            );
            State {
                dial,
                actions: self.actions.clone(),
                layout: self.layout,
                primary: self.primary.clone(),
                open: None,
                handlers: [None, None],
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            state.dial.update(cx, |dial, cx| {
                if state.primary != self.primary || dial.label != self.label {
                    dial.primary = named_icon(Some(&self.primary));
                    dial.label = self.label;
                    state.primary = self.primary;
                    cx.notify();
                }
                if state.actions != self.actions {
                    dial.actions = actions(&self.actions);
                    let target = if dial.open { 1. } else { 0. };
                    dial.springs = vec![
                        super::Animated::new(
                            target,
                            super::Timing::Spring(super::Spring::SNAPPY)
                        );
                        dial.actions.len()
                    ];
                    state.actions = self.actions;
                    dial.set_layout(self.layout, cx);
                } else if state.layout != self.layout {
                    dial.set_layout(self.layout, cx);
                }
                state.layout = self.layout;
                dial.set_show_labels(self.show_labels, cx);
                if dial.focusable != self.focusable {
                    dial.set_focusable(self.focusable, cx);
                }
                if state.open != self.open {
                    if let Some(open) = self.open {
                        dial.sync_open(open, cx);
                    }
                    state.open = self.open;
                }
            });
            state.dial.clone()
        })
    }
}
