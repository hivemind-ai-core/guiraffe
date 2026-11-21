//! DataUi adapters for inputs.

use std::collections::BTreeSet;

use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
};
use guiraffe_core::{
    ColorName,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{
    ChipEvent, ChipGroup, DropEvent, Dropzone, PasswordStrength, Range2, RangeEvent, RangeSlider,
    Thumb, aria_value,
};
use super::{TagRejected, TagRules, TagsEvent, TagsInput, accept_tag};
use crate::widget_specs;

pub(super) fn register(cx: &mut App) {
    calendar::register(cx);
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
                render: |node, _, _, _| {
                    PasswordStrength::new(node.prop(0).as_str().unwrap_or_default())
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[1],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
                render: |node, _, _, _| ChipDocument::from_node(node).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[2],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[2]),
                render: |node, _, _, _| RangeDocument::from_node(node).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[3],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[3]),
                render: |node, _, _, _| DropDocument::from_node(node).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[4],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[4]),
                render: |node, _, _, _| TagsDocument::from_node(node).into_any_element(),
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

#[derive(IntoElement)]
struct RangeDocument {
    key: ElementId,
    bounds: (f32, f32),
    step: f32,
    distance: f32,
    value: Option<(f32, f32)>,
    label: SharedString,
    disabled: bool,
    focusable: Option<bool>,
    handler: Option<EventHandler>,
}

impl RangeDocument {
    fn from_node(node: &dyn WidgetNode) -> Self {
        let number = |i, default| node.prop(i).as_f64().map_or(default, |v| v as f32);
        let value = node.prop(4);
        let endpoint = |i| value.list_get(i).and_then(Value::as_f64).map(|v| v as f32);
        Self {
            key: node.state_id(),
            bounds: (number(0, 0.0), number(1, 100.0)),
            step: number(2, 1.0),
            distance: number(3, 0.0),
            value: endpoint(0).zip(endpoint(1)),
            label: node.prop(5).as_str().unwrap_or("Range").to_owned().into(),
            disabled: node.prop(6).as_bool().unwrap_or_default(),
            focusable: node.prop(7).as_bool(),
            handler: node.handler(0),
        }
    }

    fn range(&self, (low, high): (f32, f32)) -> Range2 {
        let mut range = Range2::new(self.bounds.0, self.bounds.1, self.step, low, high);
        range.min_distance = if self.distance.is_finite() {
            self.distance.clamp(0.0, range.max - range.min)
        } else {
            0.0
        };
        range.set(Thumb::High, high);
        range.set(Thumb::Low, low);
        range
    }
}

struct RangeDocumentState {
    range: Entity<RangeSlider>,
    handler: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for RangeDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let initial = self.range(self.value.unwrap_or(self.bounds));
        let state = window.use_keyed_state(self.key.clone(), cx, |window, cx| {
            let range = RangeSlider::new(initial, cx);
            let subscription = cx.subscribe_in(
                &range,
                window,
                |state: &mut RangeDocumentState, _, event, window, cx| {
                    let RangeEvent::Changed(low, high) = *event;
                    if let Some(handler) = &state.handler {
                        handler(
                            Value::List(
                                vec![Value::Num(low.into()), Value::Num(high.into())].into(),
                            ),
                            window,
                            cx,
                        );
                    }
                },
            );
            RangeDocumentState {
                range,
                handler: None,
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.range.update(cx, |slider, cx| {
                let range = self.range(self.value.unwrap_or_else(|| slider.values()));
                if slider.range != range {
                    slider.range = range;
                    slider.aria_value = aria_value(&range);
                    cx.notify();
                }
                if slider.disabled != self.disabled {
                    slider.set_disabled(self.disabled, cx);
                }
                if slider.focusable != self.focusable {
                    slider.set_focusable(self.focusable, cx);
                }
                if slider.label != self.label {
                    slider.set_label(self.label.clone(), cx);
                }
            });
            state.handler = self.handler;
            state.range.clone()
        })
    }
}

#[derive(IntoElement)]
struct DropDocument {
    key: ElementId,
    accept: Vec<SharedString>,
    label: SharedString,
    disabled: bool,
    focusable: Option<bool>,
    handlers: [Option<EventHandler>; 3],
}

impl DropDocument {
    fn from_node(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            accept: values(node.prop(0))
                .filter_map(Value::as_str)
                .map(|s| s.to_owned().into())
                .collect(),
            label: node
                .prop(1)
                .as_str()
                .unwrap_or("Drop files")
                .to_owned()
                .into(),
            disabled: node.prop(2).as_bool().unwrap_or_default(),
            focusable: node.prop(3).as_bool(),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct DropDocumentState {
    dropzone: Entity<Dropzone>,
    handlers: [Option<EventHandler>; 3],
    _subscription: Subscription,
}

impl RenderOnce for DropDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let dropzone = Dropzone::new(vec![], self.label.clone(), cx);
            let subscription = cx.subscribe_in(
                &dropzone,
                window,
                |state: &mut DropDocumentState, _, event, window, cx| {
                    let (index, paths) = match event {
                        DropEvent::Accepted(paths) => (0, Some(paths)),
                        DropEvent::Rejected(paths) => (1, Some(paths)),
                        DropEvent::BrowseRequested => (2, None),
                    };
                    if let Some(handler) = &state.handlers[index] {
                        let payload = paths.map_or(Value::Null, |paths| {
                            Value::List(
                                paths
                                    .iter()
                                    .map(|path| path.to_string_lossy().as_ref().into())
                                    .collect(),
                            )
                        });
                        handler(payload, window, cx);
                    }
                },
            );
            DropDocumentState {
                dropzone,
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            state.dropzone.update(cx, |dropzone, cx| {
                let changed = dropzone.accept != self.accept || dropzone.label != self.label;
                dropzone.accept = self.accept;
                dropzone.label = self.label;
                if dropzone.disabled != self.disabled {
                    dropzone.set_disabled(self.disabled, cx);
                }
                if dropzone.focusable != self.focusable {
                    dropzone.set_focusable(self.focusable, cx);
                }
                if changed {
                    cx.notify();
                }
            });
            state.dropzone.clone()
        })
    }
}

#[derive(IntoElement)]
struct TagsDocument {
    key: ElementId,
    value: Option<Vec<String>>,
    rules: TagRules,
    placeholder: SharedString,
    handlers: [Option<EventHandler>; 2],
}

impl TagsDocument {
    fn from_node(node: &dyn WidgetNode) -> Self {
        let value = node.prop(0);
        let count = |i| {
            node.prop(i)
                .as_f64()
                .map(|v| v.clamp(0.0, 65536.0) as usize)
        };
        Self {
            key: node.state_id(),
            value: value.list_len().map(|_| {
                values(value)
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            }),
            rules: TagRules {
                allow_duplicates: node.prop(2).as_bool().unwrap_or_default(),
                split_chars: node.prop(3).as_str().unwrap_or(",").chars().collect(),
                max_tags: count(4),
                max_len: count(5).unwrap_or(64),
            },
            placeholder: node.prop(1).display(),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct TagsDocumentState {
    placeholder: SharedString,
    tags: Entity<TagsInput>,
    handlers: [Option<EventHandler>; 2],
    _subscription: Subscription,
}

impl RenderOnce for TagsDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let tags = TagsInput::new(self.rules.clone(), self.placeholder.clone(), window, cx);
            let subscription = cx.subscribe_in(
                &tags,
                window,
                |state: &mut TagsDocumentState, _, event, window, cx| match event {
                    TagsEvent::Changed(tags) => {
                        if let Some(handler) = &state.handlers[0] {
                            handler(
                                Value::List(tags.iter().map(|tag| tag.as_str().into()).collect()),
                                window,
                                cx,
                            );
                        }
                    }
                    TagsEvent::Rejected(tag, reason) => {
                        if let Some(handler) = &state.handlers[1] {
                            let reason = match reason {
                                TagRejected::Empty => "empty",
                                TagRejected::Duplicate => "duplicate",
                                TagRejected::Full => "full",
                                TagRejected::TooLong => "too-long",
                            };
                            let payload = Value::Record(std::sync::Arc::new(
                                [
                                    ("tag".into(), tag.as_str().into()),
                                    ("reason".into(), reason.into()),
                                ]
                                .into(),
                            ));
                            handler(payload, window, cx);
                        }
                    }
                },
            );
            TagsDocumentState {
                placeholder: self.placeholder.clone(),
                tags,
                handlers: [None, None],
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            let placeholder = (state.placeholder != self.placeholder).then(|| {
                state.placeholder = self.placeholder;
                state.placeholder.clone()
            });
            state.tags.update(cx, |tags, cx| {
                tags.rules = self.rules;
                if let Some(placeholder) = placeholder {
                    tags.input.update(cx, |input, cx| {
                        input.set_placeholder(placeholder, window, cx)
                    });
                }
                if let Some(value) = self.value
                    && tags.tags != value
                {
                    let mut accepted = Vec::with_capacity(value.len());
                    for tag in value {
                        if let Ok(tag) = accept_tag(&accepted, &tag, &tags.rules) {
                            accepted.push(tag);
                        }
                    }
                    if tags.tags != accepted {
                        tags.tags = accepted;
                        tags.tag_labels = tags.tags.iter().map(|tag| tag.as_str().into()).collect();
                        cx.notify();
                    }
                }
            });
            state.tags.clone()
        })
    }
}

mod calendar;
