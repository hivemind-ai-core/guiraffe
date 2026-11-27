//! The `TracksView` model: track rows with heights, the selection, the playhead, follow mode
//! and the view range. It lives in a [`Store`], so hosts write it with store events and
//! users change it with bus actions ([`TracksAction`]). GPUI-free apart from `SharedString`.
//!
//! - **Host writes** are store events ([`SetTracks`], [`SetPlayhead`], [`SetView`],
//!   [`SetFollow`], [`SetTrackHeight`], [`SetSelection`]). They never reach the bus or
//!   emit a `TracksEvent`, so a host never sees its own write come back as a user action.
//! - **User edits** are [`TracksAction`]s applied through the control protocol, from the
//!   pointer, the keyboard or any bus client alike.
//! - **Undo** covers track content and heights (see [`tracks_store`]); selection, playhead
//!   and view changes are not undoable.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

use gpui_kit::{App, SharedString};
use guiraffe_core::{
    protocol::{Action, ActionError},
    widget_value::Value,
};
use guiraffe_state::{Event, Fx, HistoryLimits, Store};

use crate::{
    axis::Axis,
    clips::{Clip, Edge, Track},
};

/// Height of a track row unless set.
pub const DEFAULT_TRACK_HEIGHT: f32 = 40.0;
/// Smallest and largest row height a resize accepts.
pub const MIN_TRACK_HEIGHT: f32 = 20.0;
pub const MAX_TRACK_HEIGHT: f32 = 800.0;

/// Clamp a requested height; a non-finite one becomes the default.
pub fn clamp_height(height: f32) -> f32 {
    if height.is_finite() {
        height.clamp(MIN_TRACK_HEIGHT, MAX_TRACK_HEIGHT)
    } else {
        DEFAULT_TRACK_HEIGHT
    }
}

/// A track's stable key, provided by the host (Passage's track id). Rows, heights, the
/// selection and actions refer to tracks by it, so inserting, removing or reordering
/// tracks never moves state onto a neighbour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(pub u64);

impl From<u64> for TrackId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// One track: its key, name, clips and row height.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    pub id: TrackId,
    pub name: SharedString,
    pub track: Track,
    pub height: f32,
}

impl TrackRow {
    pub fn new(id: impl Into<TrackId>, name: impl Into<SharedString>, track: Track) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            track,
            height: DEFAULT_TRACK_HEIGHT,
        }
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.height = clamp_height(height);
        self
    }
}

/// A clip by track key and clip id (both stable; never positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClipRef {
    pub track: TrackId,
    pub clip: u32,
}

impl ClipRef {
    pub fn new(track: impl Into<TrackId>, clip: u32) -> Self {
        Self {
            track: track.into(),
            clip,
        }
    }
}

/// Keep the playhead visible while it moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Follow {
    #[default]
    Off,
    /// Turn the page when the playhead leaves the view.
    Page,
    /// Keep the playhead centred.
    Center,
}

/// The visible time range: the time at the left edge and seconds per pixel. Independent of
/// the view width, so it survives resizes and rebuilds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewRange {
    pub start: f64,
    pub scale: f64,
}

impl ViewRange {
    pub fn axis(self, width: f64) -> Axis {
        let mut axis = Axis::new(0.0, 1.0, width);
        axis.scale = self.scale.clamp(axis.min_scale, axis.max_scale);
        axis.start = self.start;
        axis
    }
}

/// Row tops as a prefix sum of heights. Hit testing is a binary search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowLayout {
    /// `tops[i]` is row `i`'s top; the last entry is the total height.
    tops: Vec<f32>,
}

impl RowLayout {
    pub fn new(heights: impl IntoIterator<Item = f32>) -> Self {
        let mut layout = Self::default();
        layout.rebuild(heights);
        layout
    }

    /// Recompute in place, reusing the allocation.
    pub fn rebuild(&mut self, heights: impl IntoIterator<Item = f32>) {
        self.tops.clear();
        self.tops.push(0.0);
        let mut y = 0.0;
        for h in heights {
            y += h;
            self.tops.push(y);
        }
    }

    pub fn len(&self) -> usize {
        self.tops.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn top(&self, row: usize) -> f32 {
        self.tops.get(row).copied().unwrap_or_else(|| self.total())
    }

    pub fn height(&self, row: usize) -> f32 {
        match (self.tops.get(row), self.tops.get(row + 1)) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        }
    }

    pub fn total(&self) -> f32 {
        self.tops.last().copied().unwrap_or_default()
    }

    /// The row containing content y `y`.
    pub fn row_at(&self, y: f32) -> Option<usize> {
        if !(y >= 0.0 && y < self.total()) {
            return None;
        }
        Some(self.tops.partition_point(|&top| top <= y) - 1)
    }

    /// Rows overlapping content ys `top..bottom`.
    pub fn rows_in(&self, top: f32, bottom: f32) -> Range<usize> {
        let n = self.len();
        let first = self
            .tops
            .partition_point(|&t| t <= top)
            .saturating_sub(1)
            .min(n);
        let last = self.tops.partition_point(|&t| t < bottom).min(n);
        first..last.max(first)
    }

    /// The row whose bottom edge is within `grab` pixels of content y `y`.
    pub fn bottom_edge_at(&self, y: f32, grab: f32) -> Option<usize> {
        let i = self.tops.partition_point(|&t| t < y - grab);
        (i >= 1 && i < self.tops.len() && (self.tops[i] - y).abs() <= grab).then(|| i - 1)
    }
}

/// The model a `TracksView` renders. Cheap to clone: rows are shared.
#[derive(Debug, Clone)]
pub struct TracksModel {
    rows: Arc<Vec<Arc<TrackRow>>>,
    pub selection: Option<ClipRef>,
    additional: Arc<BTreeSet<ClipRef>>,
    pub playhead: f64,
    pub follow: Follow,
    /// `None` until the view fits the initial range.
    pub view: Option<ViewRange>,
    /// Width the view last laid out at, for follow mode.
    pub viewport: f64,
    /// Lowest time the view scrolls to, if any.
    pub min_time: Option<f64>,
}

/// Shared rows compare by pointer first, so a store flush after an edit to one row
/// compares one row's clips, not every track's.
impl PartialEq for TracksModel {
    fn eq(&self, other: &Self) -> bool {
        let rows = Arc::ptr_eq(&self.rows, &other.rows)
            || (self.rows.len() == other.rows.len()
                && self
                    .rows
                    .iter()
                    .zip(other.rows.iter())
                    .all(|(a, b)| Arc::ptr_eq(a, b) || a == b));
        rows && self.selection == other.selection
            && (Arc::ptr_eq(&self.additional, &other.additional)
                || self.additional == other.additional)
            && self.playhead == other.playhead
            && self.follow == other.follow
            && self.view == other.view
            && self.viewport == other.viewport
            && self.min_time == other.min_time
    }
}

impl Default for TracksModel {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl TracksModel {
    pub fn new(rows: Vec<TrackRow>) -> Self {
        Self {
            rows: Arc::new(rows.into_iter().map(Arc::new).collect()),
            selection: None,
            additional: Default::default(),
            playhead: 0.0,
            follow: Follow::Off,
            view: None,
            viewport: 0.0,
            min_time: None,
        }
    }

    /// From `(name, track)` pairs at the default height, keyed by their position.
    pub fn from_tracks(tracks: Vec<(String, Track)>) -> Self {
        Self::new(
            tracks
                .into_iter()
                .enumerate()
                .map(|(i, (name, track))| TrackRow::new(i as u64, name, track))
                .collect(),
        )
    }

    pub fn rows(&self) -> &[Arc<TrackRow>] {
        &self.rows
    }

    /// The row at display position `index`.
    pub fn row(&self, index: usize) -> Option<&TrackRow> {
        self.rows.get(index).map(|r| &**r)
    }

    /// The display position of track `id`.
    pub fn index_of(&self, id: TrackId) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    /// The row with key `id`.
    pub fn row_by_id(&self, id: TrackId) -> Option<&TrackRow> {
        self.row(self.index_of(id)?)
    }

    pub fn clip(&self, at: ClipRef) -> Option<&Clip> {
        self.row_by_id(at.track)?.track.clip(at.clip)
    }

    /// Selected stable clip keys, with the primary keyboard target first.
    pub fn selected_clips(&self) -> impl Iterator<Item = ClipRef> + '_ {
        self.selection
            .into_iter()
            .chain(self.additional.iter().copied())
    }

    pub fn is_selected(&self, clip: ClipRef) -> bool {
        self.selection == Some(clip) || self.additional.contains(&clip)
    }

    pub(crate) fn selected_on(&self, track: TrackId) -> impl Iterator<Item = ClipRef> + '_ {
        let range = ClipRef::new(track, 0)..=ClipRef::new(track, u32::MAX);
        self.selection
            .filter(|at| at.track == track)
            .into_iter()
            .chain(self.additional.range(range).copied())
    }

    fn replace_selection(
        &mut self,
        clips: impl IntoIterator<Item = ClipRef>,
    ) -> Option<TracksChange> {
        let mut requested = clips.into_iter();
        let first = requested.next();
        let second = requested.next();
        // The common single-click path probes just one clip. For a marquee, scan
        // each requested row once instead of doing a linear ID lookup per clip.
        let mut members = BTreeSet::new();
        let selection = if second.is_none() {
            first.filter(|at| self.clip(*at).is_some())
        } else {
            let ordered: Vec<_> = first.into_iter().chain(second).chain(requested).collect();
            let wanted: BTreeSet<_> = ordered.iter().copied().collect();
            for row in self.rows() {
                if wanted
                    .range(ClipRef::new(row.id, 0)..=ClipRef::new(row.id, u32::MAX))
                    .next()
                    .is_none()
                {
                    continue;
                }
                members.extend(
                    row.track
                        .clips()
                        .iter()
                        .map(|c| ClipRef::new(row.id, c.id))
                        .filter(|at| wanted.contains(at)),
                );
            }
            ordered.into_iter().find(|at| members.contains(at))
        };
        if let Some(at) = selection {
            members.remove(&at);
        }
        let additional = members;
        if self.selection == selection && *self.additional == additional {
            return None;
        }
        self.selection = selection;
        self.additional = Arc::new(additional);
        Some(TracksChange::SelectionChanged { selection })
    }
    fn prune_selection(&mut self) {
        let remaining: Vec<_> = self.selected_clips().collect();
        self.replace_selection(remaining);
    }
    /// The selection as a display position and clip id.
    pub fn selected_index(&self) -> Option<(usize, u32)> {
        let s = self.selection?;
        Some((self.index_of(s.track)?, s.clip))
    }

    /// Whether the rows (content or heights) are the same allocation as `other`'s.
    pub fn same_rows(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.rows, &other.rows)
    }

    fn row_mut(&mut self, id: TrackId) -> Option<&mut TrackRow> {
        let index = self.index_of(id)?;
        Arc::make_mut(&mut self.rows)
            .get_mut(index)
            .map(Arc::make_mut)
    }

    /// Replace the tracks, diffing by [`TrackId`]: a row whose key and content are unchanged
    /// keeps its allocation, a known key keeps its height (unless `height` is given) wherever
    /// it moves, new keys get the default height, and removed keys take only their own state
    /// with them. The selection follows its track's key and survives while its clip exists.
    /// Zoom and scroll are untouched. Later duplicates of a key are ignored. Returns whether
    /// anything changed.
    pub fn update_tracks(&mut self, tracks: Vec<TrackUpdate>) -> bool {
        let same = tracks.len() == self.rows.len()
            && tracks
                .iter()
                .zip(self.rows.iter())
                .all(|(u, row)| u.matches(row));
        if same {
            return false;
        }
        let old: std::collections::HashMap<TrackId, &Arc<TrackRow>> =
            self.rows.iter().map(|r| (r.id, r)).collect();
        let mut seen = std::collections::HashSet::with_capacity(tracks.len());
        let rows = tracks
            .into_iter()
            .filter(|u| seen.insert(u.id))
            .map(|u| match old.get(&u.id) {
                Some(row) if u.matches(row) => Arc::clone(row),
                Some(row) => Arc::new(u.into_row(row.height)),
                None => Arc::new(u.into_row(DEFAULT_TRACK_HEIGHT)),
            })
            .collect();
        drop(old);
        self.rows = Arc::new(rows);
        self.prune_selection();
        true
    }

    /// Apply a user action. Returns what changed, or `None` for a no-op.
    pub fn apply(&mut self, action: TracksAction) -> Option<TracksChange> {
        match action {
            TracksAction::ResizeTrack { track, height } => {
                let height = clamp_height(height);
                let row = self.row_by_id(track)?;
                if row.height == height {
                    return None;
                }
                self.row_mut(track)?.height = height;
                Some(TracksChange::TrackResized { track, height })
            }
            TracksAction::MoveClip { clip, start } => {
                let next = self.row_by_id(clip.track)?.track.moved(clip.clip, start)?;
                self.set_clip(clip, next, ClipEdit::Move)
            }
            TracksAction::TrimClip { clip, edge, time } => {
                let next = self
                    .row_by_id(clip.track)?
                    .track
                    .trimmed(clip.clip, edge, time)?;
                self.set_clip(clip, next, ClipEdit::Trim(edge))
            }
            TracksAction::Select(selection) => self.replace_selection(selection),
            TracksAction::SelectMany(clips) => self.replace_selection(clips.iter().copied()),
            TracksAction::Seek(time) => {
                let time = self.clamp_time(time)?;
                (self.playhead != time).then(|| {
                    self.set_playhead(time);
                    TracksChange::Seek { time }
                })
            }
        }
    }

    fn set_clip(&mut self, at: ClipRef, next: Clip, edit: ClipEdit) -> Option<TracksChange> {
        if self.clip(at) == Some(&next) {
            return None;
        }
        let row = self.row_mut(at.track)?;
        match edit {
            ClipEdit::Move => row.track.move_clip(at.clip, next.start),
            ClipEdit::Trim(edge) => {
                let t = match edge {
                    Edge::Start => next.start,
                    Edge::End => next.end(),
                };
                row.track.trim(at.clip, edge, t)
            }
        };
        Some(TracksChange::ClipChanged {
            clip: at,
            edit,
            span: next,
        })
    }

    fn clamp_time(&self, time: f64) -> Option<f64> {
        time.is_finite()
            .then(|| time.max(self.min_time.unwrap_or(0.0)).max(0.0))
    }

    /// Move the playhead (no event) and follow it if follow mode is on.
    pub fn set_playhead(&mut self, time: f64) {
        let Some(time) = self.clamp_time(time) else {
            return;
        };
        self.playhead = time;
        let (Some(view), true) = (self.view, self.viewport > 0.0) else {
            return;
        };
        let span = view.scale * self.viewport;
        let start = match self.follow {
            Follow::Off => return,
            Follow::Page if time < view.start || time >= view.start + span => time,
            Follow::Page => return,
            Follow::Center => time - span / 2.0,
        };
        self.set_view(ViewRange { start, ..view });
    }

    /// Set the view range, honouring the minimum time.
    pub fn set_view(&mut self, view: ViewRange) {
        if !(view.start.is_finite() && view.scale.is_finite() && view.scale > 0.0) {
            return;
        }
        let start = self.min_time.map_or(view.start, |min| view.start.max(min));
        self.view = Some(ViewRange { start, ..view });
    }

    /// The axis for a view `width` pixels wide (fitting `0..length` if no range is set).
    pub fn axis(&self, width: f64, length: f64) -> Axis {
        match self.view {
            Some(view) => view.axis(width),
            None => Axis::new(0.0, length.max(1.0), width),
        }
    }
}

/// How a clip changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipEdit {
    Move,
    Trim(Edge),
}

/// One track in a [`SetTracks`] write, keyed by `id`. `height: None` keeps the row's
/// current height.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackUpdate {
    pub id: TrackId,
    pub name: SharedString,
    pub track: Track,
    pub height: Option<f32>,
}

impl TrackUpdate {
    pub fn new(id: impl Into<TrackId>, name: impl Into<SharedString>, track: Track) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            track,
            height: None,
        }
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    fn matches(&self, row: &TrackRow) -> bool {
        self.id == row.id
            && self.name == row.name
            && self.track == row.track
            && self.height.is_none_or(|h| clamp_height(h) == row.height)
    }

    fn into_row(self, height: f32) -> TrackRow {
        TrackRow {
            id: self.id,
            name: self.name,
            track: self.track,
            height: self.height.map_or(height, clamp_height),
        }
    }
}

/// A user edit: what pointer, keyboard and bus clients send. Erased form: see
/// [`TracksAction::decode`].
#[derive(Debug, Clone, PartialEq)]
pub enum TracksAction {
    /// Set a track's row height (clamped).
    ResizeTrack {
        track: TrackId,
        height: f32,
    },
    /// Move a clip to start at `start` (clamped by its neighbours).
    MoveClip {
        clip: ClipRef,
        start: f64,
    },
    /// Move one edge of a clip to `time`. The end may extend past the clip's content.
    TrimClip {
        clip: ClipRef,
        edge: Edge,
        time: f64,
    },
    /// Select a clip, or clear the selection.
    Select(Option<ClipRef>),
    SelectMany(Arc<[ClipRef]>),
    /// Move the playhead.
    Seek(f64),
}

/// What a [`TracksAction`] changed: the outcome bus subscribers and `TracksEvent`
/// listeners see.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TracksChange {
    TrackResized {
        track: TrackId,
        height: f32,
    },
    ClipChanged {
        clip: ClipRef,
        edit: ClipEdit,
        span: Clip,
    },
    SelectionChanged {
        selection: Option<ClipRef>,
    },
    Seek {
        time: f64,
    },
}

/// Erased action names (the payload is a record).
pub mod action_names {
    /// `{track, height}`
    pub const RESIZE_TRACK: &str = "resize_track";
    /// `{track, clip, start}`
    pub const MOVE_CLIP: &str = "move_clip";
    /// `{track, clip, edge: "start" | "end", time}`
    pub const TRIM_CLIP: &str = "trim_clip";
    /// `{track, clip}`, or `null` to clear.
    pub const SELECT: &str = guiraffe_core::protocol::names::SELECT;
    /// `{time}` or a number.
    pub const SEEK: &str = "seek";
}

fn record(fields: &[(&str, Value)]) -> Value {
    Value::Record(Arc::new(
        fields
            .iter()
            .map(|(k, v)| (SharedString::from(*k), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn num(value: &Value, name: &'static str) -> Result<f64, ActionError> {
    value
        .field(&[SharedString::new_static(name)])
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(ActionError::Invalid(name))
}

fn index(value: &Value, name: &'static str) -> Result<u64, ActionError> {
    let v = num(value, name)?;
    (v >= 0.0 && v.fract() == 0.0 && v <= u32::MAX as f64)
        .then_some(v as u64)
        .ok_or(ActionError::Invalid(name))
}

/// Track keys travel as numbers, so they are exact up to 2^53.
fn track_id(value: &Value) -> Result<TrackId, ActionError> {
    let v = num(value, "track")?;
    (v >= 0.0 && v.fract() == 0.0 && v <= 9_007_199_254_740_992.0)
        .then_some(TrackId(v as u64))
        .ok_or(ActionError::Invalid("track"))
}

fn clip_ref(value: &Value) -> Result<ClipRef, ActionError> {
    Ok(ClipRef {
        track: track_id(value)?,
        clip: index(value, "clip")? as u32,
    })
}

fn edge_name(edge: Edge) -> &'static str {
    match edge {
        Edge::Start => "start",
        Edge::End => "end",
    }
}

fn clip_fields(at: ClipRef) -> [(&'static str, Value); 2] {
    [
        ("track", Value::Num(at.track.0 as f64)),
        ("clip", Value::Num(f64::from(at.clip))),
    ]
}

impl TracksAction {
    /// The erased form a bus client sends.
    pub fn to_action(&self) -> Action {
        use action_names::*;
        let (name, payload) = match self.clone() {
            Self::ResizeTrack { track, height } => (
                RESIZE_TRACK,
                record(&[
                    ("track", Value::Num(track.0 as f64)),
                    ("height", Value::Num(f64::from(height))),
                ]),
            ),
            Self::MoveClip { clip, start } => {
                let [t, c] = clip_fields(clip);
                (MOVE_CLIP, record(&[t, c, ("start", Value::Num(start))]))
            }
            Self::TrimClip { clip, edge, time } => {
                let [t, c] = clip_fields(clip);
                (
                    TRIM_CLIP,
                    record(&[
                        t,
                        c,
                        ("edge", Value::Str(edge_name(edge).into())),
                        ("time", Value::Num(time)),
                    ]),
                )
            }
            Self::SelectMany(clips) => (
                SELECT,
                Value::List(clips.iter().map(|at| record(&clip_fields(*at))).collect()),
            ),
            Self::Select(None) => (SELECT, Value::Null),
            Self::Select(Some(at)) => (SELECT, record(&clip_fields(at))),
            Self::Seek(time) => (SEEK, record(&[("time", Value::Num(time))])),
        };
        Action {
            name: SharedString::new_static(name),
            payload,
        }
    }

    /// Decode the erased form.
    pub fn decode(action: &Action) -> Result<Self, ActionError> {
        use action_names::*;
        let p = &action.payload;
        Ok(match action.name.as_ref() {
            RESIZE_TRACK => Self::ResizeTrack {
                track: track_id(p)?,
                height: num(p, "height")? as f32,
            },
            MOVE_CLIP => Self::MoveClip {
                clip: clip_ref(p)?,
                start: num(p, "start")?,
            },
            TRIM_CLIP => Self::TrimClip {
                clip: clip_ref(p)?,
                edge: match p.field(&[SharedString::new_static("edge")]).as_str() {
                    Some("start") => Edge::Start,
                    Some("end") => Edge::End,
                    _ => return Err(ActionError::Invalid("edge")),
                },
                time: num(p, "time")?,
            },
            SELECT if *p == Value::Null => Self::Select(None),
            SELECT => match p {
                Value::List(items) => {
                    Self::SelectMany(items.iter().map(clip_ref).collect::<Result<_, _>>()?)
                }
                _ => Self::Select(Some(clip_ref(p)?)),
            },
            SEEK => Self::Seek(match p.as_f64() {
                Some(t) if t.is_finite() => t,
                _ => num(p, "time")?,
            }),
            _ => return Err(ActionError::Unsupported),
        })
    }
}

impl From<TracksChange> for Value {
    fn from(change: TracksChange) -> Self {
        match change {
            TracksChange::TrackResized { track, height } => record(&[
                ("kind", Value::Str("trackResized".into())),
                ("track", Value::Num(track.0 as f64)),
                ("height", Value::Num(f64::from(height))),
            ]),
            TracksChange::ClipChanged { clip, edit, span } => {
                let [t, c] = clip_fields(clip);
                let edit = match edit {
                    ClipEdit::Move => "move".to_owned(),
                    ClipEdit::Trim(edge) => format!("trim_{}", edge_name(edge)),
                };
                record(&[
                    ("kind", Value::Str("clipChanged".into())),
                    t,
                    c,
                    ("edit", Value::Str(edit.into())),
                    ("start", Value::Num(span.start)),
                    ("length", Value::Num(span.len)),
                    ("sourceOffset", Value::Num(span.source_offset)),
                ])
            }
            TracksChange::SelectionChanged { selection } => {
                let mut fields = vec![("kind", Value::Str("selectionChanged".into()))];
                if let Some(at) = selection {
                    fields.extend(clip_fields(at));
                }
                record(&fields)
            }
            TracksChange::Seek { time } => record(&[
                ("kind", Value::Str("seek".into())),
                ("time", Value::Num(time)),
            ]),
        }
    }
}

/// A store with lens undo over the rows (content and heights), so selection, playhead and
/// view changes are never undone.
pub fn tracks_store(
    model: TracksModel,
    undo: impl Into<HistoryLimits>,
    cx: &mut App,
) -> Store<TracksModel> {
    let store = Store::new(model, cx);
    store.enable_undo_lens(
        undo,
        |m: &TracksModel| Arc::clone(&m.rows),
        |m: &mut TracksModel, rows| {
            m.rows = rows;
            m.prune_selection();
        },
        cx,
    );
    store
}

/// Apply a user action to the store (undoable unless it is a selection or a seek).
#[derive(Debug, Clone)]
pub struct ApplyAction(pub TracksAction);

impl Event<TracksModel> for ApplyAction {
    fn handle(self, fx: &mut Fx<'_, TracksModel>) {
        let mut next = fx.state().clone();
        if next.apply(self.0).is_some() {
            fx.set(next);
        }
    }
    fn undoable(&self) -> bool {
        matches!(
            self.0,
            TracksAction::ResizeTrack { .. }
                | TracksAction::MoveClip { .. }
                | TracksAction::TrimClip { .. }
        )
    }
}

macro_rules! host_event {
    ($(#[$doc:meta])* $name:ident($($field:ty),*), |$m:ident, $($arg:ident),*| $body:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $name($(pub $field),*);
        impl Event<TracksModel> for $name {
            fn handle(self, fx: &mut Fx<'_, TracksModel>) {
                let $name($($arg),*) = self;
                let mut $m = fx.state().clone();
                $body;
                if $m != *fx.state() {
                    fx.set($m);
                }
            }
            fn undoable(&self) -> bool {
                false
            }
        }
    };
}

host_event!(
    /// Host write: replace the tracks, diffed in place (see [`TracksModel::update_tracks`]).
    SetTracks(Vec<TrackUpdate>),
    |m, tracks| m.update_tracks(tracks)
);
host_event!(
    /// Host write: move the playhead (the transport), following it if follow mode is on.
    SetPlayhead(f64),
    |m, time| m.set_playhead(time)
);
host_event!(
    /// Host write: restore the view range.
    SetView(ViewRange),
    |m, view| m.set_view(view)
);
host_event!(
    /// Host write: follow mode.
    SetFollow(Follow),
    |m, follow| m.follow = follow
);
host_event!(
    /// Host write: the lowest time the view and playhead reach (`None`: no limit).
    SetMinTime(Option<f64>),
    |m, min| {
        m.min_time = min.filter(|t| t.is_finite());
        if let Some(view) = m.view {
            m.set_view(view);
        }
    }
);
host_event!(
    /// Host write: a track's height.
    SetTrackHeight(TrackId, f32),
    |m, track, height| {
        let h = clamp_height(height);
        if m.row_by_id(track).is_some_and(|r| r.height != h)
            && let Some(row) = m.row_mut(track)
        {
            row.height = h;
        }
    }
);
host_event!(
    /// Host write: the selection.
    SetSelection(Option<ClipRef>),
    |m, selection| {m.replace_selection(selection); }
);
host_event!(
    /// Replace the full selection without an edit event.
    SetSelectionMany(Arc<[ClipRef]>), |m, clips| { m.replace_selection(clips.iter().copied()); }
);
host_event!(
    /// The view's own layout and navigation writes (width, wheel zoom and pan).
    SetViewport(f64, Option<ViewRange>),
    |m, width, view| {
        if width.is_finite() && width > 0.0 {
            m.viewport = width;
        }
        if let Some(view) = view {
            m.set_view(view);
        }
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    fn track(clips: &[(f64, f64)]) -> Track {
        let mut t = Track::default();
        for &(s, l) in clips {
            t.insert(s, l);
        }
        t
    }

    fn model() -> TracksModel {
        TracksModel::new(vec![
            TrackRow::new(10, "A", track(&[(1.0, 2.0), (5.0, 1.0)])),
            TrackRow::new(20, "B", track(&[(0.0, 1.0)])).with_height(100.0),
            TrackRow::new(30, "C", track(&[])).with_height(25.0),
        ])
    }

    #[test]
    fn selection_set_deduplicates_valid_keys_and_survives_row_replacement() {
        let mut m = model();
        let a = ClipRef::new(10, 1);
        let b = ClipRef::new(20, 1);
        let action = TracksAction::SelectMany([b, a, b, ClipRef::new(40, 1)].into());
        assert_eq!(
            TracksAction::decode(&action.to_action()),
            Ok(action.clone())
        );
        assert!(m.apply(action.clone()).is_some());
        assert_eq!(m.selected_clips().collect::<Vec<_>>(), [b, a]);
        assert!(m.apply(action).is_none());
        m.update_tracks(vec![TrackUpdate::new(10, "A", track(&[(1., 2.)]))]);
        assert_eq!(m.selected_clips().collect::<Vec<_>>(), [a]);
        assert!(m.apply(TracksAction::Select(None)).is_some());
        assert_eq!(m.selected_clips().next(), None);
    }

    #[test]
    fn layout_is_a_prefix_sum_and_hit_tests_by_binary_search() {
        let layout = RowLayout::new([40.0, 100.0, 25.0]);
        assert_eq!(
            (layout.top(1), layout.top(2), layout.total()),
            (40.0, 140.0, 165.0)
        );
        assert_eq!(layout.height(1), 100.0);
        assert_eq!(layout.row_at(0.0), Some(0));
        assert_eq!(layout.row_at(39.9), Some(0));
        assert_eq!(layout.row_at(40.0), Some(1));
        assert_eq!(layout.row_at(164.0), Some(2));
        assert_eq!(layout.row_at(165.0), None);
        assert_eq!(layout.row_at(-1.0), None);
        assert_eq!(layout.rows_in(50.0, 150.0), 1..3);
        assert_eq!(layout.rows_in(0.0, 40.0), 0..1);
        assert_eq!(layout.rows_in(200.0, 300.0), 3..3);
        assert_eq!(layout.bottom_edge_at(141.0, 3.0), Some(1));
        assert_eq!(layout.bottom_edge_at(100.0, 3.0), None);
        assert_eq!(
            layout.bottom_edge_at(-2.0, 3.0),
            None,
            "the top edge is not a row bottom"
        );
        let mut again = layout.clone();
        let capacity = again.tops.capacity();
        again.rebuild([10.0, 10.0, 10.0]);
        assert_eq!(
            again.tops.capacity(),
            capacity,
            "rebuild reuses the allocation"
        );
        assert_eq!(again.row_at(25.0), Some(2));
    }

    #[test]
    fn row_lookup_matches_a_linear_scan() {
        let heights: Vec<f32> = (0..50).map(|i| 20.0 + (i * 37 % 90) as f32).collect();
        let layout = RowLayout::new(heights.iter().copied());
        let mut y = 0.0;
        for (row, h) in heights.iter().enumerate() {
            for dy in [0.0, h / 2.0, h - 0.01] {
                assert_eq!(layout.row_at(y + dy), Some(row));
            }
            y += h;
        }
    }

    #[test]
    fn actions_report_what_changed_and_no_ops_report_nothing() {
        let mut m = model();
        let a = ClipRef::new(10, 1);
        assert_eq!(
            m.apply(TracksAction::ResizeTrack {
                track: TrackId(10),
                height: 5.0
            }),
            Some(TracksChange::TrackResized {
                track: TrackId(10),
                height: MIN_TRACK_HEIGHT
            })
        );
        assert_eq!(
            m.apply(TracksAction::ResizeTrack {
                track: TrackId(10),
                height: 5.0
            }),
            None
        );
        assert!(matches!(
            m.apply(TracksAction::MoveClip { clip: a, start: 1.5 }),
            Some(TracksChange::ClipChanged { edit: ClipEdit::Move, span, .. }) if span.start == 1.5
        ));
        assert_eq!(
            m.apply(TracksAction::MoveClip {
                clip: a,
                start: 1.5
            }),
            None
        );
        // The end trims past the content up to the next clip at 5.
        assert!(matches!(
            m.apply(TracksAction::TrimClip { clip: a, edge: Edge::End, time: 9.0 }),
            Some(TracksChange::ClipChanged { edit: ClipEdit::Trim(Edge::End), span, .. })
                if span.end() == 5.0
        ));
        assert_eq!(
            m.apply(TracksAction::Select(Some(a))),
            Some(TracksChange::SelectionChanged { selection: Some(a) })
        );
        assert_eq!(
            m.apply(TracksAction::Select(Some(ClipRef::new(10, 99)))),
            Some(TracksChange::SelectionChanged { selection: None }),
            "a missing clip clears"
        );
        assert_eq!(m.apply(TracksAction::Seek(-3.0)), None, "already at 0");
        assert_eq!(
            m.apply(TracksAction::Seek(2.0)),
            Some(TracksChange::Seek { time: 2.0 })
        );
        assert_eq!(m.apply(TracksAction::Seek(f64::NAN)), None);
    }

    #[test]
    fn update_tracks_diffs_in_place_and_keeps_heights_selection_and_view() {
        let mut m = model();
        m.selection = Some(ClipRef::new(10, 2));
        m.set_view(ViewRange {
            start: 3.0,
            scale: 0.01,
        });
        let before = m.clone();
        let same: Vec<_> = m
            .rows()
            .iter()
            .map(|r| TrackUpdate::new(r.id, r.name.clone(), r.track.clone()))
            .collect();
        assert!(!m.update_tracks(same.clone()));
        assert!(m.same_rows(&before));
        let mut changed = same;
        changed[1].track = track(&[(2.0, 1.0)]);
        assert!(m.update_tracks(changed));
        assert!(
            Arc::ptr_eq(&m.rows()[0], &before.rows()[0]),
            "unchanged row shared"
        );
        assert_eq!(m.rows()[1].height, 100.0, "height kept");
        assert_eq!(m.view, before.view, "zoom survives");
        assert_eq!(m.selection, before.selection);
        assert!(m.update_tracks(vec![TrackUpdate::new(10, "A", Track::default())]));
        assert_eq!(m.selection, None, "selected clip gone");
        assert_eq!(m.rows().len(), 1);
    }

    #[test]
    fn follow_modes_and_min_time() {
        let mut m = model();
        m.viewport = 100.0;
        m.set_view(ViewRange {
            start: 0.0,
            scale: 0.1,
        }); // 10 s visible
        m.set_playhead(5.0);
        assert_eq!(m.view.map(|v| v.start), Some(0.0), "follow off");
        m.follow = Follow::Page;
        m.set_playhead(9.0);
        assert_eq!(m.view.map(|v| v.start), Some(0.0), "still on the page");
        m.set_playhead(12.0);
        assert_eq!(m.view.map(|v| v.start), Some(12.0), "turned the page");
        m.follow = Follow::Center;
        m.set_playhead(30.0);
        assert_eq!(m.view.map(|v| v.start), Some(25.0));
        m.min_time = Some(0.0);
        m.set_view(ViewRange {
            start: -4.0,
            scale: 0.1,
        });
        assert_eq!(
            m.view.map(|v| v.start),
            Some(0.0),
            "clamped to the minimum time"
        );
        m.set_playhead(-1.0);
        assert_eq!(m.playhead, 0.0);
    }

    #[test]
    fn erased_actions_round_trip_and_reject_bad_payloads() {
        let at = ClipRef::new(2, 7);
        for action in [
            TracksAction::ResizeTrack {
                track: TrackId(1),
                height: 64.0,
            },
            TracksAction::MoveClip {
                clip: at,
                start: 1.25,
            },
            TracksAction::TrimClip {
                clip: at,
                edge: Edge::End,
                time: 9.5,
            },
            TracksAction::TrimClip {
                clip: at,
                edge: Edge::Start,
                time: 0.5,
            },
            TracksAction::Select(Some(at)),
            TracksAction::Select(None),
            TracksAction::Seek(3.0),
        ] {
            assert_eq!(TracksAction::decode(&action.to_action()), Ok(action));
        }
        let bad = Action {
            name: action_names::MOVE_CLIP.into(),
            payload: record(&[("track", Value::Num(-1.0))]),
        };
        assert_eq!(
            TracksAction::decode(&bad),
            Err(ActionError::Invalid("track"))
        );
        let seek = Action {
            name: action_names::SEEK.into(),
            payload: Value::Num(2.0),
        };
        assert_eq!(TracksAction::decode(&seek), Ok(TracksAction::Seek(2.0)));
        let other = Action {
            name: "toggle".into(),
            payload: Value::Null,
        };
        assert_eq!(TracksAction::decode(&other), Err(ActionError::Unsupported));
    }

    fn ids(m: &TracksModel) -> Vec<u64> {
        m.rows().iter().map(|r| r.id.0).collect()
    }

    fn updates(m: &TracksModel) -> Vec<TrackUpdate> {
        m.rows()
            .iter()
            .map(|r| TrackUpdate::new(r.id, r.name.clone(), r.track.clone()))
            .collect()
    }

    #[test]
    fn insert_remove_and_reorder_keep_state_on_the_right_tracks() {
        let mut m = model(); // 10: 40 px, 20: 100 px, 30: 25 px
        m.selection = Some(ClipRef::new(20, 1));
        let before = m.clone();

        // Insert a track in the middle.
        let mut rows = updates(&m);
        rows.insert(1, TrackUpdate::new(15, "New", track(&[(0.0, 4.0)])));
        assert!(m.update_tracks(rows));
        assert_eq!(ids(&m), [10, 15, 20, 30]);
        let heights: Vec<f32> = m.rows().iter().map(|r| r.height).collect();
        assert_eq!(heights, [40.0, DEFAULT_TRACK_HEIGHT, 100.0, 25.0]);
        for id in [10, 20, 30] {
            let (a, b) = (
                m.rows()[m.index_of(TrackId(id)).unwrap_or(0)].clone(),
                before.row_by_id(TrackId(id)),
            );
            assert!(
                b.is_some_and(|b| std::ptr::eq(&*a, b)),
                "track {id} keeps its allocation"
            );
        }
        assert_eq!(
            m.selection,
            Some(ClipRef::new(20, 1)),
            "selection follows the key"
        );
        assert_eq!(m.selected_index(), Some((2, 1)), "and moves down a row");

        // Remove the track above the selection.
        let mut rows = updates(&m);
        rows.remove(0);
        assert!(m.update_tracks(rows));
        assert_eq!(ids(&m), [15, 20, 30]);
        assert_eq!(m.row_by_id(TrackId(20)).map(|r| r.height), Some(100.0));
        assert_eq!(m.selected_index(), Some((1, 1)));

        // Reorder: state moves with the key.
        let mut rows = updates(&m);
        rows.reverse();
        assert!(m.update_tracks(rows));
        assert_eq!(ids(&m), [30, 20, 15]);
        let heights: Vec<f32> = m.rows().iter().map(|r| r.height).collect();
        assert_eq!(heights, [25.0, 100.0, DEFAULT_TRACK_HEIGHT]);
        assert_eq!(m.selection, Some(ClipRef::new(20, 1)));

        // Actions address tracks by key wherever they are.
        assert!(
            m.apply(TracksAction::ResizeTrack {
                track: TrackId(30),
                height: 60.0
            })
            .is_some()
        );
        assert_eq!(m.rows()[0].height, 60.0);
        assert!(
            m.apply(TracksAction::MoveClip {
                clip: ClipRef::new(20, 1),
                start: 0.5
            })
            .is_some()
        );
        assert_eq!(
            m.row_by_id(TrackId(20)).map(|r| r.track.clips()[0].start),
            Some(0.5)
        );

        // Removing the selected track clears the selection; a duplicate key is ignored.
        let mut rows = updates(&m);
        rows.retain(|u| u.id != TrackId(20));
        rows.push(TrackUpdate::new(30, "Dup", Track::default()));
        assert!(m.update_tracks(rows));
        assert_eq!(ids(&m), [30, 15]);
        assert_eq!(
            m.rows()[0].name.as_ref(),
            "C",
            "the first occurrence of a key wins"
        );
        assert_eq!(m.selection, None);
    }
}
