//! DataUi adapters for inputs.

use std::collections::BTreeSet;

use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
};
use guiraffe_core::{
    ColorName,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_spec::Schema,
    widget_value::Value,
};

use super::{ChipEvent, ChipGroup, PasswordStrength};
use crate::widget_specs;

pub(super) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: &Schema {
                    props: &["password"],
                    events: &[],
                },
                render: |node, _, _, _| {
                    PasswordStrength::new(node.prop(0).as_str().unwrap_or_default())
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[1],
                schema: &Schema {
                    props: &[
                        "options",
                        "multiple",
                        "value",
                        "disabled",
                        "focusable",
                        "color",
                    ],
                    events: &["change"],
                },
                render: |node, _, _, _| ChipDocument::from_node(node).into_any_element(),
            },
        ],
    );
}

fn values(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

#[derive(IntoElement)]
struct ChipDocument {
    key: ElementId,
    options: Vec<SharedString>,
    multiple: bool,
    selection: Option<BTreeSet<usize>>,
    disabled: bool,
    focusable: Option<bool>,
    color: Option<ColorName>,
    on_change: Option<EventHandler>,
}

impl ChipDocument {
    fn from_node(node: &dyn WidgetNode) -> Self {
        let selection = node.prop(2);
        Self {
            key: node.state_id(),
            options: values(node.prop(0))
                .filter_map(Value::as_str)
                .map(|s| s.to_owned().into())
                .collect(),
            multiple: node.prop(1).as_bool().unwrap_or_default(),
            selection: selection.list_len().map(|_| {
                values(selection)
                    .filter_map(Value::as_f64)
                    .filter(|v| *v >= 0.0 && v.fract() == 0.0 && *v < usize::MAX as f64)
                    .map(|v| v as usize)
                    .collect()
            }),
            disabled: node.prop(3).as_bool().unwrap_or_default(),
            focusable: node.prop(4).as_bool(),
            color: node.color(5),
            on_change: node.handler(0),
        }
    }
}

struct ChipDocumentState {
    control: Entity<ChipGroup>,
    on_change: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for ChipDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let control = window.use_keyed_state(self.key, cx, |window, cx| {
            let control = ChipGroup::new(self.options.clone(), self.multiple, cx);
            let subscription = cx.subscribe_in(
                &control,
                window,
                |state: &mut ChipDocumentState, _, event, window, cx| {
                    let ChipEvent::Changed(indices) = event;
                    if let Some(handler) = &state.on_change {
                        let value =
                            Value::List(indices.iter().map(|i| Value::Num(*i as f64)).collect());
                        handler(value, window, cx);
                    }
                },
            );
            ChipDocumentState {
                control,
                on_change: None,
                _subscription: subscription,
            }
        });
        let control = control.update(cx, |state, _| {
            state.on_change = self.on_change;
            state.control.clone()
        });
        control.update(cx, |chip, cx| {
            let mut changed = chip.disabled != self.disabled
                || chip.focusable != self.focusable
                || chip.color != self.color
                || chip.labels != self.options
                || chip.selection.multiple != self.multiple;
            if chip.labels != self.options {
                chip.labels = self.options;
                chip.text = chip.labels.clone();
                chip.checked_text = chip
                    .labels
                    .iter()
                    .map(|label| format!("✓ {label}").into())
                    .collect();
            }
            chip.disabled = self.disabled;
            chip.focusable = self.focusable;
            chip.color = self.color;
            chip.selection.multiple = self.multiple;
            let mut selection = self
                .selection
                .unwrap_or_else(|| chip.selection.selected.clone());
            selection.retain(|index| *index < chip.labels.len());
            if !self.multiple && selection.len() > 1 {
                selection = selection.into_iter().take(1).collect();
            }
            if chip.selection.selected != selection {
                chip.selection.selected = selection;
                changed = true;
            }
            if changed {
                cx.notify();
            }
        });
        control
    }
}
