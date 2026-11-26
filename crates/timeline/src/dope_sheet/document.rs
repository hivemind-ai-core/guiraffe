//! DataUi adapter for the keyframe timing editor.

use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    ColorName,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{Channel, DopeEvent, DopeSheet, LockStyle, PlotSurface, TimeFormat};
use crate::{Interp, Key, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::DOPE_SHEET,
            schema: guiraffe_core::widget_schema!(&widget_specs::DOPE_SHEET),
            render: |node, _, _, _| DopeDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

fn channel(value: &Value, remaining_depth: usize) -> Option<Channel> {
    let name = field(value, "name").as_str()?;
    let keys = field(value, "keyframes");
    let mut result = if keys.list_len().is_some() {
        Channel::with_keyframes(
            name,
            items(keys)
                .filter_map(|key| {
                    let time = field(key, "time")
                        .as_f64()
                        .filter(|time| time.is_finite())?;
                    let value = field(key, "value")
                        .as_f64()
                        .filter(|value| value.is_finite())?;
                    let interpolation = field(key, "interpolation")
                        .as_str()
                        .and_then(Interp::from_name)
                        .unwrap_or(Interp::Linear);
                    Some(Key::new(time, value, interpolation))
                })
                .collect(),
        )
    } else {
        Channel::new(
            name,
            items(field(value, "keys"))
                .filter_map(Value::as_f64)
                .collect(),
        )
    };
    if remaining_depth > 0 {
        result.children = items(field(value, "children"))
            .filter_map(|item| channel(item, remaining_depth - 1))
            .collect();
    }
    result.group =
        field(value, "group").as_bool().unwrap_or_default() || !result.children.is_empty();
    result.color = field(value, "color")
        .as_str()
        .and_then(ColorName::from_name)
        .unwrap_or(ColorName::Blue);
    result.collapsed = field(value, "collapsed").as_bool().unwrap_or_default();
    result.locked = field(value, "locked").as_bool().unwrap_or_default();
    result.muted = field(value, "muted").as_bool().unwrap_or_default();
    result.visible = field(value, "visible").as_bool().unwrap_or(true);
    Some(result)
}

fn parse_channels(value: &Value) -> Vec<Channel> {
    items(value).filter_map(|item| channel(item, 8)).collect()
}

fn record(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn flags(channel: &Channel, index: usize) -> [(&'static str, Value); 5] {
    [
        ("channel", Value::Num(index as f64)),
        ("collapsed", channel.collapsed.into()),
        ("locked", channel.locked.into()),
        ("muted", channel.muted.into()),
        ("visible", channel.visible.into()),
    ]
}

#[derive(IntoElement)]
struct DopeDocument {
    key: ElementId,
    channels: Value,
    format: TimeFormat,
    length: f64,
    editable: bool,
    focusable: Option<bool>,
    lock_style: LockStyle,
    playhead: Option<f64>,
    handlers: [Option<EventHandler>; 4],
}

impl DopeDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            channels: node.prop(0).clone(),
            format: match node.prop(1).as_str() {
                Some("frames") => {
                    TimeFormat::Frames(node.prop(2).as_f64().unwrap_or(30.).clamp(1., 240.))
                }
                Some("days") => TimeFormat::Days,
                _ => TimeFormat::Seconds,
            },
            length: node
                .prop(3)
                .as_f64()
                .filter(|length| length.is_finite())
                .unwrap_or(10.)
                .max(0.001),
            editable: node.prop(4).as_bool().unwrap_or(true),
            focusable: node.prop(5).as_bool(),
            lock_style: match node.prop(6).as_str() {
                Some("none") => LockStyle::None,
                Some("dimmed") => LockStyle::Dimmed,
                Some("hatched") => LockStyle::Hatched,
                _ => LockStyle::Badge,
            },
            playhead: node.prop(7).as_f64().filter(|time| time.is_finite()),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct DopeDocumentState {
    sheet: Entity<DopeSheet>,
    channels: Value,
    length: f64,
    handlers: [Option<EventHandler>; 4],
    _subscription: Subscription,
}

impl RenderOnce for DopeDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let sheet =
                DopeSheet::new(parse_channels(&self.channels), self.format, self.length, cx);
            let subscription = cx.subscribe_in(
                &sheet,
                window,
                |state: &mut DopeDocumentState, sheet, event, window, cx| {
                    let index = match event {
                        DopeEvent::KeysRetimed { .. } => 0,
                        DopeEvent::SelectionChanged => 1,
                        DopeEvent::ChannelChanged { .. } => 3,
                        DopeEvent::Seek(_) => return,
                        _ => 2,
                    };
                    let Some(handler) = state.handlers[index].clone() else {
                        return;
                    };
                    let doc = sheet.read(cx).doc(cx);
                    let model = sheet.read(cx).model();
                    let value = match event {
                        // One `key-retime` per moved key, in display indices.
                        DopeEvent::KeysRetimed { keys, delta } => {
                            let moved: Vec<Value> = keys
                                .iter()
                                .filter_map(|k| {
                                    let channel = doc.index_of(k.channel)?;
                                    let to = doc.key(*k)?.key.t;
                                    Some(record([
                                        ("channel", Value::Num(channel as f64)),
                                        ("from", Value::Num(to - delta)),
                                        ("to", Value::Num(to)),
                                    ]))
                                })
                                .collect();
                            for value in moved {
                                handler(value, window, cx);
                            }
                            return;
                        }
                        DopeEvent::SelectionChanged => Value::List(
                            model
                                .selected
                                .iter()
                                .map(|(channel, time)| {
                                    record([
                                        ("channel", Value::Num(*channel as f64)),
                                        ("time", Value::Num(f64::from_bits(*time))),
                                    ])
                                })
                                .collect::<Vec<_>>()
                                .into(),
                        ),
                        _ if index == 2 => Value::List(
                            model
                                .channels
                                .iter()
                                .enumerate()
                                .map(|(index, channel)| {
                                    record(
                                        flags(channel, index).into_iter().chain([(
                                            "keys",
                                            Value::List(
                                                channel
                                                    .keys()
                                                    .iter()
                                                    .copied()
                                                    .map(Value::Num)
                                                    .collect::<Vec<_>>()
                                                    .into(),
                                            ),
                                        )]),
                                    )
                                })
                                .collect::<Vec<_>>()
                                .into(),
                        ),
                        DopeEvent::ChannelChanged { channel, .. } => {
                            let Some(index) = doc.index_of(*channel) else {
                                return;
                            };
                            let Some(channel_data) = model.channels.get(index) else {
                                return;
                            };
                            record(flags(channel_data, index))
                        }
                        _ => return,
                    };
                    handler(value, window, cx);
                },
            );
            DopeDocumentState {
                sheet,
                channels: self.channels.clone(),
                length: self.length,
                handlers: [None, None, None, None],
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            state.sheet.update(cx, |sheet, cx| {
                if state.channels != self.channels {
                    state.channels = self.channels;
                    let doc = super::doc_from_channels(parse_channels(&state.channels));
                    let channels = doc.channels().iter().map(|c| (**c).clone()).collect();
                    sheet.drag = None;
                    sheet
                        .store()
                        .dispatch_now(crate::dope_store::SetChannels(channels), cx);
                    sheet.resync(cx);
                    cx.notify();
                }
                if sheet.format != self.format {
                    sheet.format = self.format;
                    sheet.surface = PlotSurface::timeline(*sheet.axis(), self.format);
                    cx.notify();
                }
                if state.length != self.length {
                    state.length = self.length;
                    sheet.pending_fit = Some((0., self.length));
                    cx.notify();
                }
                if sheet.editable != self.editable {
                    sheet.set_editable(self.editable, cx);
                }
                if sheet.focusable != self.focusable {
                    sheet.focusable = self.focusable;
                    cx.notify();
                }
                if sheet.lock_style != self.lock_style {
                    sheet.set_lock_style(self.lock_style, cx);
                }
                if let Some(playhead) = self.playhead
                    && sheet.playhead(cx) != playhead
                {
                    sheet
                        .store()
                        .dispatch_now(crate::dope_store::SetPlayhead(playhead), cx);
                    cx.notify();
                }
            });
            state.sheet.clone()
        })
    }
}
