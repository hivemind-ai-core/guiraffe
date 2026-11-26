//! Hierarchical, frame-snapped dope sheet with viewport-culling and explicit selection.
#[path = "dope_sheet_button.rs"]
mod button;
#[path = "dope_sheet_strip.rs"]
mod strip;
use crate::plot::append_diamond;
use crate::{
    Axis, LockStyle, Playhead, PlotSurface, SelectionBox, TimeFormat, Zebra, dope_model::DopeModel,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Theme, tooltip::Tooltip};
use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Render, Role, Styled,
    TestSupportExt, Window, canvas, div, prelude::*, px,
};
use guiraffe_core::{A11yDisabledExt, ColorName, GuiraffeTheme, StateKind, Variant};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub const ROW_H: f32 = 28.;
pub const NAME_W: f32 = 230.;
pub const KEY_GRAB: f64 = 7.;
pub const RULER_H: f32 = 28.;
pub use crate::channels::{Channel, KeyShape, snap_time};
use crate::dope_store::ApplyDopeAction;
pub use crate::dope_store::{
    ChannelCurve, ChannelFlag, ChannelId, ChannelKind, DopeAction, DopeChange, DopeChannel,
    DopeDoc, DopeKey, KeyId, KeyRef, NumberFormat, ValueBounds, ValueFormatter, dope_store,
};
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
    Outcome, Source,
};
use guiraffe_state::{Selected, Store};
use std::sync::Arc;

/// Height of an expanded value channel's curve lane.
pub const LANE_H: f32 = 140.;

/// A user action was applied (one per edit). Host writes to the store never emit these.
#[derive(Debug, Clone, PartialEq)]
pub enum DopeEvent {
    KeyInserted {
        key: KeyRef,
        t: f64,
        v: f64,
    },
    KeysRemoved {
        keys: Vec<KeyRef>,
    },
    KeysRetimed {
        keys: Vec<KeyRef>,
        delta: f64,
    },
    KeysDuplicated {
        keys: Vec<KeyRef>,
        delta: f64,
    },
    KeyValueChanged {
        key: KeyRef,
        from: f64,
        to: f64,
    },
    SegmentInterpChanged {
        key: KeyRef,
        interp: crate::Interp,
    },
    SegmentBent {
        key: KeyRef,
        amount: f64,
    },
    SpanToggled {
        channel: ChannelId,
        t: f64,
        on: bool,
    },
    CurveEdited {
        channel: ChannelId,
    },
    SelectionChanged,
    ChannelChanged {
        channel: ChannelId,
        flag: ChannelFlag,
        on: bool,
    },
    Seek(f64),
}

impl From<DopeChange> for DopeEvent {
    fn from(change: DopeChange) -> Self {
        match change {
            DopeChange::KeyInserted { key, t, v } => Self::KeyInserted { key, t, v },
            DopeChange::KeysRemoved { keys } => Self::KeysRemoved { keys },
            DopeChange::KeysRetimed { keys, delta } => Self::KeysRetimed { keys, delta },
            DopeChange::KeysDuplicated { keys, delta } => Self::KeysDuplicated { keys, delta },
            DopeChange::KeyValueChanged { key, from, to } => {
                Self::KeyValueChanged { key, from, to }
            }
            DopeChange::SegmentInterpChanged { key, interp } => {
                Self::SegmentInterpChanged { key, interp }
            }
            DopeChange::SegmentBent { key, amount } => Self::SegmentBent { key, amount },
            DopeChange::SpanToggled { channel, t, on } => Self::SpanToggled { channel, t, on },
            DopeChange::CurveEdited { channel } => Self::CurveEdited { channel },
            DopeChange::SelectionChanged => Self::SelectionChanged,
            DopeChange::ChannelChanged { channel, flag, on } => {
                Self::ChannelChanged { channel, flag, on }
            }
            DopeChange::Seek { time } => Self::Seek(time),
        }
    }
}

#[derive(Clone)]
enum Gesture {
    /// Retiming the selection: a preview on the derived model until release.
    Keys {
        last: f64,
        keys: Vec<KeyRef>,
        delta: f64,
    },
    Box {
        start: (f64, f64),
        end: (f64, f32),
        add: bool,
        base: std::collections::BTreeSet<(usize, u64)>,
    },
    /// Alt-drag a value key vertically.
    Value {
        key: KeyRef,
        from_y: f32,
        from_v: f64,
        value: f64,
    },
    /// Ctrl-drag inside a value segment to bend it.
    Bend {
        key: KeyRef,
        from_y: f32,
        amount: f64,
    },
}

/// The derived render model is rebuilt per channel when the store changes; this is what it
/// was built from.
#[derive(Default)]
struct Derived {
    channels: Vec<*const DopeChannel>,
    selection: Option<*const std::collections::BTreeSet<KeyRef>>,
    /// Row tops (content y) as a prefix sum; one more entry than rows.
    tops: Vec<f64>,
}

pub struct DopeSheet {
    store: Store<DopeDoc>,
    doc: Selected<DopeDoc>,
    derived: Derived,
    /// The render and hit-test cache derived from the store's document.
    model: DopeModel,
    strip: Rc<RefCell<strip::DiamondStrip>>,
    surface: PlotSurface,
    format: TimeFormat,
    drag: Option<Gesture>,
    body: Rc<Cell<Bounds<Pixels>>>,
    pending_fit: Option<(f64, f64)>,
    offset: f64,
    editable: bool,
    focusable: Option<bool>,
    focus: FocusHandle,
    lock_style: LockStyle,
    header_column: Option<Entity<HeaderColumn>>,
    header_offset: f64,
    first_header_row: usize,
    headers: Vec<ChannelHeader>,
    id: Option<ControlId>,
    /// Keyboard row cursor (a row index).
    cursor_row: usize,
    formatter: Option<Rc<dyn ValueFormatter>>,
    /// Embedded curve editors for expanded value channels, with the channel they last
    /// showed.
    lanes: std::collections::HashMap<
        ChannelId,
        (
            Entity<crate::CurveView>,
            gpui_kit::Subscription,
            *const DopeChannel,
        ),
    >,
    readout: String,
    /// Paint value segments and toggle bars (on by default).
    segments: bool,
}
impl EventEmitter<DopeEvent> for DopeSheet {}
impl Focusable for DopeSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Route an input as a bus client would send it: through the bus when registered (a drag
/// commit as a one-action batch), else straight to [`DopeSheet::perform`].
fn route(
    view: &gpui_kit::WeakEntity<DopeSheet>,
    action: DopeAction,
    source: Source,
    batch: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = view.upgrade() else { return };
    match entity.read(cx).id {
        Some(id) => {
            let action = action.to_action();
            if batch {
                let _ = ActionBus::dispatch_batch(
                    id,
                    std::slice::from_ref(&action),
                    source,
                    window,
                    cx,
                );
            } else {
                ActionBus::dispatch(id, &action, source, window, cx).ok();
            }
        }
        None => {
            entity.update(cx, |sheet, cx| sheet.perform(action, cx).ok());
        }
    }
}

/// Several actions as one bus batch (one undo step), or applied in turn when unregistered.
fn route_many(
    view: &gpui_kit::WeakEntity<DopeSheet>,
    actions: &[DopeAction],
    source: Source,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = view.upgrade() else { return };
    match entity.read(cx).id {
        Some(id) => {
            let erased: Vec<Action> = actions.iter().map(DopeAction::to_action).collect();
            let _ = ActionBus::dispatch_batch(id, &erased, source, window, cx);
        }
        None => entity.update(cx, |sheet, cx| {
            for action in actions {
                sheet.perform(action.clone(), cx).ok();
            }
        }),
    }
}

/// Legacy channels to a document: ids follow the flattened order (channels from 0, keys
/// from 1); channels made with key values (`with_keyframes`) become value channels.
fn doc_from_channels(channels: Vec<Channel>) -> DopeDoc {
    fn flatten(
        input: Vec<Channel>,
        parent: Option<u64>,
        out: &mut Vec<DopeChannel>,
        next_key: &mut u64,
    ) {
        for mut channel in input {
            let children = std::mem::take(&mut channel.children);
            let id = out.len() as u64;
            let values: Option<Vec<(f64, KeyShape)>> = channel.decor.as_ref().map(|d| {
                d.iter()
                    .map(|(v, shape)| (v.unwrap_or(0.0), *shape))
                    .collect()
            });
            let keys: Vec<DopeKey> = channel
                .keys()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    *next_key += 1;
                    let (v, shape) = values
                        .as_ref()
                        .and_then(|v| v.get(i).copied())
                        .unwrap_or((0.0, KeyShape::Diamond));
                    let interp = match shape {
                        KeyShape::Square => crate::Interp::Constant,
                        KeyShape::Circle => crate::Interp::Bezier,
                        KeyShape::Diamond => crate::Interp::Linear,
                    };
                    DopeKey::new(*next_key, *t, v, interp)
                })
                .collect();
            let mut c = match &values {
                Some(values) => {
                    let lo = values.iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
                    let hi = values.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max);
                    let (lo, hi) = if hi > lo {
                        (lo, hi)
                    } else {
                        (lo - 0.5, lo + 0.5)
                    };
                    DopeChannel::value(id, channel.name.clone(), keys, ValueBounds::new(lo, hi))
                }
                None if channel.group || !children.is_empty() => {
                    let mut g = DopeChannel::group(id, channel.name.clone());
                    g.curve =
                        ChannelCurve::new(keys, ValueBounds::default(), crate::Interp::Linear);
                    g
                }
                None => DopeChannel::keys(id, channel.name.clone(), keys),
            };
            c.color = channel.color;
            c.parent = parent.map(ChannelId);
            c.collapsed = channel.collapsed;
            c.muted = channel.muted;
            c.locked = channel.locked;
            c.visible = channel.visible;
            out.push(c);
            flatten(children, Some(id), out, next_key);
        }
    }
    let mut out = Vec::new();
    flatten(channels, None, &mut out, &mut 0);
    DopeDoc::new(out)
}

/// The render-cache channel for a document channel.
fn derive_channel(c: &DopeChannel, parent: Option<usize>) -> Channel {
    let mut ch = Channel::new(
        c.name.to_string(),
        c.curve.keys().iter().map(|k| k.key.t).collect(),
    );
    ch.color = c.color;
    ch.parent = parent;
    ch.group = c.group;
    ch.collapsed = c.collapsed;
    ch.muted = c.muted;
    ch.locked = c.locked;
    ch.visible = c.visible;
    // Value and toggle channels paint their own segments; the shape marks the mode.
    ch.decor = match c.kind {
        ChannelKind::Keys => None,
        ChannelKind::Value => Some(
            c.curve
                .keys()
                .iter()
                .map(|k| {
                    let shape = match k.key.interp {
                        crate::Interp::Constant => KeyShape::Square,
                        crate::Interp::Linear => KeyShape::Diamond,
                        _ => KeyShape::Circle,
                    };
                    (None, shape)
                })
                .collect(),
        ),
        ChannelKind::Toggle => Some(vec![(None, KeyShape::Square); c.curve.keys().len()]),
    };
    ch
}

impl DopeSheet {
    /// A sheet over a new store holding `channels` (ids follow their flattened order), with
    /// 100 steps of undo.
    pub fn new(
        channels: Vec<Channel>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        let store = dope_store(doc_from_channels(channels), 100, cx);
        Self::with_store(store, format, length, cx)
    }

    /// A sheet over the host's store (see [`dope_store`]).
    pub fn with_store(
        store: Store<DopeDoc>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let doc = store.select(cx, |d: &DopeDoc| d.clone());
            let mut sheet = Self {
                strip: {
                    let strip = Rc::new(RefCell::new(strip::DiamondStrip::default()));
                    let owned = strip.clone();
                    cx.on_release(move |_, cx| owned.borrow_mut().release(cx))
                        .detach();
                    strip
                },
                store,
                doc,
                derived: Derived::default(),
                model: DopeModel::from_flat(Vec::new()),
                surface: PlotSurface::timeline(Axis::new(0., length.max(1e-3), 800.), format),
                format,
                drag: None,
                body: Rc::default(),
                pending_fit: Some((0., length.max(1e-3))),
                offset: 0.,
                editable: true,
                focusable: None,
                focus: cx.focus_handle(),
                lock_style: LockStyle::Badge,
                header_column: None,
                header_offset: 0.,
                first_header_row: 0,
                headers: Vec::new(),
                id: None,
                cursor_row: 0,
                formatter: None,
                lanes: Default::default(),
                readout: String::new(),
                segments: true,
            };
            sheet.sync(cx);
            sheet
        })
    }

    /// Register on the window's action bus at `path`: pointer and keyboard input then go
    /// through the bus, so interceptors see them.
    pub fn register(view: &Entity<Self>, path: &str, window: &Window, cx: &mut App) -> ControlId {
        let id = ActionBus::register(path, view, "dope-sheet", Capabilities::SELECT, window, cx);
        view.update(cx, |v, _| v.id = Some(id));
        id
    }

    pub fn control_id(&self) -> Option<ControlId> {
        self.id
    }

    pub fn store(&self) -> &Store<DopeDoc> {
        &self.store
    }

    /// The document as of the last store flush.
    pub fn doc<'a>(&self, cx: &'a App) -> &'a DopeDoc {
        self.doc.get(cx)
    }

    /// Rebuild the render cache from the document: only channels whose allocation changed.
    fn sync(&mut self, cx: &App) {
        let doc = self.doc.get(cx);
        let channels = doc.channels();
        let same_shape = channels.len() == self.derived.channels.len()
            && channels.len() == self.model.channels.len();
        let mut rows_changed = !same_shape;
        if same_shape {
            for (i, c) in channels.iter().enumerate() {
                let ptr = Arc::as_ptr(c);
                if self.derived.channels[i] != ptr {
                    let parent = c.parent.and_then(|p| doc.index_of(p));
                    let old = &self.model.channels[i];
                    rows_changed |= old.collapsed != c.collapsed
                        || old.visible != c.visible
                        || old.parent != parent;
                    self.model.channels[i] = derive_channel(c, parent);
                    self.derived.channels[i] = ptr;
                }
            }
            if rows_changed {
                self.model.rebuild_rows();
            }
            self.model.rebuild_summaries();
        } else {
            let derived: Vec<Channel> = channels
                .iter()
                .map(|c| derive_channel(c, c.parent.and_then(|p| doc.index_of(p))))
                .collect();
            self.model = DopeModel::from_flat(derived);
            self.derived.channels = channels.iter().map(Arc::as_ptr).collect();
        }
        let selection = Arc::as_ptr(&doc.selection);
        if rows_changed
            || !same_shape
            || self.derived.selection != Some(selection)
            || self.drag.is_none()
        {
            self.model.selected = doc
                .selection
                .iter()
                .filter_map(|k| {
                    let i = doc.index_of(k.channel)?;
                    let t = doc.channels()[i].curve.key(k.key)?.key.t;
                    Some((i, t.to_bits()))
                })
                .collect();
            self.derived.selection = Some(selection);
        }
        // Row tops: expanded value channels take a curve lane.
        self.derived.tops.clear();
        self.derived.tops.push(0.);
        let mut y = 0.;
        for row in self.model.rows() {
            y += f64::from(Self::row_height(&channels[row.channel]));
            self.derived.tops.push(y);
        }
        self.cursor_row = self
            .cursor_row
            .min(self.model.rows().len().saturating_sub(1));
    }

    fn row_height(c: &DopeChannel) -> f32 {
        if c.expanded && c.kind == ChannelKind::Value {
            ROW_H + LANE_H
        } else {
            ROW_H
        }
    }

    /// Whether the cache is behind the document (a store flush happened since).
    fn stale(&self, cx: &App) -> bool {
        let doc = self.doc.get(cx);
        doc.channels().len() != self.derived.channels.len()
            || doc
                .channels()
                .iter()
                .zip(&self.derived.channels)
                .any(|(c, p)| Arc::as_ptr(c) != *p)
            || self.derived.selection != Some(Arc::as_ptr(&doc.selection))
    }

    /// Rebuild the whole cache, dropping any preview.
    fn resync(&mut self, cx: &App) {
        self.derived.channels.clear();
        self.derived.selection = None;
        self.sync(cx);
    }

    /// Apply a user action: what the bus, the pointer and the keyboard all run. Emits one
    /// [`DopeEvent`] and returns the change, or `None` for a no-op or refused edit.
    pub fn perform(
        &mut self,
        action: DopeAction,
        cx: &mut Context<Self>,
    ) -> Result<Option<DopeChange>, ActionError> {
        if !self.editable && action.undoable() {
            return Err(ActionError::Disabled);
        }
        let mut next = self.doc(cx).clone();
        let Some(change) = next.apply(action.clone()) else {
            return Ok(None);
        };
        self.store.dispatch_now(ApplyDopeAction(action), cx);
        self.sync(cx);
        cx.emit(DopeEvent::from(change.clone()));
        cx.notify();
        Ok(Some(change))
    }

    /// The selected keys, as stable references.
    pub fn selected_keys(&self, cx: &App) -> Vec<KeyRef> {
        self.doc(cx).selection.iter().copied().collect()
    }

    /// Show or hide value segments and toggle bars (keys always show).
    pub fn set_show_segments(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.segments != on {
            self.segments = on;
            cx.notify();
        }
    }

    /// Format values with `formatter` instead of each channel's [`NumberFormat`].
    pub fn set_value_formatter(
        &mut self,
        formatter: Option<Rc<dyn ValueFormatter>>,
        cx: &mut Context<Self>,
    ) {
        self.formatter = formatter;
        cx.notify();
    }

    /// Expand a value channel into a curve-editor lane (a user action; see
    /// [`ChannelFlag::Expanded`]).
    pub fn set_expanded(&mut self, channel: ChannelId, expanded: bool, cx: &mut Context<Self>) {
        self.perform(
            DopeAction::SetChannelFlag {
                channel,
                flag: ChannelFlag::Expanded,
                on: expanded,
            },
            cx,
        )
        .ok();
    }

    /// Legacy channel view of the document (display order, `parent` as indices).
    pub fn channels(&self) -> &[Channel] {
        &self.model.channels
    }
    pub fn axis(&self) -> &Axis {
        &self.surface.axis
    }
    /// The render cache: rows, summaries and the selection as display indices.
    pub fn model(&self) -> &DopeModel {
        &self.model
    }
    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn set_focusable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.focusable = Some(on);
        cx.notify();
    }
    pub fn set_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.editable = on;
        self.drag = None;
        cx.notify();
    }
    /// Lock or unlock a channel by display index (a user action).
    pub fn set_locked(&mut self, channel: usize, locked: bool, cx: &mut Context<Self>) {
        if let Some(id) = self.doc(cx).channels().get(channel).map(|c| c.id) {
            self.drag = None;
            self.perform(
                DopeAction::SetChannelFlag {
                    channel: id,
                    flag: ChannelFlag::Locked,
                    on: locked,
                },
                cx,
            )
            .ok();
        }
    }
    pub fn set_lock_style(&mut self, style: LockStyle, cx: &mut Context<Self>) {
        self.lock_style = style;
        cx.notify();
    }
    fn all_keys(&self, cx: &App) -> Vec<KeyRef> {
        self.doc(cx)
            .channels()
            .iter()
            .filter(|c| !c.group)
            .flat_map(|c| {
                c.curve.keys().iter().map(|k| KeyRef {
                    channel: c.id,
                    key: k.id,
                })
            })
            .collect()
    }
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let keys = self.all_keys(cx);
        self.perform(DopeAction::Select { keys, add: false }, cx)
            .ok();
    }
    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let keys = self.selected_keys(cx);
        self.perform(DopeAction::RemoveKeys(keys), cx).ok();
    }
    pub fn nudge(&mut self, frames: f64, cx: &mut Context<Self>) {
        let keys = self.selected_keys(cx);
        let delta = frames * self.frame_step();
        self.perform(DopeAction::RetimeKeys { keys, delta }, cx)
            .ok();
    }
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        let keys = self.selected_keys(cx);
        let delta = self.frame_step();
        self.perform(DopeAction::DuplicateKeys { keys, delta }, cx)
            .ok();
    }
    /// A user seek (emits [`DopeEvent::Seek`]); hosts write `SetPlayhead` instead.
    pub fn seek(&mut self, t: f64, cx: &mut Context<Self>) {
        self.perform(DopeAction::Seek(snap_time(t, self.format)), cx)
            .ok();
    }
    pub fn playhead(&self, cx: &App) -> f64 {
        self.doc(cx).playhead
    }
    fn frame_step(&self) -> f64 {
        match self.format {
            TimeFormat::Frames(fps) if fps > 0. => 1. / fps,
            _ => 1.,
        }
    }

    /// The document key for a derived (index, time) pair.
    fn key_at(&self, channel: usize, t: f64, cx: &App) -> Option<KeyRef> {
        let c = self.doc(cx).channels().get(channel)?;
        Some(KeyRef {
            channel: c.id,
            key: c.curve.at_time(t)?.id,
        })
    }

    /// The derived selection as document keys.
    fn derived_selection(&self, cx: &App) -> Vec<KeyRef> {
        self.model
            .selected
            .iter()
            .filter_map(|(c, t)| self.key_at(*c, f64::from_bits(*t), cx))
            .collect()
    }

    pub fn press(&mut self, row: usize, x: f64, cx: &mut Context<Self>) -> bool {
        self.press_with(row, x, false, cx)
    }
    pub fn press_with(&mut self, row: usize, x: f64, add: bool, cx: &mut Context<Self>) -> bool {
        let hit = self.press_key(row, x, add, cx).is_some();
        if hit {
            let keys = self.derived_selection(cx);
            self.perform(DopeAction::Select { keys, add: false }, cx)
                .ok();
        }
        hit
    }

    // On a hit, report whether the selection needs repainting. Starting a
    // drag on an already selected key changes only the pending gesture.
    fn press_key(&mut self, row: usize, x: f64, add: bool, cx: &App) -> Option<bool> {
        let channel = self.model.rows().get(row).map(|r| r.channel).unwrap_or(row);
        let nearest = self
            .model
            .channels
            .get(channel)
            .filter(|c| c.visible)
            .and_then(|c| {
                let keys = if c.group {
                    self.model.summary_keys(channel)
                } else {
                    c.keys()
                };
                let axis = self.surface.axis;
                let index = keys.partition_point(|time| axis.x(*time) < x);
                [
                    index.checked_sub(1).and_then(|i| keys.get(i)),
                    keys.get(index),
                ]
                .into_iter()
                .flatten()
                .copied()
                .map(|t| (t, (axis.x(t) - x).abs()))
                .filter(|(_, distance)| *distance <= KEY_GRAB)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            });
        let (t, _) = nearest?;
        let changed = add || !self.model.selected.contains(&(channel, t.to_bits()));
        if changed {
            if self.model.channels[channel].group {
                self.model.select_summary(channel, t, add);
            } else {
                self.model.select(channel, t, add);
            }
        }
        self.drag = (self.editable && self.model.editable(channel)).then(|| Gesture::Keys {
            last: self.surface.axis.t(x),
            keys: self.derived_selection(cx),
            delta: 0.,
        });
        Some(changed)
    }

    /// Continue a key drag to body-local `x`: a preview until [`DopeSheet::release`].
    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some(Gesture::Keys { last, keys, delta }) = self.drag.clone() else {
            return;
        };
        if !self.editable {
            return;
        }
        let step = snap_time(self.surface.axis.t(x) - last, self.format);
        if self.model.translate(step, self.format) {
            self.drag = Some(Gesture::Keys {
                last: last + step,
                keys,
                delta: delta + step,
            });
            cx.notify();
        }
    }

    /// The action a finished gesture commits.
    fn take_commit(&mut self, cx: &App) -> Option<DopeAction> {
        match self.drag.take()? {
            Gesture::Keys { keys, delta, .. } => {
                (delta != 0.).then_some(DopeAction::RetimeKeys { keys, delta })
            }
            Gesture::Box { .. } => Some(DopeAction::Select {
                keys: self.derived_selection(cx),
                add: false,
            }),
            Gesture::Value {
                key, value, from_v, ..
            } => (value != from_v).then_some(DopeAction::SetKeyValue { key, value }),
            Gesture::Bend { key, amount, .. } => {
                (amount != 0.).then_some(DopeAction::BendSegment { key, amount })
            }
        }
    }

    /// End the gesture and commit it (one action).
    pub fn release(&mut self, cx: &mut Context<Self>) {
        if let Some(action) = self.take_commit(cx) {
            self.perform(action, cx).ok();
        }
        // The preview is dropped: rebuild the cache from the document (a refused or empty
        // commit leaves it as it was).
        self.resync(cx);
        cx.notify();
    }
    fn local(&self, p: gpui_kit::Point<Pixels>) -> (f64, f32) {
        let b = self.body.get();
        (
            f64::from(f32::from(p.x - b.origin.x)),
            f32::from(p.y - b.origin.y),
        )
    }
    /// The row at body-local content y (offset applied), by the row tops.
    fn row_at(&self, content_y: f64) -> Option<usize> {
        let tops = &self.derived.tops;
        if !(content_y >= 0. && tops.last().is_some_and(|t| content_y < *t)) {
            return None;
        }
        Some(tops.partition_point(|t| *t <= content_y) - 1)
    }
    fn flag_action(&self, channel: usize, kind: ChannelControl, cx: &App) -> Option<DopeAction> {
        let c = self.doc(cx).channels().get(channel)?;
        let (flag, on) = match kind {
            ChannelControl::Expand => (ChannelFlag::Collapsed, !c.collapsed),
            ChannelControl::Mute => (ChannelFlag::Muted, !c.muted),
            ChannelControl::Lock => (ChannelFlag::Locked, !c.locked),
            ChannelControl::Visibility => (ChannelFlag::Visible, !c.visible),
            ChannelControl::Curve => (ChannelFlag::Expanded, !c.expanded),
        };
        Some(DopeAction::SetChannelFlag {
            channel: c.id,
            flag,
            on,
        })
    }
}

impl DopeSheet {
    /// The doc channel shown on display row `row`.
    fn row_channel<'a>(&self, row: usize, cx: &'a App) -> Option<(usize, &'a DopeChannel)> {
        let index = self.model.rows().get(row)?.channel;
        Some((index, &**self.doc(cx).channels().get(index)?))
    }

    /// The value key that starts the segment under time `t` on `channel`.
    fn segment_at(c: &DopeChannel, t: f64) -> Option<KeyRef> {
        let keys = c.curve.keys();
        let i = keys.partition_point(|k| k.key.t <= t).checked_sub(1)?;
        keys.get(i + 1)?;
        Some(KeyRef {
            channel: c.id,
            key: keys[i].id,
        })
    }

    /// Right-click on a value segment: the next interpolation mode.
    fn segment_interp_action(&self, e: &MouseDownEvent, cx: &App) -> Option<DopeAction> {
        let (x, y) = self.local(e.position);
        let row = self.row_at(f64::from(y) + self.offset)?;
        let (_, c) = self.row_channel(row, cx)?;
        if c.kind != ChannelKind::Value {
            return None;
        }
        let key = Self::segment_at(c, self.surface.axis.t(x))?;
        let interp = c.curve.key(key.key)?.key.interp.next();
        Some(DopeAction::SetSegmentInterp { key, interp })
    }

    fn mouse_down(
        view: &gpui_kit::WeakEntity<Self>,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(sheet) = view.upgrade() else { return };
        let action = sheet.update(cx, |this, cx| {
            let (x, y) = this.local(e.position);
            let content = f64::from(y) + this.offset;
            let Some(row) = this.row_at(content) else {
                this.start_box(x, y, e.modifiers.shift);
                cx.notify();
                return None;
            };
            this.cursor_row = row;
            let t = snap_time(this.surface.axis.t(x), this.format).max(0.);
            if let Some(repaint) = this.press_key(row, x, e.modifiers.shift, cx) {
                // Alt on a value key edits its value instead of its time.
                let value_key = e.modifiers.alt
                    && this.editable
                    && this
                        .row_channel(row, cx)
                        .is_some_and(|(_, c)| c.kind == ChannelKind::Value);
                if value_key
                    && let Some(&(index, bits)) = this.model.selected.iter().next()
                    && let Some(key) = this.key_at(index, f64::from_bits(bits), cx)
                    && let Some(v) = this.doc(cx).key(key).map(|k| k.key.v)
                {
                    this.drag = Some(Gesture::Value {
                        key,
                        from_y: y,
                        from_v: v,
                        value: v,
                    });
                }
                if !repaint {
                    return None; // already selected: only the gesture starts
                }
                cx.notify();
                let keys = this.derived_selection(cx);
                return Some(DopeAction::Select { keys, add: false });
            }
            let (_, c) = this.row_channel(row, cx)?;
            let kind = c.kind;
            let id = c.id;
            if e.click_count >= 2 && kind != ChannelKind::Toggle && !c.group {
                return Some(DopeAction::InsertKey {
                    channel: id,
                    t,
                    value: None,
                });
            }
            if kind == ChannelKind::Toggle && !c.group {
                return Some(DopeAction::ToggleSpan { channel: id, t });
            }
            if kind == ChannelKind::Value
                && (e.modifiers.control || e.modifiers.platform)
                && let Some(key) = Self::segment_at(c, this.surface.axis.t(x))
            {
                this.drag = Some(Gesture::Bend {
                    key,
                    from_y: y,
                    amount: 0.,
                });
                return None;
            }
            this.start_box(x, y, e.modifiers.shift);
            cx.notify();
            None
        });
        if let Some(action) = action {
            route(view, action, Source::Pointer, false, window, cx);
        }
    }

    fn start_box(&mut self, x: f64, y: f32, add: bool) {
        if !add {
            self.model.selected.clear();
        }
        self.drag = Some(Gesture::Box {
            start: (self.surface.axis.t(x), f64::from(y) + self.offset),
            end: (x, y),
            add,
            base: self.model.selected.clone(),
        });
    }

    fn drag_move(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        let (x, y) = self.local(e.position);
        match &mut self.drag {
            Some(Gesture::Box {
                start,
                end,
                add,
                base,
            }) => {
                *end = (x, y);
                let a = self
                    .derived
                    .tops
                    .partition_point(|t| *t <= start.1)
                    .saturating_sub(1);
                let b = self
                    .derived
                    .tops
                    .partition_point(|t| *t <= f64::from(y) + self.offset)
                    .saturating_sub(1);
                if *add {
                    self.model.selected = base.clone();
                }
                let (t0, add) = (start.0, *add);
                self.model
                    .box_select(a.min(b)..=a.max(b), t0, self.surface.axis.t(x), add);
                cx.notify();
            }
            Some(Gesture::Value {
                key,
                from_y,
                from_v,
                value,
            }) => {
                let bounds = self
                    .doc
                    .get(cx)
                    .channel(key.channel)
                    .map(|c| c.curve.bounds)
                    .unwrap_or_default();
                // The full row height spans the channel's range.
                let span = bounds.hi - bounds.lo;
                let next = bounds.clamp(*from_v - f64::from(y - *from_y) / f64::from(ROW_H) * span);
                if next != *value {
                    *value = next;
                    cx.notify();
                }
            }
            Some(Gesture::Bend { from_y, amount, .. }) => {
                let next = (f64::from(*from_y - y) / 40.).clamp(-1., 1.);
                if next != *amount {
                    *amount = next;
                    cx.notify();
                }
            }
            Some(Gesture::Keys { .. }) => self.drag_to(x, cx),
            None => {}
        }
    }

    /// The keyboard map. Arrow up/down move the row cursor; `[`/`]` select the previous or
    /// next key on it; `,`/`.` step the playhead a frame; `K` or Enter inserts at the
    /// playhead (toggles a toggle channel there); `I` cycles the selected segments'
    /// interpolation; Left/Right nudge, Delete removes, Ctrl+A selects all, Ctrl+D
    /// duplicates.
    fn key_actions(
        &mut self,
        e: &gpui_kit::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> Vec<DopeAction> {
        let m = &e.keystroke.modifiers;
        let command = m.control || m.platform;
        let keys = self.selected_keys(cx);
        let step = self.frame_step();
        let playhead = self.doc(cx).playhead;
        let row = self
            .row_channel(self.cursor_row, cx)
            .map(|(_, c)| c.clone());
        match e.keystroke.key.as_str() {
            "delete" | "backspace" => vec![DopeAction::RemoveKeys(keys)],
            "left" => vec![DopeAction::RetimeKeys { keys, delta: -step }],
            "right" => vec![DopeAction::RetimeKeys { keys, delta: step }],
            "a" if command => vec![DopeAction::Select {
                keys: self.all_keys(cx),
                add: false,
            }],
            "d" if command => vec![DopeAction::DuplicateKeys { keys, delta: step }],
            "up" | "down" => {
                let last = self.model.rows().len().saturating_sub(1);
                let next = if e.keystroke.key == "up" {
                    self.cursor_row.saturating_sub(1)
                } else {
                    (self.cursor_row + 1).min(last)
                };
                if next != self.cursor_row {
                    self.cursor_row = next;
                    self.scroll_to_row(next);
                    cx.notify();
                }
                vec![]
            }
            "," => vec![DopeAction::Seek(snap_time(
                (playhead - step).max(0.),
                self.format,
            ))],
            "." => vec![DopeAction::Seek(snap_time(playhead + step, self.format))],
            "[" | "]" => {
                let Some(c) = row.filter(|c| !c.group) else {
                    return vec![];
                };
                let ks = c.curve.keys();
                let target = if e.keystroke.key == "]" {
                    ks.iter().find(|k| k.key.t > playhead)
                } else {
                    ks.iter().rev().find(|k| k.key.t < playhead)
                };
                target.map_or(vec![], |k| {
                    vec![
                        DopeAction::Select {
                            keys: vec![KeyRef {
                                channel: c.id,
                                key: k.id,
                            }],
                            add: false,
                        },
                        DopeAction::Seek(k.key.t),
                    ]
                })
            }
            "k" | "enter" => match row.filter(|c| !c.group) {
                Some(c) if c.kind == ChannelKind::Toggle => vec![DopeAction::ToggleSpan {
                    channel: c.id,
                    t: playhead,
                }],
                Some(c) => vec![DopeAction::InsertKey {
                    channel: c.id,
                    t: playhead,
                    value: None,
                }],
                None => vec![],
            },
            "i" => {
                let doc = self.doc(cx);
                keys.iter()
                    .filter_map(|k| {
                        let c = doc.channel(k.channel)?;
                        let key = c.curve.key(k.key)?;
                        (c.kind == ChannelKind::Value).then(|| DopeAction::SetSegmentInterp {
                            key: *k,
                            interp: key.key.interp.next(),
                        })
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    fn scroll_to_row(&mut self, row: usize) {
        let (Some(top), Some(bottom)) =
            (self.derived.tops.get(row), self.derived.tops.get(row + 1))
        else {
            return;
        };
        let h = f64::from(f32::from(self.body.get().size.height)).max(1.);
        if *top < self.offset {
            self.offset = *top;
        } else if *bottom > self.offset + h {
            self.offset = bottom - h;
        }
    }

    /// The row the keyboard acts on.
    pub fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    /// The live value while Alt-dragging a key, or the bend amount, formatted.
    fn readout_element(&mut self, cx: &App) -> Option<gpui_kit::AnyElement> {
        let (channel, value) = match &self.drag {
            Some(Gesture::Value { key, value, .. }) => (key.channel, *value),
            Some(Gesture::Bend { amount, .. }) => {
                use std::fmt::Write;
                self.readout.clear();
                let _ = write!(self.readout, "bend {amount:+.2}");
                return Some(readout_label(self.readout.clone()));
            }
            _ => return None,
        };
        let c = self.doc(cx).channel(channel)?.clone();
        match &self.formatter {
            Some(f) => f.format(&c, value, &mut self.readout),
            None => c.format.write(value, &mut self.readout),
        }
        Some(readout_label(self.readout.clone()))
    }

    /// Curve editors for expanded value channels among rows `start..end`, kept in sync with
    /// the document; their edits commit as one `SetCurve` action each.
    fn lane_elements(
        &mut self,
        start: usize,
        end: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        let doc = self.doc(cx).clone();
        let axis = self.surface.axis;
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for row in start..end {
            let Some(index) = self.model.rows().get(row).map(|r| r.channel) else {
                continue;
            };
            let Some(c) = doc.channels().get(index) else {
                continue;
            };
            if !(c.expanded && c.kind == ChannelKind::Value) {
                continue;
            }
            keep.push(c.id);
            let curve = || {
                let mut curve = crate::Curve::default();
                for k in c.curve.keys() {
                    curve.set(k.key);
                }
                curve
            };
            let ptr = Arc::as_ptr(c);
            let lane = match self.lanes.get_mut(&c.id) {
                Some((lane, _, seen)) => {
                    if *seen != ptr {
                        *seen = ptr;
                        lane.update(cx, |l, cx| l.set_curve(curve(), cx));
                    }
                    lane.clone()
                }
                None => {
                    let lane = crate::CurveView::new(
                        curve(),
                        axis.start,
                        axis.end(),
                        c.curve.bounds.lo,
                        c.curve.bounds.hi,
                        cx,
                    );
                    let channel = c.id;
                    let sheet = cx.weak_entity();
                    let subscription = cx.subscribe_in(
                        &lane,
                        window,
                        move |_, lane, e: &crate::CurveEvent, window, cx| {
                            if matches!(e, crate::CurveEvent::Changed | crate::CurveEvent::Released)
                            {
                                let keys = lane.read(cx).curve().keys().to_vec();
                                let sheet = sheet.clone();
                                window.defer(cx, move |window, cx| {
                                    route(
                                        &sheet,
                                        DopeAction::SetCurve { channel, keys },
                                        Source::Pointer,
                                        true,
                                        window,
                                        cx,
                                    );
                                });
                            }
                        },
                    );
                    self.lanes.insert(c.id, (lane.clone(), subscription, ptr));
                    lane
                }
            };
            lane.update(cx, |l, cx| {
                l.set_view(
                    axis.start,
                    axis.scale,
                    c.curve.bounds.lo,
                    c.curve.bounds.hi,
                    cx,
                )
            });
            let top = (self.derived.tops[row] - self.offset) as f32 + ROW_H;
            out.push(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(top))
                    .h(px(LANE_H))
                    .child(lane)
                    .into_any_element(),
            );
        }
        self.lanes.retain(|id, _| keep.contains(id));
        out
    }
}

fn readout_label(text: String) -> gpui_kit::AnyElement {
    div()
        .absolute()
        .top(px(4.))
        .right(px(8.))
        .px(px(6.))
        .rounded(px(4.))
        .text_size(px(11.))
        .child(text)
        .into_any_element()
}

impl Observe for DopeSheet {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: Some(guiraffe_core::widget_value::Value::Num(
                self.doc(cx).playhead,
            )),
            disabled: Some(!self.editable),
            ..Default::default()
        }
    }
}

/// The bus outcome of a [`DopeAction`]: the [`DopeChange`], or empty for a no-op.
pub struct DopeOutcome(pub Option<DopeChange>);

impl From<DopeOutcome> for Outcome {
    fn from(o: DopeOutcome) -> Self {
        o.0.map_or(Outcome::EMPTY, |c| Outcome((&c).into()))
    }
}

impl Drive for DopeSheet {
    type Action = DopeAction;
    type Output = DopeOutcome;
    fn decode(action: &Action) -> Result<DopeAction, ActionError> {
        DopeAction::decode(action)
    }
    fn apply(
        &mut self,
        action: DopeAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<DopeOutcome, ActionError> {
        self.perform(action, cx).map(DopeOutcome)
    }
}
#[derive(Clone, PartialEq)]
struct ChannelHeader {
    channel: usize,
    depth: usize,
    name: gpui_kit::SharedString,
    color: ColorName,
    group: bool,
    collapsed: bool,
    muted: bool,
    locked: bool,
    visible: bool,
    /// A value channel (it can expand into a curve lane).
    value: bool,
    expanded: bool,
    /// Top relative to the first visible row.
    y: f32,
    owner: gpui_kit::WeakEntity<DopeSheet>,
}

#[derive(Clone, Copy)]
enum ChannelControl {
    Expand,
    Mute,
    Lock,
    Visibility,
    /// Expand a value channel into a curve lane.
    Curve,
}

impl ChannelHeader {
    fn control(&self, kind: ChannelControl, theme: &Theme) -> impl IntoElement + use<> {
        let (id, label, icon, selected, disabled) = match kind {
            ChannelControl::Expand => (
                "channel-expand",
                "Expand or collapse channel",
                if !self.group {
                    IconName::Minus
                } else if self.collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                },
                false,
                !self.group,
            ),
            ChannelControl::Mute => (
                "channel-mute",
                "Mute channel",
                if self.muted {
                    IconName::VolumeX
                } else {
                    IconName::Volume2
                },
                self.muted,
                false,
            ),
            ChannelControl::Lock => (
                "channel-lock",
                "Lock channel",
                if self.locked {
                    IconName::Lock
                } else {
                    IconName::LockOpen
                },
                self.locked,
                false,
            ),
            ChannelControl::Visibility => (
                "channel-visible",
                "Channel visibility",
                if self.visible {
                    IconName::Eye
                } else {
                    IconName::EyeOff
                },
                false,
                false,
            ),
            ChannelControl::Curve => (
                "channel-curve",
                "Show the curve editor",
                IconName::PanelBottom,
                self.expanded,
                !self.value,
            ),
        };
        let owner = self.owner.clone();
        let channel = self.channel;
        let foreground = if disabled {
            theme.muted_foreground.opacity(0.5)
        } else {
            theme.secondary_foreground
        };
        let hover = theme
            .tokens
            .accent
            .opacity(if theme.mode.is_dark() { 0.5 } else { 1. });
        let hover_foreground = theme.accent_foreground;
        let active = theme.tokens.button_active;
        let radius = theme.radius;
        let inner = div()
            .debug_selector(move || format!("{id}-{channel}"))
            .id((id, channel))
            .role(Role::Button)
            .aria_description(label)
            .aria_selected(selected)
            .size_5()
            .flex_shrink_0()
            .when(selected, |el| {
                el.bg(theme.tokens.secondary_active).rounded(radius)
            })
            .when(!disabled && !selected, |el| {
                el.hover(move |style| style.bg(hover).rounded(radius))
                    .active(move |style| style.bg(active).rounded(radius))
            })
            .when(!disabled, |el| {
                el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |_, window, cx| {
                        let Some(sheet) = owner.upgrade() else { return };
                        let action = sheet.read(cx).flag_action(channel, kind, cx);
                        if let Some(action) = action {
                            route(&owner, action, Source::Pointer, false, window, cx);
                        }
                    })
            })
            .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            .a11y_disabled(disabled);
        button::ChannelButton {
            inner,
            icon: icon.path(),
            foreground,
            hover_foreground: (!disabled && !selected).then_some(hover_foreground),
        }
    }
}

struct HeaderColumn {
    rows: Vec<ChannelHeader>,
    top: f32,
}

impl Render for HeaderColumn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .children(self.rows.iter().map(|header| {
                header
                    .element(cx)
                    .absolute()
                    .top(px(self.top + header.y))
                    .w(px(NAME_W))
                    .h(px(ROW_H))
            }))
    }
}

impl ChannelHeader {
    fn element(&self, cx: &App) -> gpui_kit::Div {
        let theme = GuiraffeTheme::global(cx);
        let control_theme = cx.theme();
        div()
            .size_full()
            .flex()
            .items_center()
            .pl(px(self.depth as f32 * 12.))
            .child(self.control(ChannelControl::Expand, control_theme))
            .child(
                div()
                    .w(px(4.))
                    .h(px(12.))
                    .rounded(px(2.))
                    .mr(px(4.))
                    .bg(theme
                        .variant(Some(self.color), Variant::Filled)
                        .background
                        .to_hsla()),
            )
            .child(div().flex_1().overflow_hidden().child(self.name.clone()))
            .child(self.control(ChannelControl::Mute, control_theme))
            .child(self.control(ChannelControl::Lock, control_theme))
            .child(self.control(ChannelControl::Visibility, control_theme))
            .when(self.value, |el| {
                el.child(self.control(ChannelControl::Curve, control_theme))
            })
    }
}

fn key_indices(
    times: &[f64],
    first: usize,
    end: usize,
    spacing: Option<f64>,
) -> impl Iterator<Item = usize> + Clone + '_ {
    std::iter::successors((first < end).then_some(first), move |&index| {
        let next = index + 1;
        // Preserve the source sequence for picking and editing. Only the
        // rendered marks skip diamonds that overlap at this zoom.
        let next = if let Some(spacing) = spacing
            && next < end
            && times[next] < times[index] + spacing
        {
            next + times[next..end].partition_point(|&t| t < times[index] + spacing)
        } else {
            next
        };
        (next < end).then_some(next)
    })
}

#[derive(Clone, Copy)]
struct KeyMark {
    x: f64,
    y: f64,
    selected: bool,
    shape: KeyShape,
    hold: Option<f64>,
    muted: bool,
    summary: bool,
    locked: bool,
}

/// Rows overlapping the viewport. `tops` holds each row's content y (one more entry than
/// rows); rows are `ROW_H` apart unless a lane is expanded.
fn viewport_rows<'a>(
    model: &'a DopeModel,
    tops: &'a [f64],
    axis: Axis,
    height: f64,
    offset: f64,
) -> impl Iterator<Item = KeyRow<'a>> + Clone + 'a {
    let n = model.rows().len().min(tops.len().saturating_sub(1));
    let start = tops
        .partition_point(|t| *t <= offset)
        .saturating_sub(1)
        .min(n);
    let end = tops
        .partition_point(|t| *t < offset + height)
        .min(n)
        .max(start);
    model.rows()[start..end]
        .iter()
        .enumerate()
        .filter(|(_, row)| model.channels[row.channel].visible)
        .map(move |(row_index, row)| {
            let channel = row.channel;
            let c = &model.channels[channel];
            let times = if row.summary {
                model.summary_keys(channel)
            } else {
                c.keys()
            };
            let first = times.partition_point(|t| *t < axis.t(-KEY_GRAB));
            let end = times.partition_point(|t| *t <= axis.t(axis.width + KEY_GRAB));
            let has_selection = model
                .selected
                .range((channel, 0)..=(channel, u64::MAX))
                .next()
                .is_some();
            // Check spacing where the keys actually fall. A tight cluster can
            // cover a few pixels even when most of the viewport is empty.
            let compact = !row.summary && c.decor.is_none() && !has_selection;
            let locked = !model.editable(channel);
            let y = tops[start + row_index] - offset;
            KeyRow {
                model,
                channel,
                axis,
                times: &times[first..end],
                first,
                summary: row.summary,
                compact,
                locked,
                muted: c.muted,
                y,
            }
        })
}

#[derive(Clone, Copy)]
struct KeyRow<'a> {
    model: &'a DopeModel,
    channel: usize,
    axis: Axis,
    times: &'a [f64],
    first: usize,
    summary: bool,
    compact: bool,
    locked: bool,
    muted: bool,
    y: f64,
}

impl<'a> KeyRow<'a> {
    fn indices(self) -> impl Iterator<Item = usize> + Clone + 'a {
        key_indices(
            self.times,
            0,
            self.times.len(),
            self.compact.then_some(4. * self.axis.scale),
        )
    }

    fn marks(self) -> impl Iterator<Item = KeyMark> + Clone + 'a {
        self.indices().map(move |index| {
            let time = self.times[index];
            let selected = if self.compact {
                false
            } else if self.summary {
                self.model.summary_selected(self.channel, time)
            } else {
                self.model
                    .selected
                    .contains(&(self.channel, time.to_bits()))
            };
            let (shape, hold) = if self.compact {
                (KeyShape::Diamond, None)
            } else {
                self.model
                    .key_display(self.channel, self.summary, self.first + index)
            };
            KeyMark {
                x: self.axis.x(time),
                y: self.y,
                selected,
                shape,
                hold: hold.map(|t| self.axis.x(t)),
                muted: self.muted,
                summary: self.summary,
                locked: self.locked,
            }
        })
    }
}

#[cfg(test)]
fn viewport_keys<'a>(
    model: &'a DopeModel,
    tops: &'a [f64],
    axis: Axis,
    height: f64,
    offset: f64,
) -> impl Iterator<Item = KeyMark> + 'a {
    viewport_rows(model, tops, axis, height, offset).flat_map(KeyRow::marks)
}

#[cfg(test)]
fn uniform_tops(rows: usize) -> Vec<f64> {
    (0..=rows).map(|i| i as f64 * f64::from(ROW_H)).collect()
}

impl Render for DopeSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity();
        gpui_kit::container_query(move |size, window, cx| {
            editor.update(cx, |editor, cx| editor.render_at_size(size, window, cx))
        })
    }
}

impl DopeSheet {
    fn render_at_size(
        &mut self,
        size: gpui_kit::Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        // A host write since the last frame: bring the cache up to date (allocates only for
        // the channels that changed). A gesture keeps its preview until it commits.
        if self.drag.is_none() && self.stale(cx) {
            self.sync(cx);
        }
        let theme = GuiraffeTheme::global(cx).clone();
        let s = theme.surfaces();
        // The outer layout fixes the rail and ruler dimensions. Use the actual
        // allocation on the first frame, including inside a split or tab.
        let w = f64::from((f32::from(size.width) - NAME_W).max(0.));
        let h = f64::from((f32::from(size.height) - RULER_H).max(1.));
        if w > 0. {
            self.surface.axis.set_width(w);
            if let Some((a, b)) = self.pending_fit.take() {
                self.surface.fit(a, b, 0.03);
            }
        }
        let total = self.derived.tops.last().copied().unwrap_or(0.);
        self.offset = self.offset.clamp(0., (total - h).max(0.));
        let tops = &self.derived.tops;
        let start = tops
            .partition_point(|t| *t <= self.offset)
            .saturating_sub(1);
        let end = tops.partition_point(|t| *t < self.offset + h);
        let first_top = tops.get(start).copied().unwrap_or(0.);
        let rows = self
            .model
            .rows()
            .get(start..end.min(self.model.rows().len()))
            .unwrap_or(&[]);
        let owner = cx.weak_entity();
        // Retain names and control state for channels still visible after scrolling.
        // Header equality below also handles edits and changes in the row tree.
        let mut headers_changed =
            self.header_offset != self.offset || rows.len() != self.headers.len();
        self.header_offset = self.offset;
        let shift = start
            .abs_diff(self.first_header_row)
            .min(self.headers.len());
        if start > self.first_header_row {
            self.headers.rotate_left(shift);
        } else {
            self.headers.rotate_right(shift);
        }
        self.first_header_row = start;
        let doc = self.doc.get(cx);
        for (index, row) in rows.iter().enumerate() {
            let c = &self.model.channels[row.channel];
            let source = doc.channels().get(row.channel);
            let y = (self.derived.tops[start + index] - first_top) as f32;
            let current = self.headers.get_mut(index);
            let name = current
                .as_ref()
                .map(|old| &old.name)
                .filter(|name| name.as_str() == c.name.as_str())
                .cloned()
                .unwrap_or_else(|| c.name.clone().into());
            let header = ChannelHeader {
                channel: row.channel,
                depth: row.depth,
                name,
                color: c.color,
                group: c.group,
                collapsed: c.collapsed,
                muted: c.muted,
                locked: c.locked,
                visible: c.visible,
                value: source.is_some_and(|c| c.kind == ChannelKind::Value),
                expanded: source.is_some_and(|c| c.expanded),
                y,
                owner: owner.clone(),
            };
            if let Some(entry) = current {
                if *entry != header {
                    headers_changed = true;
                    *entry = header;
                }
            } else {
                self.headers.push(header);
            }
        }
        self.headers.truncate(rows.len());
        let top = (first_top - self.offset) as f32;
        let header_rows = || self.headers.iter().cloned();
        let column = match &self.header_column {
            Some(column) => {
                if headers_changed {
                    column.update(cx, |column, cx| {
                        column.rows.clear();
                        column.rows.extend(header_rows());
                        column.top = top;
                        cx.notify();
                    });
                }
                column.clone()
            }
            None => {
                let column = cx.new(|_| HeaderColumn {
                    rows: header_rows().collect(),
                    top,
                });
                self.header_column = Some(column.clone());
                column
            }
        };
        let mut background = self.surface.clone();
        background.insert_layer(
            0,
            Zebra {
                rows: self.model.rows().len(),
            },
        );
        let mut foreground = PlotSurface::new(self.surface.axis);
        foreground.add_layer(Playhead {
            time: self.doc.get(cx).playhead,
            format: self.format,
        });
        if let Some(Gesture::Box { start, end, .. }) = &self.drag {
            let (start, end) = (*start, *end);
            foreground.add_layer(SelectionBox {
                start: (self.surface.axis.x(start.0), start.1 - self.offset),
                end: (end.0, f64::from(end.1)),
            });
        }
        let lanes = self.lane_elements(start, end, window, cx);
        let readout = self.readout_element(cx);
        let key_owner = cx.weak_entity();
        let body = self.body.clone();
        let key_selected = theme
            .state(StateKind::Selected, Variant::Filled, None)
            .background
            .to_hsla();
        let key_muted = s.dimmed.to_hsla();
        let key_normal = theme
            .variant(Some(ColorName::Yellow), Variant::Filled)
            .background
            .to_hsla();
        let lock_style = self.lock_style;
        let tab = guiraffe_core::tab_stop(self.focusable);
        let focus_visible = guiraffe_core::ring_visible(&self.focus, window);
        let ring = theme
            .state(StateKind::FocusRing, Variant::Default, None)
            .border;
        let root = div()
            .id("dope-sheet")
            .test_support()
            .role(Role::Tree)
            .aria_label("Dope sheet")
            .track_focus(&self.focus.clone().tab_stop(tab))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(12.))
            .when(tab, |el| {
                let view = owner.clone();
                el.key_context("TimelineDope").on_key_down(
                    move |e: &gpui_kit::KeyDownEvent, window: &mut Window, cx: &mut App| {
                        let Some(sheet) = view.upgrade() else { return };
                        let actions = sheet.update(cx, |this, cx| this.key_actions(e, cx));
                        match actions.as_slice() {
                            [] => {}
                            [one] => route(&view, one.clone(), Source::Keyboard, false, window, cx),
                            many => route_many(&view, many, Source::Keyboard, window, cx),
                        }
                    },
                )
            })
            .child(
                div()
                    .h(px(RULER_H))
                    .flex_none()
                    .flex()
                    .child(div().w(px(NAME_W)).flex_none().px(px(8.)).child("Channels"))
                    .child(
                        div()
                            .id("dope-ruler")
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.surface.element(
                                theme.clone(),
                                true,
                                f64::from(ROW_H),
                                self.offset,
                            ))
                            .child(foreground.element(
                                theme.clone(),
                                true,
                                f64::from(ROW_H),
                                self.offset,
                            ))
                            .on_mouse_down(MouseButton::Left, {
                                let view = owner.clone();
                                move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                    let Some(sheet) = view.upgrade() else { return };
                                    let t = {
                                        let this = sheet.read(cx);
                                        let (x, _) = this.local(e.position);
                                        snap_time(this.surface.axis.t(x), this.format).max(0.)
                                    };
                                    route(
                                        &view,
                                        DopeAction::Seek(t),
                                        Source::Pointer,
                                        false,
                                        window,
                                        cx,
                                    );
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        column.cached(
                            gpui_kit::StyleRefinement::default()
                                .w(px(NAME_W))
                                .flex_none()
                                .h_full(),
                        ),
                    )
                    .child(
                        div()
                            .id("dope-body")
                            .test_support()
                            .debug_selector(|| "dope-body".into())
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .cursor(gpui_kit::CursorStyle::Crosshair)
                            // The plot owns its drag feedback. Suppress GPUI's generic
                            // pressed state before bubbling would refresh the whole window.
                            .capture_any_mouse_down(|event, window, _| {
                                if event.button == MouseButton::Left {
                                    window.prevent_default();
                                }
                            })
                            .on_scroll(cx.listener(|this, e: &gpui_kit::ScrollEvent, _, cx| {
                                let d = e.delta.pixel_delta(px(16.));
                                let (x, _) = this.local(e.position);
                                let changed = if e.modifiers.control
                                    || e.modifiers.platform
                                    || e.modifiers.shift
                                {
                                    this.surface.navigate(
                                        x,
                                        f64::from(f32::from(d.x)),
                                        f64::from(f32::from(d.y)),
                                        e.modifiers.control || e.modifiers.platform,
                                    )
                                } else {
                                    let height =
                                        f64::from(f32::from(this.body.get().size.height)).max(1.);
                                    let max = (this.model.rows().len() as f64 * f64::from(ROW_H)
                                        - height)
                                        .max(0.);
                                    let offset =
                                        (this.offset - f64::from(f32::from(d.y))).clamp(0., max);
                                    let changed = offset != this.offset;
                                    this.offset = offset;
                                    changed
                                };
                                if changed {
                                    cx.notify();
                                }
                            }))
                            .on_mouse_down(MouseButton::Left, {
                                let view = owner.clone();
                                move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                    Self::mouse_down(&view, e, window, cx)
                                }
                            })
                            .on_mouse_down(MouseButton::Right, {
                                let view = owner.clone();
                                move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                    let Some(sheet) = view.upgrade() else { return };
                                    let action = sheet.read(cx).segment_interp_action(e, cx);
                                    if let Some(action) = action {
                                        route(&view, action, Source::Pointer, false, window, cx);
                                    }
                                }
                            })
                            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    this.drag_move(e, cx);
                                }
                            }))
                            .on_mouse_up(MouseButton::Left, {
                                let view = owner.clone();
                                move |_: &gpui_kit::MouseUpEvent,
                                      window: &mut Window,
                                      cx: &mut App| {
                                    let Some(sheet) = view.upgrade() else { return };
                                    let commit = sheet.update(cx, |this, cx| {
                                        let commit = this.take_commit(cx);
                                        this.resync(cx);
                                        cx.notify();
                                        commit
                                    });
                                    if let Some(action) = commit {
                                        route(&view, action, Source::Pointer, true, window, cx);
                                    }
                                }
                            })
                            .child(
                                canvas(move |b, _, _| body.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                            .child(background.element(
                                theme.clone(),
                                false,
                                f64::from(ROW_H),
                                self.offset,
                            ))
                            .child(segment_canvas(
                                owner.clone(),
                                key_normal,
                                key_muted,
                                focus_visible,
                            ))
                            .child(key_canvas(
                                key_owner,
                                key_selected,
                                key_muted,
                                key_normal,
                                lock_style,
                            ))
                            .children(lanes)
                            .children(readout)
                            .child(foreground.element(theme, false, f64::from(ROW_H), self.offset)),
                    ),
            );
        guiraffe_core::focus_ring(root, focus_visible, px(0.), ring).into_any_element()
    }
}
/// Value segments (a mini-curve, or a bar for Constant), toggle on-bars and the keyboard row
/// cursor, under the keys. Quads only: nothing allocates per frame.
fn segment_canvas(
    owner: gpui_kit::WeakEntity<DopeSheet>,
    ink: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
    focused: bool,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, _, window, cx| {
            let Some(owner) = owner.upgrade() else { return };
            let sheet = owner.read(cx);
            let doc = sheet.doc.get(cx);
            let axis = sheet.surface.axis;
            let tops = &sheet.derived.tops;
            let height = f64::from(f32::from(b.size.height));
            let n = sheet.model.rows().len().min(tops.len().saturating_sub(1));
            let start = tops
                .partition_point(|t| *t <= sheet.offset)
                .saturating_sub(1)
                .min(n);
            let end = tops
                .partition_point(|t| *t < sheet.offset + height)
                .min(n)
                .max(start);
            let quad = |x: f64, y: f64, w: f64, h: f64, color, window: &mut Window| {
                window.paint_quad(gpui_kit::fill(
                    Bounds::new(
                        gpui_kit::point(b.origin.x + px(x as f32), b.origin.y + px(y as f32)),
                        gpui_kit::size(px(w.max(0.) as f32), px(h.max(0.) as f32)),
                    ),
                    color,
                ));
            };
            window.paint_layer(b, |window| {
                for row in start..end {
                    let index = sheet.model.rows()[row].channel;
                    let Some(c) = doc.channels().get(index) else {
                        continue;
                    };
                    let top = tops[row] - sheet.offset;
                    if focused && row == sheet.cursor_row {
                        let mut hl = ink;
                        hl.a = 0.08;
                        quad(0., top, axis.width, f64::from(ROW_H), hl, window);
                    }
                    if !c.visible || c.group || !sheet.segments {
                        continue;
                    }
                    let color = if c.muted { muted } else { ink };
                    let keys = c.curve.keys();
                    match c.kind {
                        ChannelKind::Keys => {}
                        ChannelKind::Toggle => {
                            let mut bar = color;
                            bar.a = 0.45;
                            for (i, k) in keys.iter().enumerate() {
                                if k.key.v < 0.5 {
                                    continue;
                                }
                                let x0 = axis.x(k.key.t).max(0.);
                                let x1 = keys
                                    .get(i + 1)
                                    .map_or(axis.width, |n| axis.x(n.key.t))
                                    .min(axis.width);
                                if x1 > x0 {
                                    quad(
                                        x0,
                                        top + 7.,
                                        x1 - x0,
                                        f64::from(ROW_H) - 14.,
                                        bar,
                                        window,
                                    );
                                }
                            }
                        }
                        ChannelKind::Value => {
                            let (inset, span) = (4., f64::from(ROW_H) - 8.);
                            let y_of = |v: f64| top + inset + (1. - c.curve.bounds.unit(v)) * span;
                            let first = keys
                                .partition_point(|k| axis.x(k.key.t) < 0.)
                                .saturating_sub(1);
                            for i in first..keys.len().saturating_sub(1) {
                                let (a, z) = (&keys[i].key, &keys[i + 1].key);
                                let (x0, x1) = (axis.x(a.t), axis.x(z.t));
                                if x0 > axis.width {
                                    break;
                                }
                                if a.interp == crate::Interp::Constant {
                                    let mut fill = color;
                                    fill.a = 0.3;
                                    let y = y_of(a.v);
                                    let (l, r) = (x0.max(0.), x1.min(axis.width));
                                    // A hold: a solid line at the value over a light fill,
                                    // so a hold at the bottom of the range still shows.
                                    quad(l, y, r - l, top + inset + span - y, fill, window);
                                    quad(l, y - 1.5, r - l, 3., color, window);
                                    continue;
                                }
                                // A sampled mini-curve: one short column per 2 px, joined.
                                let (l, r) = (x0.max(0.), x1.min(axis.width));
                                let mut prev = y_of(crate::curves::segment_value(a, z, axis.t(l)));
                                let mut x = l;
                                while x < r {
                                    let next_x = (x + 2.).min(r);
                                    let y =
                                        y_of(crate::curves::segment_value(a, z, axis.t(next_x)));
                                    let (lo, hi) = (prev.min(y), prev.max(y));
                                    quad(
                                        x,
                                        lo - 0.75,
                                        next_x - x + 0.5,
                                        hi - lo + 1.5,
                                        color,
                                        window,
                                    );
                                    prev = y;
                                    x = next_x;
                                }
                            }
                        }
                    }
                }
            });
        },
    )
    .absolute()
    .size_full()
}

fn key_canvas(
    key_owner: gpui_kit::WeakEntity<DopeSheet>,
    key_selected: gpui_kit::Hsla,
    key_muted: gpui_kit::Hsla,
    key_normal: gpui_kit::Hsla,
    lock_style: LockStyle,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, _, window, cx| {
            let Some(owner) = key_owner.upgrade() else {
                return;
            };
            let sheet = owner.read(cx);
            let height = f64::from(f32::from(b.size.height)).max(1.);
            let rows = viewport_rows(
                &sheet.model,
                &sheet.derived.tops,
                sheet.surface.axis,
                height,
                sheet.offset,
            );
            // Sparse scenes keep their original rasterization. Count
            // only up to the crossover before traversing for paint.
            let batch_diamonds = rows.clone().flat_map(KeyRow::marks).nth(32).is_some();
            let at = |x: f64, y: f64| {
                gpui_kit::point(b.origin.x + px(x as f32), b.origin.y + px(y as f32))
            };
            let mut diamonds: Option<(gpui_kit::Hsla, gpui_kit::Path<Pixels>)> = None;
            let paint_mark = |mark: KeyMark,
                              diamonds: &mut Option<(gpui_kit::Hsla, gpui_kit::Path<Pixels>)>,
                              window: &mut Window| {
                let KeyMark {
                    x,
                    y,
                    selected,
                    shape,
                    hold,
                    muted,
                    summary,
                    locked,
                    ..
                } = mark;
                let ink = if selected {
                    key_selected
                } else if muted || (locked && lock_style == LockStyle::Dimmed) {
                    key_muted
                } else {
                    key_normal
                };
                let cy = y + f64::from(ROW_H) / 2.;
                let radius = if selected || summary { 6. } else { 4. };
                if batch_diamonds
                    && shape == KeyShape::Diamond
                    && hold.is_none()
                    && !(locked && lock_style == LockStyle::Hatched)
                {
                    if diamonds.as_ref().is_some_and(|(color, _)| *color != ink)
                        && let Some((color, mesh)) = diamonds.take()
                    {
                        window.paint_path(mesh, color);
                    }
                    let (_, mesh) =
                        diamonds.get_or_insert_with(|| (ink, gpui_kit::Path::new(at(x, cy))));
                    append_diamond(
                        mesh,
                        [
                            at(x, cy - radius),
                            at(x + radius, cy),
                            at(x, cy + radius),
                            at(x - radius, cy),
                        ],
                    );
                    return;
                }
                if let Some((color, mesh)) = diamonds.take() {
                    window.paint_path(mesh, color);
                }
                if let Some(end) = hold {
                    let mut path = gpui_kit::PathBuilder::stroke(px(3.));
                    path.move_to(at(x, cy));
                    path.line_to(at(end, cy));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, ink);
                    }
                }
                let mut path = gpui_kit::PathBuilder::fill();
                if shape == KeyShape::Diamond {
                    for (i, (dx, dy)) in [(0., -radius), (radius, 0.), (0., radius), (-radius, 0.)]
                        .iter()
                        .enumerate()
                    {
                        if i == 0 {
                            path.move_to(at(x + dx, cy + dy));
                        } else {
                            path.line_to(at(x + dx, cy + dy));
                        }
                    }
                } else {
                    let sides = if shape == KeyShape::Circle { 16 } else { 4 };
                    for i in 0..sides {
                        let angle = (i as f64 + 0.5) * std::f64::consts::TAU / sides as f64;
                        let p = at(x + radius * angle.cos(), cy + radius * angle.sin());
                        if i == 0 {
                            path.move_to(p)
                        } else {
                            path.line_to(p)
                        }
                    }
                }
                path.close();
                if locked && lock_style == LockStyle::Hatched {
                    path.move_to(at(x - radius, cy + radius));
                    path.line_to(at(x + radius, cy - radius));
                }

                if let Ok(path) = path.build() {
                    window.paint_path(path, ink);
                }
            };
            let strip = sheet.strip.clone();
            let scale = window.scale_factor();
            let left = f32::from(b.left());
            let origin = (left * scale).floor();
            let mut centers = Vec::new();
            let mut previous = None;
            let mut can_replace = true;
            for row in rows {
                if row.compact
                    && row.times.len() >= 64
                    && !(row.locked && lock_style == LockStyle::Hatched)
                {
                    // Bound comparisons by the viewport, even for a lane
                    // with millions of keys at subpixel spacing.
                    let same = row.times.len() <= 4096 && previous == Some(row.times);
                    if !same {
                        previous = Some(row.times);
                        centers.clear();
                        centers
                            .extend(row.indices().map(|i| {
                                (left + row.axis.x(row.times[i]) as f32) * scale - origin
                            }));
                    }
                    if centers.len() >= 64 {
                        let ink = if row.muted || (row.locked && lock_style == LockStyle::Dimmed) {
                            key_muted
                        } else {
                            key_normal
                        };
                        let cy = f32::from(b.top()) + row.y as f32 + ROW_H / 2.;
                        if let Some((color, mesh)) = diamonds.take() {
                            window.paint_path(mesh, color);
                        }
                        if strip
                            .borrow_mut()
                            .paint(&centers, b, cy, ink, can_replace, window)
                        {
                            can_replace = false;
                            continue;
                        }
                    }
                }
                for mark in row.marks() {
                    paint_mark(mark, &mut diamonds, window);
                }
            }
            if let Some((color, mesh)) = diamonds {
                window.paint_path(mesh, color);
            }
            if let Some(old) = strip.borrow_mut().retired.take() {
                cx.drop_image(old, Some(window));
            }
        },
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_batch_matches_generic_triangle_vertices_and_bounds() {
        use gpui_kit::{Path, point};

        let origin = point(px(2.), px(7.));
        let mut batch = Path::new(origin);
        let mut reference = Path::new(origin);
        let uv = point(0., 1.);
        for x in [-4., 20., 13.] {
            let top = point(px(x), px(4.));
            let right = point(px(x + 3.), px(7.));
            let bottom = point(px(x), px(10.));
            let left = point(px(x - 3.), px(7.));
            append_diamond(&mut batch, [top, right, bottom, left]);
            reference.push_triangle((top, right, bottom), (uv, uv, uv));
            reference.push_triangle((top, bottom, left), (uv, uv, uv));
        }
        assert_eq!(batch.bounds, reference.bounds);
        assert_eq!(batch.vertices.len(), reference.vertices.len());
        for (actual, expected) in batch.vertices.iter().zip(&reference.vertices) {
            assert_eq!(actual.xy_position, expected.xy_position);
            assert_eq!(actual.st_position, expected.st_position);
            assert_eq!(actual.content_mask, expected.content_mask);
        }
    }

    #[test]
    fn viewport_visits_only_intersecting_rows_at_both_scales() {
        for count in [10_000, 100_000] {
            let model =
                DopeModel::new((0..count).map(|_| Channel::new("lane", vec![1.])).collect());
            let axis = Axis::new(0., 10., 100.);
            let row_height = f64::from(ROW_H);
            let offset = 25.5 * row_height;
            let tops = uniform_tops(count);
            let rows: Vec<_> = viewport_keys(&model, &tops, axis, 2. * row_height, offset)
                .map(|mark| mark.y)
                .collect();
            assert_eq!(
                rows,
                [-0.5 * row_height, 0.5 * row_height, 1.5 * row_height]
            );
        }
    }

    #[test]
    fn a_cluster_in_an_otherwise_sparse_lane_retains_isolated_keys() {
        let model = DopeModel::new(vec![Channel::new("lane", vec![1., 2., 3., 5., 80.])]);
        let tops = uniform_tops(1);
        let marks: Vec<_> = viewport_keys(
            &model,
            &tops,
            Axis::new(0., 100., 100.),
            f64::from(ROW_H),
            0.,
        )
        .map(|mark| mark.x)
        .collect();
        assert_eq!(marks, [1., 5., 80.]);
    }

    #[test]
    fn dense_plain_keys_emit_the_same_marks_across_input_scales() {
        for count in [10_000, 100_000, 10_000_000] {
            let times: Vec<_> = (0..count).map(|i| i as f64 * 100. / count as f64).collect();
            let marks: Vec<_> = key_indices(&times, 0, count, Some(4.))
                .map(|i| times[i])
                .collect();
            assert_eq!(
                marks,
                (0..25).map(|i| f64::from(i) * 4.).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn channels_sort_dedup_and_refuse_collisions() {
        let mut c = Channel::new("x", vec![2.0, 1.0, 2.0, f64::NAN]);
        assert_eq!(c.keys(), &[1.0, 2.0]);
        assert_eq!(c.retime(0, 2.0), None, "collision");
        assert_eq!(c.retime(0, 3.0), Some(1), "reorders");
        assert_eq!(c.keys(), &[2.0, 3.0]);
        assert_eq!(c.retime(9, 1.0), None);
    }

    #[test]
    fn frames_snap() {
        assert_eq!(snap_time(0.51, TimeFormat::Frames(10.0)), 0.5);
        assert_eq!(snap_time(0.51, TimeFormat::Seconds), 0.51);
    }

    #[test]
    fn dragging_retimes_with_frame_snap_and_follows_reorder() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            DopeSheet::new(
                vec![Channel::new("pos", vec![0.5, 1.0])],
                TimeFormat::Frames(10.0),
                2.0,
                cx,
            )
        });
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &DopeEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            assert!(v.press(0, a.x(0.5), cx));
            v.drag_to(a.x(1.42), cx); // -> 1.4, now index 1
            v.drag_to(a.x(1.61), cx); // same key keeps moving -> 1.6
            v.release(cx); // one retime action
        });
        assert_eq!(
            view.read_with(&cx, |v, _| v.channels()[0].keys().to_vec()),
            vec![1.0, 1.6]
        );
        assert_eq!(events.borrow().len(), 2);
    }
}

#[cfg(test)]
mod hierarchy_regressions {
    use super::*;
    #[test]
    fn summary_drag_selects_real_children_and_duplicate_keeps_metadata() {
        let mut model = DopeModel::new(vec![Channel::group(
            "Object",
            vec![
                Channel::with_keyframes(
                    "x",
                    vec![crate::Key::new(0.5, 1., crate::Interp::Constant)],
                ),
                Channel::new("locked", vec![0.5]),
            ],
        )]);
        model.channels[2].locked = true;
        model.select_summary(0, 0.5, false);
        assert_eq!(model.selected.len(), 2);
        assert!(model.duplicate(0.1, TimeFormat::Frames(10.)));
        assert_eq!(model.channels[1].keys(), &[0.5, 0.6]);
        assert_eq!(model.channels[2].keys(), &[0.5]);
        assert_eq!(
            model.channels[1].decor,
            Some(vec![(Some(1.), KeyShape::Square); 2])
        );
    }
    #[test]
    fn adjacent_selected_keys_move_atomically() {
        let mut model = DopeModel::new(vec![Channel::new("x", vec![0., 1., 2.])]);
        model.select_all();
        assert!(model.translate(1., TimeFormat::Seconds));
        assert_eq!(model.channels[0].keys(), &[1., 2., 3.]);
        assert!(model.channels[0].decor.is_none());
    }
}

pub(crate) mod document;
