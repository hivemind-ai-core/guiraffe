//! DataUi adapter for the editable clip timeline.

use std::{collections::HashSet, sync::Arc};

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_spec::Schema,
    widget_value::Value,
};

use super::{TracksEvent, TracksView};
use crate::{Clip, LockStyle, ResizeEdges, TimeFormat, Track, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: &Schema {
                props: &[
                    "tracks",
                    "time-format",
                    "frames-per-second",
                    "length",
                    "playhead",
                    "snap",
                    "resize-edges",
                    "editable",
                    "focusable",
                    "lock-style",
                    "global-lock-style",
                    "selected",
                ],
                events: &["clip-change", "seek"],
            },
            render: |node, _, _, _| TrackDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

fn uint(value: &Value) -> Option<u32> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v <= u32::MAX as f64 && v.fract() == 0.0)
        .map(|v| v as u32)
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

#[derive(Default)]
struct Tracks {
    tracks: Vec<(String, Track)>,
    locks: HashSet<(usize, u32)>,
}

impl Tracks {
    fn parse(values: &Value) -> Self {
        let mut result = Self::default();
        for value in items(values) {
            let Some(name) = field(value, "name").as_str() else {
                continue;
            };
            let mut clips = Vec::new();
            let mut ids = HashSet::new();
            for entry in items(field(value, "clips")) {
                let Some((id, start, len)) = uint(field(entry, "id"))
                    .zip(field(entry, "start").as_f64())
                    .zip(field(entry, "length").as_f64())
                    .map(|((id, start), len)| (id, start, len))
                else {
                    continue;
                };
                let offset = field(entry, "sourceOffset").as_f64().unwrap_or_default();
                if id == u32::MAX
                    || !ids.insert(id)
                    || !start.is_finite()
                    || !len.is_finite()
                    || !(start + len).is_finite()
                    || len < crate::clips::MIN_LEN
                    || start + len <= start
                    || !offset.is_finite()
                    || offset < 0.0
                {
                    continue;
                }
                clips.push((
                    Clip {
                        id,
                        start,
                        len,
                        source_offset: offset,
                    },
                    field(entry, "locked").as_bool().unwrap_or_default(),
                ));
            }
            clips.sort_by(|a, b| a.0.start.total_cmp(&b.0.start));
            let mut end = f64::NEG_INFINITY;
            let track = result.tracks.len();
            let clips = clips
                .into_iter()
                .filter_map(|(clip, locked)| {
                    if clip.start < end - 1e-12 {
                        return None;
                    }
                    end = clip.end();
                    if locked {
                        result.locks.insert((track, clip.id));
                    }
                    Some(clip)
                })
                .collect();
            result
                .tracks
                .push((name.to_owned(), Track::from_validated_clips(clips)));
        }
        result
    }
}

#[derive(IntoElement)]
struct TrackDocument {
    key: ElementId,
    tracks: Value,
    format: TimeFormat,
    length: f64,
    playhead: Option<f64>,
    snap: Option<f64>,
    resize: ResizeEdges,
    editable: bool,
    focusable: Option<bool>,
    lock_style: LockStyle,
    global_lock_style: LockStyle,
    selection: Option<Option<(usize, u32)>>,
    handlers: [Option<EventHandler>; 2],
}

impl TrackDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        fn style(value: &Value, default: LockStyle) -> LockStyle {
            match value.as_str() {
                Some("none") => LockStyle::None,
                Some("badge") => LockStyle::Badge,
                Some("dimmed") => LockStyle::Dimmed,
                Some("hatched") => LockStyle::Hatched,
                _ => default,
            }
        }
        let selection = node.prop(11);
        Self {
            key: node.state_id(),
            tracks: node.prop(0).clone(),
            format: match node.prop(1).as_str() {
                Some("frames") => {
                    TimeFormat::Frames(node.prop(2).as_f64().unwrap_or(30.0).clamp(1.0, 240.0))
                }
                Some("days") => TimeFormat::Days,
                _ => TimeFormat::Seconds,
            },
            length: node.prop(3).as_f64().unwrap_or(10.0).max(1.0),
            playhead: node
                .prop(4)
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| v.max(0.0)),
            snap: node.prop(5).as_f64(),
            resize: match node.prop(6).as_str() {
                Some("none") => ResizeEdges::None,
                Some("start") => ResizeEdges::Start,
                Some("end") => ResizeEdges::End,
                _ => ResizeEdges::Both,
            },
            editable: node.prop(7).as_bool().unwrap_or(true),
            focusable: node.prop(8).as_bool(),
            lock_style: style(node.prop(9), LockStyle::Badge),
            global_lock_style: style(node.prop(10), LockStyle::None),
            selection: selection.list_len().map(|_| {
                selection
                    .list_get(0)
                    .and_then(uint)
                    .map(|track| track as usize)
                    .zip(selection.list_get(1).and_then(uint))
            }),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct TrackState {
    timeline: Entity<TracksView>,
    tracks: Value,
    format: TimeFormat,
    length: f64,
    handlers: [Option<EventHandler>; 2],
    _subscription: Subscription,
}

impl RenderOnce for TrackDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let source = Tracks::parse(&self.tracks);
            let timeline = TracksView::new(source.tracks, self.format, self.length, cx);
            timeline.update(cx, |timeline, _| timeline.locked = source.locks);
            let subscription = cx.subscribe_in(
                &timeline,
                window,
                |state: &mut TrackState, timeline, event, window, cx| {
                    let (index, value) =
                        match event {
                            TracksEvent::ClipChanged { track, clip } => {
                                let entry = timeline.read(cx).tracks().get(*track).and_then(
                                    |(_, track)| track.clips().iter().find(|c| c.id == *clip),
                                );
                                let mut fields = std::collections::BTreeMap::from([
                                    ("track".into(), Value::Num(*track as f64)),
                                    ("clip".into(), Value::Num(f64::from(*clip))),
                                ]);
                                if let Some(entry) = entry {
                                    fields.insert("start".into(), Value::Num(entry.start));
                                    fields.insert("length".into(), Value::Num(entry.len));
                                    fields.insert(
                                        "sourceOffset".into(),
                                        Value::Num(entry.source_offset),
                                    );
                                }
                                (0, Value::Record(Arc::new(fields)))
                            }
                            TracksEvent::Seek(time) => (1, Value::Num(*time)),
                        };
                    if let Some(handler) = &state.handlers[index] {
                        handler(value, window, cx);
                    }
                },
            );
            TrackState {
                timeline,
                tracks: self.tracks.clone(),
                format: self.format,
                length: self.length,
                handlers: [None, None],
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            let source = (state.tracks != self.tracks).then(|| {
                state.tracks = self.tracks;
                Tracks::parse(&state.tracks)
            });
            state.timeline.update(cx, |timeline, cx| {
                if state.format != self.format || state.length != self.length {
                    if state.format != self.format {
                        timeline.surface =
                            crate::PlotSurface::timeline(*timeline.axis(), self.format);
                        timeline.format = self.format;
                        state.format = self.format;
                    }
                    if state.length != self.length {
                        timeline.pending_fit = Some((0.0, self.length));
                        state.length = self.length;
                    }
                    cx.notify();
                }
                if let Some(source) = source {
                    let selected = timeline.selected();
                    timeline.tracks = source.tracks;
                    timeline.locked = source.locks;
                    timeline.release();
                    timeline.select(selected, cx);
                    cx.notify();
                }
                if let Some(playhead) = self.playhead
                    && timeline.playhead != playhead
                {
                    timeline.playhead = playhead;
                    cx.notify();
                }
                timeline.set_snap(self.snap);
                if timeline.resize != self.resize {
                    timeline.set_resize_edges(self.resize, cx);
                }
                timeline.set_editable(self.editable, cx);
                if timeline.focusable != self.focusable {
                    timeline.set_focusable(self.focusable, cx);
                }
                if timeline.lock_style != self.lock_style
                    || timeline.global_lock_style != self.global_lock_style
                {
                    timeline.set_lock_style(self.lock_style, self.global_lock_style, cx);
                }
                if let Some(selection) = self.selection {
                    timeline.select(selection, cx);
                }
            });
            state.timeline.clone()
        })
    }
}
