//! DAW / animation track timeline: named tracks of clips with per-track heights, a playhead,
//! drag to move, drag clip edges to trim, drag a row's bottom edge to resize it, wheel pan
//! and scroll, ctrl+wheel zoom.
//!
//! The model ([`TracksModel`]) lives in a [`Store`]. Hosts write it with store events
//! ([`SetTracks`](crate::tracks_model::SetTracks), [`SetPlayhead`](crate::tracks_model::SetPlayhead),
//! [`SetView`](crate::tracks_model::SetView), ...), which never come back as user actions.
//! User edits are [`TracksAction`]s: pointer and keyboard input build the same erased action
//! a bus client sends and, once the view is registered ([`TracksView::register`]), dispatch it
//! through the [`ActionBus`]. A drag previews locally and commits one action on release in a
//! `dispatch_batch`, so it is one undo step. Every applied action emits a [`TracksEvent`] and
//! returns the [`TracksChange`] as its bus outcome.

use std::{cell::Cell, rc::Rc, sync::Arc};

use gpui_kit::{
    AnyElement, App, Bounds, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    Hsla, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement,
    Pixels, Render, Role, ScrollEvent, SharedString, Styled, TestSupportExt, WeakEntity, Window,
    canvas, div, prelude::*, px,
};
use guiraffe_core::{
    GuiraffeTheme, StateKind, Variant, focus_ring,
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source,
    },
    ring_visible,
    widget_value::Value,
};
use guiraffe_state::{Selected, Store};

pub use crate::tracks_model::{
    ClipEdit, ClipRef, DEFAULT_TRACK_HEIGHT, Follow, RowLayout, TrackRow, TrackUpdate,
    TracksAction, TracksChange, TracksModel, ViewRange, tracks_store,
};
use crate::{
    axis::{Axis, TimeFormat, label},
    clips::{Clip, Edge, Track},
    edit::{
        EndEarlier, EndLater, KEY_CONTEXT, MoveEarlier, MoveLater, Nudge, SelectNextItem,
        SelectPrevItem, StartEarlier, StartLater, key_step,
    },
    tracks_model::{ApplyAction, SetSelection, SetViewport},
};

/// Default row height (rows may differ; see [`TrackRow::height`]).
pub const TRACK_H: f32 = DEFAULT_TRACK_HEIGHT;
pub const HEADER_W: f32 = 140.0;
pub const RULER_H: f32 = 24.0;
/// Pixels either side of a row's bottom edge that start a resize.
pub const RESIZE_GRAB: f32 = 3.0;
/// Vertical inset of a clip inside its row.
const CLIP_INSET: f32 = 4.0;
pub use crate::edit::EDGE_GRAB;

/// A user action was applied. Host writes to the store never emit these.
#[derive(Debug, Clone, PartialEq)]
pub enum TracksEvent {
    ClipChanged {
        track: usize,
        clip: u32,
        edit: ClipEdit,
    },
    TrackResized {
        track: usize,
        height: f32,
    },
    SelectionChanged {
        selected: Option<(usize, u32)>,
    },
    Seek(f64),
}

impl From<TracksChange> for TracksEvent {
    fn from(change: TracksChange) -> Self {
        match change {
            TracksChange::ClipChanged { clip, edit, .. } => Self::ClipChanged {
                track: clip.track,
                clip: clip.clip,
                edit,
            },
            TracksChange::TrackResized { track, height } => Self::TrackResized { track, height },
            TracksChange::SelectionChanged { selection } => Self::SelectionChanged {
                selected: selection.map(|s| (s.track, s.clip)),
            },
            TracksChange::Seek { time } => Self::Seek(time),
        }
    }
}

pub use crate::edit::{Grab, ResizeEdges};

/// What a [`ClipPainter`] paints: a snapshot of one visible clip.
pub struct ClipPaint<'a> {
    pub track: usize,
    pub row: &'a TrackRow,
    pub clip: &'a Clip,
    /// The clip's on-screen rectangle.
    pub bounds: Bounds<Pixels>,
    /// Maps time to x, so content lines up with the ruler.
    pub axis: &'a Axis,
    pub selected: bool,
}

/// Paints inside each visible clip, after its background and before the selection. It gets
/// a snapshot and read-only app access: painters only paint.
pub trait ClipPainter: 'static {
    fn paint(&self, clip: &ClipPaint<'_>, window: &mut Window, cx: &App);
}

/// How the selected clip is marked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionStyle {
    pub border: Hsla,
    pub fill: Hsla,
    pub width: Pixels,
}

impl SelectionStyle {
    /// The theme's accent: an accent border with a light accent tint.
    pub fn themed(theme: &GuiraffeTheme) -> Self {
        let accent = theme.variant(None, Variant::Filled).background.to_hsla();
        Self {
            border: accent,
            fill: Hsla { a: 0.18, ..accent },
            width: px(2.),
        }
    }
}

/// One row handed to a custom header renderer.
pub struct HeaderRow {
    pub index: usize,
    pub row: Arc<TrackRow>,
    pub height: f32,
    /// Whether the selected clip is on this track.
    pub selected: bool,
}

/// Renders one header cell; the view sizes and positions it.
pub type HeaderRenderer = Rc<dyn Fn(&HeaderRow, &mut Window, &mut App) -> AnyElement>;

/// The track header column.
#[derive(Clone, Default)]
pub enum HeaderMode {
    /// Names and a lock toggle, [`HEADER_W`] wide.
    #[default]
    Default,
    /// No header column.
    Hidden,
    /// The host's own cells, `width` pixels wide, aligned to the rows.
    Custom(f32, HeaderRenderer),
}

impl HeaderMode {
    pub fn width(&self) -> f32 {
        match self {
            Self::Default => HEADER_W,
            Self::Hidden => 0.0,
            Self::Custom(w, _) => w.max(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    Clip { clip: ClipRef, grab: Grab },
    Resize { track: usize, from_y: f32, from_h: f32 },
}

pub struct TracksView {
    store: Store<TracksModel>,
    model: Selected<TracksModel>,
    layout: RowLayout,
    length: f64,
    surface: crate::PlotSurface,
    lock_style: crate::LockStyle,
    global_lock_style: crate::LockStyle,
    format: TimeFormat,
    snap: Option<f64>,
    drag: Option<Drag>,
    /// The action a drag will commit on release; painted as a preview meanwhile.
    preview: Option<TracksAction>,
    body: Rc<Cell<Bounds<Pixels>>>,
    /// Range to fit once the real width is known.
    pending_fit: Option<(f64, f64)>,
    resize: ResizeEdges,
    /// Clips that cannot be moved or resized, by (track, clip id).
    pub(crate) locked: std::collections::HashSet<(usize, u32)>,
    /// When false, nothing can be edited (seek still works).
    editable: bool,
    resizable: bool,
    header: HeaderMode,
    painter: Option<Rc<dyn ClipPainter>>,
    selection_style: Option<SelectionStyle>,
    /// Vertical scroll of the rows, in pixels.
    scroll_y: f32,
    /// What the pointer is over, for the cursor.
    hover: Option<Grab>,
    hover_resize: bool,
    id: Option<ControlId>,
    focus: FocusHandle,
    /// `None`: a tab stop only inside a focus scope.
    focusable: Option<bool>,
}

impl EventEmitter<TracksEvent> for TracksView {}

impl Focusable for TracksView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Element id of the clip `clip` on track `track` (for tests and accessibility).
pub fn clip_element_id(track: usize, clip: u32) -> ElementId {
    ElementId::NamedInteger(SharedString::from(format!("clip-{track}")), u64::from(clip))
}

/// Decide what a press at time `t` (pixel `x`) on `track` grabs. Pure; tested.
pub fn hit(track: &Track, axis: &Axis, x: f64, edges: ResizeEdges) -> Option<(u32, Grab)> {
    let (index, grab) = hit_index(track, axis, x, edges)?;
    Some((track.clips()[index].id, grab))
}

fn hit_index(track: &Track, axis: &Axis, x: f64, edges: ResizeEdges) -> Option<(usize, Grab)> {
    let t = axis.t(x);
    // Clips are sorted and disjoint; only those near x can match.
    let clips = track.clips();
    let first = clips.partition_point(|c| c.end() < axis.t((x - EDGE_GRAB).max(0.)));
    let last = clips.partition_point(|c| c.start <= axis.t((x + EDGE_GRAB).min(axis.width)));
    clips
        .get(first..last)?
        .iter()
        .enumerate()
        .find_map(|(i, c)| {
            crate::edit::grab_at(axis.x(c.start), axis.x(c.end()), x, t, c.start, edges)
                .map(|g| (first + i, g))
        })
}

/// Route an input exactly as a bus client would send it: through the bus when registered
/// (a drag commit as a one-action batch), else straight to [`TracksView::perform`].
fn route(
    view: &WeakEntity<TracksView>,
    action: TracksAction,
    source: Source,
    batch: bool,
    window: &mut Window,
    cx: &mut App,
) -> Result<Option<TracksChange>, ActionError> {
    let entity = view.upgrade().ok_or(ActionError::UnknownTarget)?;
    match entity.read(cx).id {
        Some(id) => {
            let action = action.to_action();
            let outcome = if batch {
                ActionBus::dispatch_batch(id, std::slice::from_ref(&action), source, window, cx)
                    .into_result()
                    .map(|mut o| o.pop().unwrap_or(Outcome::EMPTY))
                    .map_err(|e| e.error)?
            } else {
                ActionBus::dispatch(id, &action, source, window, cx)?
            };
            // Bus clients see the change as the outcome; the model is already updated.
            drop(outcome);
            Ok(None)
        }
        None => entity.update(cx, |view, cx| view.perform(action, cx)),
    }
}

impl TracksView {
    /// A view over a new store holding `tracks`, with 100 steps of undo.
    pub fn new(
        tracks: Vec<(String, Track)>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        let store = tracks_store(TracksModel::from_tracks(tracks), 100, cx);
        Self::with_store(store, format, length, cx)
    }

    /// A view over the host's store (see [`tracks_store`]). `length` is the range fitted
    /// until the store has a view range.
    pub fn with_store(
        store: Store<TracksModel>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        let length = length.max(1.0);
        cx.new(|cx| {
            let model = store.select(cx, |m: &TracksModel| m.clone());
            let state = model.get(cx).clone();
            let axis = state.axis(800.0, length);
            let mut layout = RowLayout::default();
            layout.rebuild(state.rows().iter().map(|r| r.height));
            Self {
                surface: crate::PlotSurface::timeline(axis, format),
                pending_fit: state.view.is_none().then_some((0.0, length)),
                store,
                model,
                layout,
                length,
                lock_style: crate::LockStyle::Badge,
                global_lock_style: crate::LockStyle::None,
                format,
                snap: None,
                drag: None,
                preview: None,
                body: Rc::default(),
                resize: ResizeEdges::default(),
                locked: Default::default(),
                editable: true,
                resizable: false,
                header: HeaderMode::Default,
                painter: None,
                selection_style: None,
                scroll_y: 0.0,
                hover: None,
                hover_resize: false,
                id: None,
                focus: cx.focus_handle(),
                focusable: None,
            }
        })
    }

    /// Register on the window's action bus at `path`. Pointer and keyboard input then go
    /// through the bus, so interceptors see them.
    pub fn register(
        view: &Entity<Self>,
        path: &str,
        window: &Window,
        cx: &mut App,
    ) -> ControlId {
        let id = ActionBus::register(path, view, "tracks", Capabilities::SELECT, window, cx);
        view.update(cx, |v, _| v.id = Some(id));
        id
    }

    pub fn control_id(&self) -> Option<ControlId> {
        self.id
    }

    pub fn store(&self) -> &Store<TracksModel> {
        &self.store
    }

    /// The model as of the last store flush.
    pub fn model<'a>(&self, cx: &'a App) -> &'a TracksModel {
        self.model.get(cx)
    }

    pub fn tracks<'a>(&self, cx: &'a App) -> &'a [Arc<TrackRow>] {
        self.model(cx).rows()
    }

    /// The selected clip `(track, clip id)`.
    pub fn selected(&self, cx: &App) -> Option<(usize, u32)> {
        self.model(cx).selection.map(|s| (s.track, s.clip))
    }

    pub fn playhead(&self, cx: &App) -> f64 {
        self.model(cx).playhead
    }

    pub fn track_height(&self, track: usize, cx: &App) -> f32 {
        self.model(cx).row(track).map_or(0.0, |r| r.height)
    }

    /// Row tops and heights as of the last render.
    pub fn layout(&self) -> &RowLayout {
        &self.layout
    }

    /// Host write of the selection (no event).
    pub fn select(&mut self, sel: Option<(usize, u32)>, cx: &mut Context<Self>) {
        self.store.dispatch_now(
            SetSelection(sel.map(|(track, clip)| ClipRef { track, clip })),
            cx,
        );
    }

    /// Apply a user action: what the bus, the pointer and the keyboard all run. Emits a
    /// [`TracksEvent`] and returns the change, or `None` for a no-op.
    pub fn perform(
        &mut self,
        action: TracksAction,
        cx: &mut Context<Self>,
    ) -> Result<Option<TracksChange>, ActionError> {
        if let TracksAction::MoveClip { clip, .. } | TracksAction::TrimClip { clip, .. } = action
            && (!self.editable || self.locked.contains(&(clip.track, clip.clip)))
        {
            return Err(ActionError::Disabled);
        }
        if matches!(action, TracksAction::ResizeTrack { .. }) && !self.editable {
            return Err(ActionError::Disabled);
        }
        let mut next = self.model(cx).clone();
        let Some(change) = next.apply(action) else {
            return Ok(None);
        };
        self.store.dispatch_now(ApplyAction(action), cx);
        cx.emit(TracksEvent::from(change));
        cx.notify();
        Ok(Some(change))
    }

    /// `Some(true)` enables Tab and focus on item clicks. `None` follows the
    /// enclosing Tab scope and preserves focus on clicks. `Some(false)` opts out.
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = v;
        cx.notify();
    }

    /// Paint inside each visible clip (see [`ClipPainter`]).
    pub fn set_clip_painter(&mut self, painter: Option<Rc<dyn ClipPainter>>, cx: &mut Context<Self>) {
        self.painter = painter;
        cx.notify();
    }

    /// Override the selection style (`None`: the theme's accent).
    pub fn set_selection_style(&mut self, style: Option<SelectionStyle>, cx: &mut Context<Self>) {
        self.selection_style = style;
        cx.notify();
    }

    /// The selection style in effect.
    pub fn selection_style(&self, cx: &App) -> SelectionStyle {
        self.selection_style
            .unwrap_or_else(|| SelectionStyle::themed(GuiraffeTheme::global(cx)))
    }

    pub fn set_header(&mut self, header: HeaderMode, cx: &mut Context<Self>) {
        self.header = header;
        cx.notify();
    }

    /// Let the user drag a row's bottom edge to resize it (a `ResizeTrack` action).
    pub fn set_resizable_tracks(&mut self, on: bool, cx: &mut Context<Self>) {
        self.resizable = on;
        cx.notify();
    }

    /// Vertical scroll offset of the rows.
    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    pub fn set_scroll_y(&mut self, y: f32, cx: &mut Context<Self>) {
        let y = self.clamp_scroll(y);
        if y != self.scroll_y {
            self.scroll_y = y;
            cx.notify();
        }
    }

    fn body_height(&self) -> f32 {
        f32::from(self.body.get().size.height)
    }

    fn clamp_scroll(&self, y: f32) -> f32 {
        let max = (self.layout.total() - self.body_height()).max(0.0);
        if y.is_finite() { y.clamp(0.0, max) } else { 0.0 }
    }

    /// Keyboard edit of the selected clip by `dir` steps (snap grid, else a quarter tick).
    pub fn nudge(&mut self, what: Nudge, dir: f64, cx: &mut Context<Self>) {
        if let Some(action) = self.nudge_action(what, dir, cx) {
            self.perform(action, cx).ok();
        }
    }

    fn nudge_action(&self, what: Nudge, dir: f64, cx: &App) -> Option<TracksAction> {
        let model = self.model(cx);
        let clip = model.selection?;
        let c = *model.clip(clip)?;
        let d = dir * key_step(self.snap, self.surface.axis.tick_step(80.0, self.format));
        let snapped = |t: f64| self.snapped(t);
        Some(match what {
            Nudge::Move => TracksAction::MoveClip {
                clip,
                start: snapped(c.start + d),
            },
            Nudge::End => TracksAction::TrimClip {
                clip,
                edge: Edge::End,
                time: snapped(c.end() + d),
            },
            Nudge::Start => TracksAction::TrimClip {
                clip,
                edge: Edge::Start,
                time: snapped(c.start + d),
            },
        })
    }

    fn select_step_action(&self, dir: i32, cx: &App) -> Option<TracksAction> {
        let model = self.model(cx);
        let rows = model.rows();
        let current = model.selection.and_then(|s| {
            Some((s.track, rows.get(s.track)?.track.index_of(s.clip)?))
        });
        let next = if dir < 0 {
            match current {
                Some((row, index)) if index > 0 => Some((row, index - 1)),
                _ => rows
                    .iter()
                    .enumerate()
                    .take(current.map_or(rows.len(), |(row, _)| row))
                    .rev()
                    .find_map(|(row, r)| r.track.clips().len().checked_sub(1).map(|i| (row, i)))
                    .or(current),
            }
        } else {
            match current {
                Some((row, index)) if index + 1 < rows[row].track.clips().len() => {
                    Some((row, index + 1))
                }
                _ => rows
                    .iter()
                    .enumerate()
                    .skip(current.map_or(0, |(row, _)| row + 1))
                    .find_map(|(row, r)| (!r.track.clips().is_empty()).then_some((row, 0)))
                    .or(current),
            }
        };
        (next != current).then(|| {
            TracksAction::Select(next.map(|(row, index)| ClipRef {
                track: row,
                clip: rows[row].track.clips()[index].id,
            }))
        })
    }

    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn set_lock_style(
        &mut self,
        item: crate::LockStyle,
        global: crate::LockStyle,
        cx: &mut Context<Self>,
    ) {
        self.lock_style = item;
        self.global_lock_style = global;
        cx.notify();
    }
    /// The time axis as of the last render (or the initial fit).
    pub fn axis(&self) -> &Axis {
        &self.surface.axis
    }

    /// Snap edits to a grid (seconds); `None` disables.
    pub fn set_snap(&mut self, grid: Option<f64>) {
        self.snap = grid.filter(|g| g.is_finite() && *g > 0.0);
    }

    fn snapped(&self, t: f64) -> f64 {
        match self.snap {
            Some(g) => (t / g).round() * g,
            None => t,
        }
    }

    /// A user seek (emits [`TracksEvent::Seek`]); hosts write `SetPlayhead` instead.
    pub fn seek(&mut self, t: f64, cx: &mut Context<Self>) {
        self.perform(TracksAction::Seek(t), cx).ok();
    }

    /// Begin a drag at body-local pixel `x` on `track`, selecting the clip under it.
    /// Returns whether a clip was grabbed.
    pub fn press(&mut self, track: usize, x: f64, cx: &mut Context<Self>) -> bool {
        if let Some(select) = self.press_item(track, x, cx) {
            self.perform(select, cx).ok();
        }
        self.drag.is_some()
    }

    /// Start a drag; returns the selection action to route, if the press hit a clip.
    fn press_item(&mut self, track: usize, x: f64, cx: &App) -> Option<TracksAction> {
        let model = self.model(cx);
        let hit = model.row(track).and_then(|row| {
            hit_index(&row.track, &self.surface.axis, x, ResizeEdges::None)
                .map(|(index, _)| row.track.clips()[index].id)
        });
        self.preview = None;
        self.drag = self.grab(track, x, cx).map(|(clip, grab)| Drag::Clip {
            clip: ClipRef { track, clip },
            grab,
        });
        let selection = model.selection;
        hit.map(|clip| ClipRef { track, clip })
            .filter(|at| selection != Some(*at))
            .map(|at| TracksAction::Select(Some(at)))
    }

    /// Begin resizing `track` from content y `y`.
    pub fn press_resize(&mut self, track: usize, y: f32, cx: &App) -> bool {
        let Some(row) = self.model(cx).row(track) else {
            return false;
        };
        if !(self.resizable && self.editable) {
            return false;
        }
        self.preview = None;
        self.drag = Some(Drag::Resize {
            track,
            from_y: y,
            from_h: row.height,
        });
        true
    }

    /// What a press at (`track`, `x`) would grab, honouring locks and the resize policy.
    pub fn grab(&self, track: usize, x: f64, cx: &App) -> Option<(u32, Grab)> {
        if !self.editable {
            return None;
        }
        let row = self.model(cx).row(track)?;
        hit(&row.track, &self.surface.axis, x, self.resize)
            .filter(|(clip, _)| !self.locked.contains(&(track, *clip)))
    }

    /// Which clip edges resize (default: both).
    pub fn set_resize_edges(&mut self, edges: ResizeEdges, cx: &mut Context<Self>) {
        self.resize = edges;
        cx.notify();
    }

    /// Lock or unlock one clip's position and size.
    pub fn set_locked(&mut self, track: usize, clip: u32, locked: bool, cx: &mut Context<Self>) {
        let changed = if locked {
            self.locked.insert((track, clip))
        } else {
            self.locked.remove(&(track, clip))
        };
        if changed {
            cx.notify();
        }
    }

    pub fn is_locked(&self, track: usize, clip: u32) -> bool {
        self.locked.contains(&(track, clip))
    }

    /// Allow or forbid all edits (a global lock).
    pub fn set_editable(&mut self, editable: bool, cx: &mut Context<Self>) {
        if self.editable != editable {
            self.editable = editable;
            self.drag = None;
            self.preview = None;
            cx.notify();
        }
    }

    /// Update the hover grab (cursor); re-renders only when it changes.
    fn hover_at(&mut self, y: f32, x: f64, cx: &mut Context<Self>) {
        let content = y + self.scroll_y;
        let resize = self.resizable
            && self.editable
            && self.layout.bottom_edge_at(content, RESIZE_GRAB).is_some();
        let g = if resize {
            None
        } else {
            self.layout
                .row_at(content)
                .and_then(|row| self.grab(row, x, cx))
                .map(|(_, g)| g)
        };
        let kind = |g: Option<Grab>| g.map(|g| matches!(g, Grab::Move { .. }));
        let edge = |g: Option<Grab>| match g {
            Some(Grab::Trim(e)) => Some(e),
            _ => None,
        };
        if kind(g) != kind(self.hover) || edge(g) != edge(self.hover) || resize != self.hover_resize
        {
            self.hover = g;
            self.hover_resize = resize;
            cx.notify();
        }
    }

    /// Continue the active drag to body-local pixel `x` (and content y `y` for a resize).
    fn drag_preview(&mut self, x: f64, y: f32, cx: &App) -> bool {
        let Some(drag) = self.drag else { return false };
        let action = match drag {
            Drag::Clip { clip, grab } => {
                let t = self.surface.axis.t(x);
                match grab {
                    Grab::Move { grab_dt } => TracksAction::MoveClip {
                        clip,
                        start: self.snapped(t - grab_dt),
                    },
                    Grab::Trim(edge) => TracksAction::TrimClip {
                        clip,
                        edge,
                        time: self.snapped(t),
                    },
                }
            }
            Drag::Resize {
                track,
                from_y,
                from_h,
            } => TracksAction::ResizeTrack {
                track,
                height: crate::tracks_model::clamp_height(from_h + (y - from_y)),
            },
        };
        let mut probe = self.model(cx).clone();
        let next = probe.apply(action).map(|_| action);
        if next != self.preview {
            self.preview = next;
            true
        } else {
            false
        }
    }

    /// Continue the active clip drag to body-local pixel `x` (the change shows as a
    /// preview until [`TracksView::release`]).
    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        if self.drag_preview(x, 0.0, cx) {
            cx.notify();
        }
    }

    /// Continue a resize drag to content y `y`.
    pub fn drag_resize_to(&mut self, y: f32, cx: &mut Context<Self>) {
        if self.drag_preview(0.0, y, cx) {
            cx.notify();
        }
    }

    /// End the drag, taking the action to commit.
    fn take_commit(&mut self) -> Option<TracksAction> {
        self.drag = None;
        self.preview.take()
    }

    /// Drop an active drag without committing it.
    pub fn cancel_drag(&mut self) {
        self.drag = None;
        self.preview = None;
    }

    /// End the drag and commit its change (one action).
    pub fn release(&mut self, cx: &mut Context<Self>) {
        if let Some(action) = self.take_commit() {
            self.perform(action, cx).ok();
        }
        cx.notify();
    }

    fn local(&self, p: gpui_kit::Point<Pixels>) -> (f64, f32) {
        let b = self.body.get();
        (
            f64::from(f32::from(p.x - b.origin.x)),
            f32::from(p.y - b.origin.y),
        )
    }

    fn on_wheel(&mut self, e: &ScrollEvent, cx: &mut Context<Self>) {
        let d = e.delta.pixel_delta(px(16.0));
        let (dx, dy) = (f64::from(f32::from(d.x)), f64::from(f32::from(d.y)));
        let zoom = e.modifiers.control || e.modifiers.platform;
        let overflow = self.layout.total() > self.body_height();
        // A vertical wheel scrolls the rows when they overflow (shift: pan time instead).
        if !zoom && dx == 0.0 && overflow && !e.modifiers.shift {
            let y = self.clamp_scroll(self.scroll_y - dy as f32);
            if y != self.scroll_y {
                self.scroll_y = y;
                cx.notify();
            }
            return;
        }
        let (x, _) = self.local(e.position);
        let mut surface = self.surface.clone();
        if surface.navigate(x, dx, dy, zoom) {
            self.surface.axis = surface.axis;
            let view = ViewRange {
                start: surface.axis.start,
                scale: surface.axis.scale,
            };
            self.store
                .dispatch_now(SetViewport(surface.axis.width, Some(view)), cx);
            cx.notify();
        }
    }

    /// Fit `start..end` once the width is known (keeps the current scroll).
    pub fn fit(&mut self, start: f64, end: f64, cx: &mut Context<Self>) {
        self.pending_fit = Some((start, end));
        cx.notify();
    }

    /// The clip as the active preview would leave it.
    fn shown(&self, track: usize, clip: &Clip, model: &TracksModel) -> Clip {
        let preview = match self.preview {
            Some(TracksAction::MoveClip { clip: at, start }) if at.track == track && at.clip == clip.id => {
                model.row(track).and_then(|r| r.track.moved(clip.id, start))
            }
            Some(TracksAction::TrimClip { clip: at, edge, time })
                if at.track == track && at.clip == clip.id =>
            {
                model.row(track).and_then(|r| r.track.trimmed(clip.id, edge, time))
            }
            _ => None,
        };
        preview.unwrap_or(*clip)
    }
}

impl Observe for TracksView {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        let model = self.model(cx);
        ControlSnapshot {
            value: Some(Value::Num(model.playhead)),
            selected: model
                .selection
                .map(|s| Rc::from([s.track, s.clip as usize].as_slice())),
            disabled: Some(!self.editable),
            ..Default::default()
        }
    }
}

/// The bus outcome of a [`TracksAction`]: the [`TracksChange`], or empty for a no-op.
pub struct TracksOutcome(pub Option<TracksChange>);

impl From<TracksOutcome> for Outcome {
    fn from(o: TracksOutcome) -> Self {
        o.0.map_or(Outcome::EMPTY, |c| Outcome(c.into()))
    }
}

impl Drive for TracksView {
    type Action = TracksAction;
    type Output = TracksOutcome;
    fn decode(action: &Action) -> Result<TracksAction, ActionError> {
        TracksAction::decode(action)
    }
    fn apply(
        &mut self,
        action: TracksAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TracksOutcome, ActionError> {
        self.perform(action, cx).map(TracksOutcome)
    }
}

// At overview zoom, each column represents the first clip intersecting it.
fn overview_columns(track: &Track, axis: Axis) -> impl Iterator<Item = (usize, u32)> + '_ {
    let width = axis.width.ceil().clamp(0., 16_384.) as usize;
    let step = axis.width / width.max(1) as f64;
    let mut remaining = track.clips();
    let nearby = remaining.len() <= width * 2;
    (0..width).filter_map(move |column| {
        let start = axis.t(column as f64 * step);
        let end = axis.t((column + 1) as f64 * step);
        let expired = |clip: &crate::Clip| clip.end() <= start;
        let skip = if nearby {
            // Around one clip per column, check the next few intervals before
            // searching the tail. A discontinuous pan can still jump by index.
            let prefix = remaining.len().min(3);
            let near = remaining[..prefix]
                .iter()
                .take_while(|clip| expired(clip))
                .count();
            if near == prefix {
                near + remaining[near..].partition_point(expired)
            } else {
                near
            }
        } else {
            remaining.partition_point(expired)
        };
        remaining = &remaining[skip..];
        remaining
            .first()
            .filter(|clip| clip.start < end)
            .map(|clip| (column, clip.id))
    })
}

// Plain clips have no per-element input handlers. For populated rows the
// canvas paints the same bordered, rounded rectangles; the body owns picking.
fn canvas_clips(track: &Track, axis: &Axis) -> bool {
    crate::visible(track.clips(), axis, |c| c.start, |c| c.end()).len() >= 16
}

fn overview(track: &Track, axis: &Axis) -> bool {
    crate::visible(track.clips(), axis, |c| c.start, |c| c.end()).len() > (axis.width / 4.) as usize
}

/// A clip's rectangle inside a row whose top is `top` (body-local) and height `height`.
fn clip_rect(origin: gpui_kit::Point<Pixels>, axis: &Axis, clip: &Clip, top: f32, height: f32) -> Bounds<Pixels> {
    let left = axis.x(clip.start) as f32;
    let right = axis.x(clip.end()) as f32;
    Bounds::new(
        origin + gpui_kit::point(px(left), px(top + CLIP_INSET)),
        gpui_kit::size(px((right - left).max(2.)), px((height - 2. * CLIP_INSET).max(1.))),
    )
}

impl TracksView {
    fn mouse_down(view: &WeakEntity<Self>, e: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        // Claim focus only for a clicked item in an explicitly opted-in editor.
        window.prevent_default();
        let Some(entity) = view.upgrade() else { return };
        let (select, focus) = entity.update(cx, |this, cx| {
            let (x, y) = this.local(e.position);
            if y < 0.0 {
                return (None, false);
            }
            let content = y + this.scroll_y;
            if this.resizable
                && let Some(track) = this.layout.bottom_edge_at(content, RESIZE_GRAB)
                && this.press_resize(track, content, cx)
            {
                cx.notify();
                return (None, false);
            }
            let Some(row) = this.layout.row_at(content) else {
                return (None, false);
            };
            let previous = this.drag;
            let select = this.press_item(row, x, cx);
            if previous != this.drag {
                cx.notify();
            }
            let focus = select.is_some() && this.focusable == Some(true);
            (select, focus)
        });
        if focus {
            let handle = entity.read(cx).focus.clone();
            handle.focus(window, cx);
        }
        if let Some(select) = select {
            route(view, select, Source::Pointer, false, window, cx).ok();
        }
    }

    fn mouse_move(view: &WeakEntity<Self>, e: &MouseMoveEvent, cx: &mut App) {
        let Some(entity) = view.upgrade() else { return };
        entity.update(cx, |this, cx| {
            let (x, y) = this.local(e.position);
            if e.pressed_button == Some(MouseButton::Left) && this.drag.is_some() {
                if this.drag_preview(x, y + this.scroll_y, cx) {
                    cx.notify();
                }
            } else if y >= 0.0 {
                this.hover_at(y, x, cx);
            }
        });
    }

    fn mouse_up(view: &WeakEntity<Self>, _: &MouseUpEvent, window: &mut Window, cx: &mut App) {
        let Some(entity) = view.upgrade() else { return };
        let commit = entity.update(cx, |this, cx| {
            let commit = this.take_commit();
            cx.notify();
            commit
        });
        if let Some(action) = commit {
            route(view, action, Source::Pointer, true, window, cx).ok();
        }
    }

    fn header_column(
        &self,
        model: &TracksModel,
        rows: std::ops::Range<usize>,
        line: Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let width = self.header.width();
        if width <= 0.0 {
            return None;
        }
        let scroll = self.scroll_y;
        let selected_track = model.selection.map(|s| s.track);
        let column = div()
            .id("tracks-header")
            .relative()
            .w(px(width))
            .flex_none()
            .overflow_hidden()
            .border_r_1()
            .border_color(line);
        let cell = |i: usize| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(self.layout.top(i) - scroll))
                .h(px(self.layout.height(i)))
                .border_b_1()
                .border_color(line)
        };
        Some(match &self.header {
            HeaderMode::Hidden => return None,
            HeaderMode::Custom(_, render) => {
                let render = render.clone();
                let mut cells = Vec::with_capacity(rows.len());
                for i in rows {
                    let Some(row) = model.rows().get(i).cloned() else { continue };
                    let info = HeaderRow {
                        index: i,
                        height: self.layout.height(i),
                        selected: selected_track == Some(i),
                        row,
                    };
                    cells.push(cell(i).child(render(&info, window, cx)));
                }
                column.children(cells).into_any_element()
            }
            HeaderMode::Default => column
                .children(rows.filter_map(|i| {
                    let row = model.rows().get(i)?;
                    let first = row.track.clips().first().map(|c| c.id);
                    let locked = first.is_some_and(|id| self.is_locked(i, id));
                    Some(
                        cell(i)
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .child(crate::locks::probe(
                                "track-lock",
                                i,
                                gpui_kit::component::button::Button::new(("track-lock", i))
                                    .icon(gpui_kit::component::Icon::new(if locked {
                                        gpui_kit::assets::IconName::Lock
                                    } else {
                                        gpui_kit::assets::IconName::LockOpen
                                    }))
                                    .tab_stop(false)
                                    .tooltip("Toggle track clip locks")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let ids: Vec<_> = this
                                            .model(cx)
                                            .row(i)
                                            .map(|r| r.track.clips().iter().map(|c| c.id).collect())
                                            .unwrap_or_default();
                                        for id in ids {
                                            this.set_locked(i, id, !locked, cx);
                                        }
                                    })),
                            ))
                            .child(row.name.clone()),
                    )
                }))
                .into_any_element(),
        })
    }
}

impl Render for TracksView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let t = &theme;
        let s = t.surfaces();
        let clip_v = t.variant(None, Variant::Light);
        let ring = t.state(StateKind::FocusRing, Variant::Light, None).border;
        let ring_on = ring_visible(&self.focus, window);
        let tab = guiraffe_core::tab_stop(self.focusable);
        let selection_style = self
            .selection_style
            .unwrap_or_else(|| SelectionStyle::themed(t));
        let model = self.model(cx).clone();
        let selected = model.selection.map(|s| (s.track, s.clip));
        let accessibility = window.is_a11y_active();
        let view = cx.weak_entity();

        // Row layout: a prefix sum over heights, with a resize preview applied.
        let resizing = match self.preview {
            Some(TracksAction::ResizeTrack { track, height }) => Some((track, height)),
            _ => None,
        };
        self.layout.rebuild(model.rows().iter().enumerate().map(|(i, r)| match resizing {
            Some((track, h)) if track == i => h,
            _ => r.height,
        }));
        self.scroll_y = self.clamp_scroll(self.scroll_y);
        let scroll = self.scroll_y;
        let body_h = self.body_height();
        let rows = self.layout.rows_in(scroll, scroll + body_h.max(1.0));

        // The axis comes from the store's view range; a first fit or a width change is
        // written back once.
        let mut axis = model.axis(self.surface.axis.width, self.length);
        if let Some(w) = crate::measure::width(&self.body) {
            axis = model.axis(w, self.length);
            let mut fitted = None;
            if let Some((a, b)) = self.pending_fit.take() {
                axis.fit(a, b, 0.0);
                fitted = Some(ViewRange {
                    start: axis.start,
                    scale: axis.scale,
                });
            }
            if fitted.is_some() || model.viewport != w {
                self.store.dispatch(SetViewport(w, fitted), cx);
            }
        }
        self.surface.axis = axis;
        let line = s.border.to_hsla();
        let header = self.header_column(&model, rows.clone(), line, window, cx);
        let this: &Self = self;
        let format = this.format;
        let editable = this.editable;
        let item_style = this.lock_style;
        let global_style = this.global_lock_style;
        let locked = &this.locked;
        let layout = &this.layout;
        let body = this.body.clone();
        let owner = cx.entity_id();
        let header_w = this.header.width();
        let cursor = if this.hover_resize || matches!(this.drag, Some(Drag::Resize { .. })) {
            gpui_kit::CursorStyle::ResizeUpDown
        } else {
            crate::edit::cursor_for(
                match this.drag {
                    Some(Drag::Clip { grab, .. }) => Some(grab),
                    _ => this.hover,
                },
            )
        };
        let on = |view: &WeakEntity<Self>, f: fn(&mut Self, &mut Context<Self>) -> Option<TracksAction>| {
            let view = view.clone();
            move |window: &mut Window, cx: &mut App| {
                let Some(entity) = view.upgrade() else { return };
                if let Some(action) = entity.update(cx, |this, cx| f(this, cx)) {
                    route(&view, action, Source::Keyboard, false, window, cx).ok();
                }
            }
        };
        macro_rules! key {
            ($act:ty, $f:expr) => {{
                let run = on(&view, $f);
                move |_: &$act, window: &mut Window, cx: &mut App| run(window, cx)
            }};
        }
        let root = div()
            .id("tracks")
            .test_support()
            .role(Role::ListBox)
            .aria_label("Tracks")
            .track_focus(&this.focus.clone().tab_stop(tab))
            .key_context(KEY_CONTEXT)
            .on_action(key!(MoveEarlier, |v, cx| v.nudge_action(Nudge::Move, -1.0, cx)))
            .on_action(key!(MoveLater, |v, cx| v.nudge_action(Nudge::Move, 1.0, cx)))
            .on_action(key!(EndEarlier, |v, cx| v.nudge_action(Nudge::End, -1.0, cx)))
            .on_action(key!(EndLater, |v, cx| v.nudge_action(Nudge::End, 1.0, cx)))
            .on_action(key!(StartEarlier, |v, cx| v.nudge_action(Nudge::Start, -1.0, cx)))
            .on_action(key!(StartLater, |v, cx| v.nudge_action(Nudge::Start, 1.0, cx)))
            .on_action(key!(SelectPrevItem, |v, cx| v.select_step_action(-1, cx)))
            .on_action(key!(SelectNextItem, |v, cx| v.select_step_action(1, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(12.))
            .child(
                div()
                    .h(px(RULER_H))
                    .flex()
                    .border_b_1()
                    .border_color(line)
                    .when(header_w > 0.0, |el| {
                        el.child(
                            div()
                                .w(px(header_w))
                                .flex_none()
                                .border_r_1()
                                .border_color(line),
                        )
                    })
                    .child(
                        div()
                            .id("tracks-ruler")
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_mouse_down(MouseButton::Left, {
                                let view = view.clone();
                                move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                    let Some(entity) = view.upgrade() else { return };
                                    let time = {
                                        let this = entity.read(cx);
                                        let (x, _) = this.local(e.position);
                                        this.surface.axis.t(x)
                                    };
                                    route(&view, TracksAction::Seek(time), Source::Pointer, false, window, cx)
                                        .ok();
                                }
                            })
                            .child(this.surface.element(t.clone(), true, f64::from(TRACK_H), 0.)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .children(header)
                    .child(
                        div()
                            .id("tracks-body")
                            .test_support()
                            .debug_selector(|| "tracks-body".into())
                            .cursor(cursor)
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_scroll(
                                cx.listener(|this, e: &ScrollEvent, _, cx| this.on_wheel(e, cx)),
                            )
                            .on_mouse_down(MouseButton::Left, {
                                let view = view.clone();
                                move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                    Self::mouse_down(&view, e, window, cx)
                                }
                            })
                            .on_mouse_move({
                                let view = view.clone();
                                move |e: &MouseMoveEvent, _: &mut Window, cx: &mut App| {
                                    Self::mouse_move(&view, e, cx)
                                }
                            })
                            .on_mouse_up(MouseButton::Left, {
                                let view = view.clone();
                                move |e: &MouseUpEvent, window: &mut Window, cx: &mut App| {
                                    Self::mouse_up(&view, e, window, cx)
                                }
                            })
                            .child(
                                canvas(
                                    move |b, _, cx| crate::measure::record(&body, b, cx, owner),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(this.surface.element(t.clone(), false, 32., 0.))
                            .child(canvas(
                                |_, _, _| (),
                                {
                                    let theme = t.clone();
                                    let view = view.clone();
                                    let rows = rows.clone();
                                    move |b, _, window, cx| {
                                        let Some(view) = view.upgrade() else { return };
                                        let view = view.read(cx);
                                        let model = view.model(cx);
                                        let count = axis.width.ceil().clamp(1., 16_384.);
                                        let step = (axis.width / count) as f32;
                                        let normal = theme.variant(None, Variant::Light).background.to_hsla();
                                        let disabled = theme.state(StateKind::Disabled, Variant::Light, None).background.to_hsla();
                                        let dim = theme.state(StateKind::Dimmed, Variant::Light, None).background.to_hsla();
                                        if accessibility {
                                            return;
                                        }
                                        let layout = &view.layout;
                                        window.paint_layer(b, |window| {
                                            for row in rows.clone() {
                                                let Some(r) = model.rows().get(row) else { continue };
                                                let track = &r.track;
                                                let top = layout.top(row) - scroll;
                                                let height = layout.height(row);
                                                if !overview(track, &axis) {
                                                    if editable && canvas_clips(track, &axis) {
                                                        for clip in crate::visible(track.clips(), &axis, |c| c.start, |c| c.end()) {
                                                            if selected == Some((row, clip.id)) || view.locked.contains(&(row, clip.id)) {
                                                                continue;
                                                            }
                                                            let shown = view.shown(row, clip, model);
                                                            window.paint_quad(gpui_kit::quad(
                                                                clip_rect(b.origin, &axis, &shown, top, height),
                                                                px(4.), normal, px(1.), clip_v.foreground.to_hsla(), gpui_kit::BorderStyle::Solid,
                                                            ));
                                                        }
                                                    }
                                                    continue;
                                                }
                                                let paint = |start: usize, end: usize, ink, window: &mut Window| {
                                                    window.paint_quad(gpui_kit::fill(
                                                        Bounds::new(
                                                            b.origin + gpui_kit::point(px(start as f32 * step), px(top + CLIP_INSET)),
                                                            gpui_kit::size(px((end - start) as f32 * step), px((height - 2. * CLIP_INSET).max(1.))),
                                                        ),
                                                        ink,
                                                    ));
                                                };
                                                let mut run = None;
                                                for (column, clip) in overview_columns(track, axis) {
                                                    let style = if view.locked.contains(&(row, clip)) {
                                                        item_style
                                                    } else if !editable {
                                                        global_style
                                                    } else {
                                                        crate::LockStyle::None
                                                    };
                                                    let ink = match style {
                                                        crate::LockStyle::None => normal,
                                                        crate::LockStyle::Badge => disabled,
                                                        crate::LockStyle::Dimmed | crate::LockStyle::Hatched => dim,
                                                    };
                                                    if let Some((start, end, color)) = run {
                                                        if end == column && color == ink {
                                                            run = Some((start, column + 1, color));
                                                            continue;
                                                        }
                                                        paint(start, end, color, window);
                                                    }
                                                    run = Some((column, column + 1, ink));
                                                }
                                                if let Some((start, end, color)) = run {
                                                    paint(start, end, color, window);
                                                }
                                            }
                                        });
                                    }
                                },
                            ).absolute().size_full())
                            .children(rows.clone().flat_map(|i| {
                                let r = &model.rows()[i];
                                let tr = &r.track;
                                let top = layout.top(i) - scroll;
                                let height = layout.height(i);
                                let overview_row = !accessibility && overview(tr, &axis);
                                let clips: &[Clip] = if overview_row {
                                    selected
                                        .filter(|(row, _)| *row == i)
                                        .and_then(|(_, id)| tr.clip(id))
                                        .filter(|clip| clip.end() >= axis.start && clip.start <= axis.t(axis.width))
                                        .map_or(&[][..], std::slice::from_ref)
                                } else {
                                    crate::visible(tr.clips(), &axis, |c| c.start, |c| c.end())
                                };
                                let detailed_canvas = !overview_row && !accessibility && editable && canvas_clips(tr, &axis);
                                let name = &r.name;
                                let model = &model;
                                clips
                                    .iter()
                                    .filter(move |clip| !detailed_canvas || selected == Some((i, clip.id)) || locked.contains(&(i, clip.id)))
                                    .map(move |c| {
                                        let shown = this.shown(i, c, model);
                                        let (x0, x1) = (axis.x(shown.start) as f32, axis.x(shown.end()) as f32);
                                        let on = selected == Some((i, c.id));
                                        let is_locked = !editable || locked.contains(&(i, c.id));
                                        div()
                                            .id(clip_element_id(i, c.id))
                                            .test_support()
                                            .role(Role::ListBoxOption)
                                            .aria_label(format!(
                                                "{name} clip {}: {} – {}",
                                                c.id,
                                                label(shown.start, format),
                                                label(shown.end(), format)
                                            ))
                                            .aria_selected(on)
                                            .absolute()
                                            .top(px(top + CLIP_INSET))
                                            .left(px(x0))
                                            .w(px((x1 - x0).max(2.0)))
                                            .h(px((height - 2. * CLIP_INSET).max(1.)))
                                            .rounded(px(4.))
                                            .border_1()
                                            .bg(clip_v.background.to_hsla())
                                            .border_color(clip_v.foreground.to_hsla())
                                            .when(is_locked, |el| {
                                                if locked.contains(&(i, c.id)) {
                                                    item_style.decorate(el, t)
                                                } else {
                                                    global_style.decorate(el, t)
                                                }
                                            })
                                    })
                            }))
                            // Clip content (the painter) and the selection, over every clip.
                            .child(canvas(|_, _, _| (), {
                                let view = view.clone();
                                let rows = rows.clone();
                                move |b, _, window, cx| {
                                    let Some(view) = view.upgrade() else { return };
                                    let view = view.read(cx);
                                    let model = view.model(cx);
                                    let layout = &view.layout;
                                    window.paint_layer(b, |window| {
                                        for row in rows.clone() {
                                            let Some(r) = model.rows().get(row) else { continue };
                                            let top = layout.top(row) - scroll;
                                            let height = layout.height(row);
                                            if let Some(painter) = &view.painter
                                                && !overview(&r.track, &axis)
                                            {
                                                for clip in crate::visible(r.track.clips(), &axis, |c| c.start, |c| c.end()) {
                                                    let shown = view.shown(row, clip, model);
                                                    painter.paint(
                                                        &ClipPaint {
                                                            track: row,
                                                            row: r,
                                                            clip: &shown,
                                                            bounds: clip_rect(b.origin, &axis, &shown, top, height),
                                                            axis: &axis,
                                                            selected: selected == Some((row, clip.id)),
                                                        },
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            }
                                            if let Some((_, id)) = selected.filter(|(t, _)| *t == row)
                                                && let Some(clip) = r.track.clip(id)
                                            {
                                                let shown = view.shown(row, clip, model);
                                                let rect = clip_rect(b.origin, &axis, &shown, top, height);
                                                window.paint_quad(gpui_kit::quad(
                                                    rect,
                                                    px(4.),
                                                    selection_style.fill,
                                                    selection_style.width,
                                                    selection_style.border,
                                                    gpui_kit::BorderStyle::Solid,
                                                ));
                                            }
                                        }
                                    });
                                }
                            }).absolute().size_full())
                            .child({
                                let mut layer = crate::PlotSurface::new(axis);
                                layer.add_layer(crate::Playhead {
                                    time: model.playhead,
                                    format: this.format,
                                });
                                layer.element(t.clone(), false, f64::from(TRACK_H), 0.)
                            }),
                    ),
            );
        focus_ring(root, ring_on, px(0.), ring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clips::Edge;

    fn track() -> Track {
        let mut t = Track::default();
        t.insert(1.0, 2.0);
        t.insert(5.0, 1.0);
        t
    }

    fn axis() -> Axis {
        Axis::new(0.0, 10.0, 1000.0) // 100 px per second
    }

    #[test]
    fn overview_emits_one_mark_per_occupied_column_at_any_density() {
        for count in [100, 10_000, 100_000] {
            let axis = Axis::new(0., count as f64 * 0.001, 100.);
            let mut track = Track::default();
            for index in 0..count {
                track.insert(index as f64 * 0.001, 0.001);
            }
            assert_eq!(overview_columns(&track, axis).count(), 100);
        }
    }

    #[test]
    fn hit_distinguishes_edges_body_and_gaps() {
        let (t, a) = (track(), axis());
        let first = t.clips()[0].id;
        assert_eq!(
            hit(&t, &a, 102.0, ResizeEdges::Both),
            Some((first, Grab::Trim(Edge::Start)))
        );
        assert_eq!(
            hit(&t, &a, 297.0, ResizeEdges::Both),
            Some((first, Grab::Trim(Edge::End)))
        );
        assert!(
            matches!(hit(&t, &a, 200.0, ResizeEdges::Both), Some((id, Grab::Move { .. })) if id == first)
        );
        assert_eq!(hit(&t, &a, 400.0, ResizeEdges::Both), None);
    }

    #[test]
    fn drag_moves_with_snap_and_seek_emits() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &TracksEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            v.set_snap(Some(0.5));
            let a = *v.axis();
            let x = |t: f64| a.x(t);
            assert!(v.press(0, x(2.0), cx)); // clip body, grab_dt = 1
            v.drag_to(x(3.21), cx); // 2.21 -> snaps to 2.0
            assert_eq!(v.tracks(cx)[0].track.clips()[0].start, 1.0, "a preview until release");
            v.release(cx);
            v.seek(4.0, cx);
            v.seek(4.0, cx); // no-op
        });
        let start = view.read_with(&cx, |v, cx| v.tracks(cx)[0].track.clips()[0].start);
        assert_eq!(start, 2.0);
        let events = events.borrow();
        assert_eq!(
            *events,
            [
                TracksEvent::SelectionChanged { selected: Some((0, 1)) },
                TracksEvent::ClipChanged { track: 0, clip: 1, edit: ClipEdit::Move },
                TracksEvent::Seek(4.0),
            ]
        );
    }

    #[test]
    fn trim_end_and_press_on_missing_track() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            let x = |t: f64| a.x(t);
            assert!(!v.press(7, x(2.0), cx));
            assert!(v.press(0, x(3.0), cx));
            v.drag_to(x(4.5), cx); // end -> 4.5 (next clip at 5)
            v.drag_to(x(9.0), cx); // clamped by neighbour
            v.release(cx);
        });
        let c = view.read_with(&cx, |v, cx| v.tracks(cx)[0].track.clips()[0]);
        assert!(c.end() <= 5.0 + 1e-9 && c.end() >= 4.5 - 1e-9, "{c:?}");
    }

    #[test]
    fn short_clips_stay_movable_and_negative_seek_is_idempotent() {
        let mut t = Track::default();
        let id = t.insert(1.0, 0.05).unwrap_or_default();
        let a = axis(); // 100 px/s: clip is 5 px wide
        assert!(
            matches!(hit(&t, &a, 102.5, ResizeEdges::Both), Some((i, Grab::Move { .. })) if i == id)
        );
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        let n = Rc::new(std::cell::RefCell::new(0));
        let ev = n.clone();
        let _s =
            cx.update(|cx| cx.subscribe(&view, move |_, _: &TracksEvent, _| *ev.borrow_mut() += 1));
        view.update(&mut cx.clone(), |v, cx| {
            v.set_snap(Some(f64::INFINITY));
            v.seek(-1.0, cx);
            v.seek(-2.0, cx);
        });
        assert_eq!(*n.borrow(), 0, "already at 0");
        assert!(
            view.read_with(&cx, |v, _| v.snap.is_none()),
            "inf grid rejected"
        );
    }

    #[test]
    fn locks_and_resize_policy() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            let first = v.tracks(cx)[0].track.clips()[0].id;
            // Start-edge trim: clip [1,3] -> [1.5,3] (source offset advances; extending left
            // past the media start is refused by the model).
            assert!(v.press(0, a.x(1.0), cx));
            v.drag_to(a.x(1.5), cx);
            v.release(cx);
            let c = v.tracks(cx)[0].track.clips()[0];
            assert_eq!((c.start, c.end(), c.source_offset), (1.5, 3.0, 0.5));
            v.set_locked(0, first, true, cx);
            assert!(v.grab(0, a.x(2.0), cx).is_none());
            assert_eq!(
                v.perform(
                    TracksAction::MoveClip { clip: ClipRef { track: 0, clip: first }, start: 2.0 },
                    cx
                ),
                Err(ActionError::Disabled),
                "a locked clip refuses bus edits too"
            );
            v.set_locked(0, first, false, cx);
            v.set_resize_edges(ResizeEdges::End, cx);
            assert!(
                matches!(v.grab(0, a.x(1.5) + 1.0, cx), Some((_, Grab::Move { .. }))),
                "start edge disabled"
            );
            v.set_editable(false, cx);
            assert!(!v.press(0, a.x(2.0), cx));
            v.seek(1.0, cx);
            assert_eq!(v.playhead(cx), 1.0, "seek still works when not editable");
        });
    }
}

pub(crate) mod document;
