//! The authoring model behind `DopeSheet`: channels and keys keyed by stable host ids, value
//! and toggle channels with interpolated segments, and the edits a user can make. It lives in
//! a [`Store`] (see [`dope_store`]), the same pattern as `tracks_store`:
//!
//! - **Host writes** are store events ([`SetChannels`], [`SetPlayhead`], [`SetDopeSelection`]).
//!   They never reach the bus or emit a `DopeEvent`, so nothing echoes.
//! - **User edits** are [`DopeAction`]s applied through the control protocol, from the
//!   pointer, the keyboard or any bus client alike. Each applied action returns one
//!   [`DopeChange`].
//! - **Undo** covers channel content (keys, values, interpolation); selection, playhead and
//!   view flags are not undoable.
//!
//! Pure apart from `SharedString`: [`ChannelCurve::value_at`] is safe off the UI thread.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use gpui_kit::{App, SharedString};
use guiraffe_core::{
    ColorName,
    protocol::{Action, ActionError},
    widget_value::Value,
};
use guiraffe_state::{Event, Fx, HistoryLimits, Store};

use crate::curves::{Interp, Key, eval_keys};

/// A channel's stable key, provided by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelId(pub u64);

/// A key's stable id, unique within its channel. Hosts provide them; keys a user inserts get
/// fresh ids from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId(pub u64);

impl From<u64> for ChannelId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<u64> for KeyId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// A key by channel and key id (never positions or times).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyRef {
    pub channel: ChannelId,
    pub key: KeyId,
}

impl KeyRef {
    pub fn new(channel: impl Into<ChannelId>, key: impl Into<KeyId>) -> Self {
        Self {
            channel: channel.into(),
            key: key.into(),
        }
    }
}

/// What a channel's keys mean.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChannelKind {
    /// Times only: keys are markers.
    #[default]
    Keys,
    /// Keys carry values and segments interpolate between them.
    Value,
    /// Keys switch on (value 1) or off (value 0); segments hold.
    Toggle,
}

/// The lowest and highest value a value channel shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueBounds {
    pub lo: f64,
    pub hi: f64,
}

impl Default for ValueBounds {
    fn default() -> Self {
        Self { lo: 0.0, hi: 1.0 }
    }
}

impl ValueBounds {
    pub fn new(lo: f64, hi: f64) -> Self {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        if lo.is_finite() && hi.is_finite() && hi > lo {
            Self { lo, hi }
        } else {
            Self::default()
        }
    }

    /// `v` as a fraction of the range, clamped to `0..=1`.
    pub fn unit(&self, v: f64) -> f64 {
        ((v - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0)
    }

    pub fn clamp(&self, v: f64) -> f64 {
        v.clamp(self.lo, self.hi)
    }
}

/// How a value reads: plain data, shared with value mapping later. Views may override it
/// with a [`ValueFormatter`].
#[derive(Debug, Clone, PartialEq)]
pub struct NumberFormat {
    pub decimals: u8,
    /// Multiplied in before formatting (100 for a percentage of a 0..1 value).
    pub scale: f64,
    /// Appended after a space when not empty (`"dB"`, `"%"`, `"Hz"`).
    pub unit: SharedString,
}

impl Default for NumberFormat {
    fn default() -> Self {
        Self {
            decimals: 2,
            scale: 1.0,
            unit: SharedString::default(),
        }
    }
}

impl NumberFormat {
    pub fn new(decimals: u8, unit: impl Into<SharedString>) -> Self {
        Self {
            decimals,
            scale: 1.0,
            unit: unit.into(),
        }
    }

    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Write `value` into `out` (cleared first), so a caller can reuse one buffer.
    pub fn write(&self, value: f64, out: &mut String) {
        use std::fmt::Write;
        out.clear();
        let _ = write!(out, "{:.*}", usize::from(self.decimals), value * self.scale);
        if !self.unit.is_empty() {
            out.push(' ');
            out.push_str(&self.unit);
        }
    }
}

/// Formats a channel's value for the inline readout. Implement it to replace a channel's
/// [`NumberFormat`] (units the format can't express, note names, ...).
pub trait ValueFormatter: 'static {
    fn format(&self, channel: &DopeChannel, value: f64, out: &mut String);
}

impl ValueFormatter for NumberFormat {
    fn format(&self, _: &DopeChannel, value: f64, out: &mut String) {
        self.write(value, out);
    }
}

/// One key with its stable id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DopeKey {
    pub id: KeyId,
    pub key: Key,
}

impl DopeKey {
    pub fn new(id: impl Into<KeyId>, t: f64, v: f64, interp: Interp) -> Self {
        Self {
            id: id.into(),
            key: Key::new(t, v, interp),
        }
    }
}

/// A channel's keys, sorted by time with unique times. Segment `i` runs from key `i` to key
/// `i + 1` and interpolates by key `i`'s mode.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChannelCurve {
    keys: Vec<DopeKey>,
    pub bounds: ValueBounds,
    /// Interpolation of newly inserted keys.
    pub default_interp: Interp,
}

impl ChannelCurve {
    /// Keys are sorted; a key whose time or id repeats an earlier one, or that isn't finite,
    /// is dropped.
    pub fn new(mut keys: Vec<DopeKey>, bounds: ValueBounds, default_interp: Interp) -> Self {
        keys.retain(|k| k.key.t.is_finite() && k.key.v.is_finite());
        keys.sort_by(|a, b| a.key.t.total_cmp(&b.key.t));
        let mut ids = BTreeSet::new();
        keys.dedup_by(|b, a| a.key.t == b.key.t);
        keys.retain(|k| ids.insert(k.id));
        Self {
            keys,
            bounds,
            default_interp,
        }
    }

    pub fn keys(&self) -> &[DopeKey] {
        &self.keys
    }

    /// Value at time `t`, by the interpolation the dope sheet and the curve view draw.
    /// Pure, so a host can sample it off the UI thread (audio automation).
    pub fn value_at(&self, t: f64) -> f64 {
        eval_keys(&self.keys, |k| &k.key, t)
    }

    /// Whether a toggle channel is on at `t` (off before the first key).
    pub fn is_on(&self, t: f64) -> bool {
        let i = self.keys.partition_point(|k| k.key.t <= t);
        i > 0 && self.keys[i - 1].key.v >= 0.5
    }

    pub fn index_of(&self, id: KeyId) -> Option<usize> {
        self.keys.iter().position(|k| k.id == id)
    }

    pub fn key(&self, id: KeyId) -> Option<&DopeKey> {
        self.keys.iter().find(|k| k.id == id)
    }

    /// The key at exactly time `t`.
    pub fn at_time(&self, t: f64) -> Option<&DopeKey> {
        let i = self.keys.partition_point(|k| k.key.t < t);
        self.keys.get(i).filter(|k| k.key.t == t)
    }

    fn insert(&mut self, key: DopeKey) -> bool {
        if !(key.key.t.is_finite() && key.key.v.is_finite()) || self.index_of(key.id).is_some() {
            return false;
        }
        let i = self.keys.partition_point(|k| k.key.t < key.key.t);
        if self.keys.get(i).is_some_and(|k| k.key.t == key.key.t) {
            return false;
        }
        self.keys.insert(i, key);
        true
    }

    fn remove(&mut self, id: KeyId) -> Option<DopeKey> {
        let i = self.index_of(id)?;
        Some(self.keys.remove(i))
    }

    fn key_mut(&mut self, id: KeyId) -> Option<&mut DopeKey> {
        self.keys.iter_mut().find(|k| k.id == id)
    }

    fn resort(&mut self) {
        self.keys.sort_by(|a, b| a.key.t.total_cmp(&b.key.t));
    }

    fn max_id(&self) -> u64 {
        self.keys.iter().map(|k| k.id.0).max().unwrap_or(0)
    }
}

/// One row of the dope sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct DopeChannel {
    pub id: ChannelId,
    pub name: SharedString,
    pub color: ColorName,
    pub kind: ChannelKind,
    pub curve: ChannelCurve,
    pub format: NumberFormat,
    /// The group this channel belongs to.
    pub parent: Option<ChannelId>,
    pub group: bool,
    pub collapsed: bool,
    pub muted: bool,
    pub locked: bool,
    pub visible: bool,
    /// Expanded into a curve-editor lane.
    pub expanded: bool,
}

impl DopeChannel {
    fn base(id: impl Into<ChannelId>, name: impl Into<SharedString>, kind: ChannelKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            color: ColorName::Blue,
            kind,
            curve: ChannelCurve::default(),
            format: NumberFormat::default(),
            parent: None,
            group: false,
            collapsed: false,
            muted: false,
            locked: false,
            visible: true,
            expanded: false,
        }
    }

    /// Time-only markers.
    pub fn keys(
        id: impl Into<ChannelId>,
        name: impl Into<SharedString>,
        keys: Vec<DopeKey>,
    ) -> Self {
        let mut c = Self::base(id, name, ChannelKind::Keys);
        c.curve = ChannelCurve::new(keys, ValueBounds::default(), Interp::Linear);
        c
    }

    /// A value channel over `bounds`.
    pub fn value(
        id: impl Into<ChannelId>,
        name: impl Into<SharedString>,
        keys: Vec<DopeKey>,
        bounds: ValueBounds,
    ) -> Self {
        let mut c = Self::base(id, name, ChannelKind::Value);
        c.curve = ChannelCurve::new(keys, bounds, Interp::Linear);
        c
    }

    /// An on/off channel: each key switches on (`true`) or off at its time.
    pub fn toggle(
        id: impl Into<ChannelId>,
        name: impl Into<SharedString>,
        switches: Vec<(KeyId, f64, bool)>,
    ) -> Self {
        let mut c = Self::base(id, name, ChannelKind::Toggle);
        let keys = switches
            .into_iter()
            .map(|(id, t, on)| DopeKey {
                id,
                key: Key::new(t, if on { 1.0 } else { 0.0 }, Interp::Constant),
            })
            .collect();
        c.curve = ChannelCurve::new(keys, ValueBounds::default(), Interp::Constant);
        c
    }

    /// A group row; children name it as their `parent`.
    pub fn group(id: impl Into<ChannelId>, name: impl Into<SharedString>) -> Self {
        let mut c = Self::base(id, name, ChannelKind::Keys);
        c.group = true;
        c
    }

    pub fn with_parent(mut self, parent: impl Into<ChannelId>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn with_color(mut self, color: ColorName) -> Self {
        self.color = color;
        self
    }

    pub fn with_format(mut self, format: NumberFormat) -> Self {
        self.format = format;
        self
    }

    pub fn with_default_interp(mut self, interp: Interp) -> Self {
        self.curve.default_interp = interp;
        self
    }
}

/// A view flag a user toggles on a channel row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelFlag {
    Collapsed,
    Muted,
    Locked,
    Visible,
    Expanded,
}

/// The model a `DopeSheet` renders. Cheap to clone: channels are shared.
#[derive(Debug, Clone)]
pub struct DopeDoc {
    channels: Arc<Vec<Arc<DopeChannel>>>,
    pub selection: Arc<BTreeSet<KeyRef>>,
    pub playhead: f64,
    /// The highest key id in use, so inserted keys get fresh ids.
    next_key: u64,
}

impl PartialEq for DopeDoc {
    fn eq(&self, other: &Self) -> bool {
        let channels = Arc::ptr_eq(&self.channels, &other.channels)
            || (self.channels.len() == other.channels.len()
                && self
                    .channels
                    .iter()
                    .zip(other.channels.iter())
                    .all(|(a, b)| Arc::ptr_eq(a, b) || a == b));
        channels
            && (Arc::ptr_eq(&self.selection, &other.selection) || self.selection == other.selection)
            && self.playhead == other.playhead
            && self.next_key == other.next_key
    }
}

impl Default for DopeDoc {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

/// What a [`DopeAction`] changed: the outcome bus subscribers and `DopeEvent` listeners see.
#[derive(Debug, Clone, PartialEq)]
pub enum DopeChange {
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
        interp: Interp,
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
    /// A curve lane replaced a channel's keys.
    CurveEdited {
        channel: ChannelId,
    },
    SelectionChanged,
    ChannelChanged {
        channel: ChannelId,
        flag: ChannelFlag,
        on: bool,
    },
    Seek {
        time: f64,
    },
}

/// A user edit: what pointer, keyboard and bus clients send. Times are already snapped by
/// the view; the model only checks them.
#[derive(Debug, Clone, PartialEq)]
pub enum DopeAction {
    /// Insert a key at `t`. `value: None` keeps the curve's current value there.
    InsertKey {
        channel: ChannelId,
        t: f64,
        value: Option<f64>,
    },
    RemoveKeys(Vec<KeyRef>),
    /// Move keys by `delta`; refused if any lands on another key.
    RetimeKeys {
        keys: Vec<KeyRef>,
        delta: f64,
    },
    /// Copy keys `delta` later; the copies become the selection.
    DuplicateKeys {
        keys: Vec<KeyRef>,
        delta: f64,
    },
    /// Set a key's value (clamped to the channel's bounds).
    SetKeyValue {
        key: KeyRef,
        value: f64,
    },
    /// Set the interpolation of the segment that starts at `key`.
    SetSegmentInterp {
        key: KeyRef,
        interp: Interp,
    },
    /// Bend the segment that starts at `key` (`-1..=1`; 0 is straight). Makes it Bezier.
    BendSegment {
        key: KeyRef,
        amount: f64,
    },
    /// Flip a toggle channel from `t` to its next switch, splitting the span at `t`.
    ToggleSpan {
        channel: ChannelId,
        t: f64,
    },
    /// Replace a value channel's keys (what an embedded curve editor commits). Keys keep
    /// the id of the key they replace (same time, else same position); new keys get fresh
    /// ids.
    SetCurve {
        channel: ChannelId,
        keys: Vec<Key>,
    },
    /// Replace the selection (`add`: toggle these keys in it instead).
    Select {
        keys: Vec<KeyRef>,
        add: bool,
    },
    SetChannelFlag {
        channel: ChannelId,
        flag: ChannelFlag,
        on: bool,
    },
    Seek(f64),
}

impl DopeAction {
    /// Whether the store records this action in undo history.
    pub fn undoable(&self) -> bool {
        !matches!(
            self,
            Self::Select { .. } | Self::SetChannelFlag { .. } | Self::Seek(_)
        )
    }
}

impl DopeDoc {
    /// Channels in display order; a child follows its group.
    pub fn new(channels: Vec<DopeChannel>) -> Self {
        let mut doc = Self {
            channels: Arc::new(Vec::new()),
            selection: Arc::default(),
            playhead: 0.0,
            next_key: 0,
        };
        doc.set_channels(channels);
        doc
    }

    pub fn channels(&self) -> &[Arc<DopeChannel>] {
        &self.channels
    }

    pub fn index_of(&self, id: ChannelId) -> Option<usize> {
        self.channels.iter().position(|c| c.id == id)
    }

    pub fn channel(&self, id: ChannelId) -> Option<&DopeChannel> {
        self.channels.iter().find(|c| c.id == id).map(|c| &**c)
    }

    pub fn key(&self, at: KeyRef) -> Option<&DopeKey> {
        self.channel(at.channel)?.curve.key(at.key)
    }

    fn channel_mut(&mut self, id: ChannelId) -> Option<&mut DopeChannel> {
        let i = self.index_of(id)?;
        Arc::make_mut(&mut self.channels)
            .get_mut(i)
            .map(Arc::make_mut)
    }

    /// Whether `id` and every group above it are unlocked.
    pub fn editable(&self, id: ChannelId) -> bool {
        let mut at = Some(id);
        for _ in 0..=self.channels.len() {
            let Some(c) = at.and_then(|id| self.channel(id)) else {
                return at.is_none();
            };
            if c.locked {
                return false;
            }
            at = c.parent;
        }
        false
    }

    /// Replace the channels, diffing by [`ChannelId`]: an unchanged channel keeps its
    /// allocation, a known channel keeps its view flags (collapsed, expanded), and the
    /// selection keeps the keys that still exist. Returns whether anything changed.
    pub fn set_channels(&mut self, channels: Vec<DopeChannel>) -> bool {
        let old: HashMap<ChannelId, &Arc<DopeChannel>> =
            self.channels.iter().map(|c| (c.id, c)).collect();
        let mut seen = BTreeSet::new();
        let next: Vec<Arc<DopeChannel>> = channels
            .into_iter()
            .filter(|c| seen.insert(c.id))
            .map(|mut c| match old.get(&c.id) {
                Some(prev) => {
                    c.collapsed = prev.collapsed;
                    c.expanded = prev.expanded;
                    if ***prev == c {
                        Arc::clone(prev)
                    } else {
                        Arc::new(c)
                    }
                }
                None => Arc::new(c),
            })
            .collect();
        drop(old);
        let same = next.len() == self.channels.len()
            && next
                .iter()
                .zip(self.channels.iter())
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if same {
            return false;
        }
        self.channels = Arc::new(next);
        self.next_key = self
            .channels
            .iter()
            .map(|c| c.curve.max_id())
            .max()
            .unwrap_or(0)
            .max(self.next_key);
        self.prune_selection();
        true
    }

    fn prune_selection(&mut self) {
        if self.selection.iter().any(|k| self.key(*k).is_none()) {
            let kept = self
                .selection
                .iter()
                .copied()
                .filter(|k| self.key(*k).is_some())
                .collect();
            self.selection = Arc::new(kept);
        }
    }

    fn fresh_key(&mut self) -> KeyId {
        self.next_key += 1;
        KeyId(self.next_key)
    }

    /// Apply a user action. Returns what changed, or `None` for a no-op or a refused edit.
    pub fn apply(&mut self, action: DopeAction) -> Option<DopeChange> {
        match action {
            DopeAction::InsertKey { channel, t, value } => {
                let c = self.channel(channel).filter(|c| !c.group)?;
                if !t.is_finite()
                    || t < 0.0
                    || !self.editable(channel)
                    || c.curve.at_time(t).is_some()
                {
                    return None;
                }
                let v = match c.kind {
                    ChannelKind::Keys => 0.0,
                    ChannelKind::Value => c
                        .curve
                        .bounds
                        .clamp(value.unwrap_or_else(|| c.curve.value_at(t))),
                    ChannelKind::Toggle => {
                        let on = value.map_or(!c.curve.is_on(t), |v| v >= 0.5);
                        if on { 1.0 } else { 0.0 }
                    }
                };
                let interp = match c.kind {
                    ChannelKind::Toggle => Interp::Constant,
                    _ => c.curve.default_interp,
                };
                let id = self.fresh_key();
                let key = DopeKey {
                    id,
                    key: Key::new(t, v, interp),
                };
                self.channel_mut(channel)?.curve.insert(key).then_some(())?;
                Some(DopeChange::KeyInserted {
                    key: KeyRef { channel, key: id },
                    t,
                    v,
                })
            }
            DopeAction::RemoveKeys(keys) => {
                let keys: Vec<KeyRef> = keys
                    .into_iter()
                    .filter(|k| self.key(*k).is_some() && self.editable(k.channel))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if keys.is_empty() {
                    return None;
                }
                for k in &keys {
                    self.channel_mut(k.channel)?.curve.remove(k.key);
                }
                self.prune_selection();
                Some(DopeChange::KeysRemoved { keys })
            }
            DopeAction::RetimeKeys { keys, delta } => {
                let keys = self.movable(keys);
                let moves = self.check_moves(&keys, delta)?;
                for (k, to) in moves {
                    let c = self.channel_mut(k.channel)?;
                    if let Some(key) = c.curve.key_mut(k.key) {
                        key.key.t = to;
                    }
                }
                for c in keys.iter().map(|k| k.channel).collect::<BTreeSet<_>>() {
                    self.channel_mut(c)?.curve.resort();
                }
                Some(DopeChange::KeysRetimed { keys, delta })
            }
            DopeAction::DuplicateKeys { keys, delta } => {
                let keys = self.movable(keys);
                let moves = self.check_moves(&keys, delta)?;
                // Copies may not land on the originals either.
                if moves.iter().any(|(k, to)| {
                    self.channel(k.channel)
                        .is_some_and(|c| c.curve.at_time(*to).is_some())
                }) {
                    return None;
                }
                let mut copies = Vec::with_capacity(moves.len());
                for (k, to) in moves {
                    let source = *self.key(k)?;
                    let id = self.fresh_key();
                    let mut key = source.key;
                    key.t = to;
                    self.channel_mut(k.channel)?
                        .curve
                        .insert(DopeKey { id, key })
                        .then_some(())?;
                    copies.push(KeyRef {
                        channel: k.channel,
                        key: id,
                    });
                }
                self.selection = Arc::new(copies.iter().copied().collect());
                Some(DopeChange::KeysDuplicated {
                    keys: copies,
                    delta,
                })
            }
            DopeAction::SetKeyValue { key, value } => {
                let c = self.channel(key.channel)?;
                if c.kind != ChannelKind::Value || !value.is_finite() || !self.editable(key.channel)
                {
                    return None;
                }
                let to = c.curve.bounds.clamp(value);
                let from = c.curve.key(key.key)?.key.v;
                if from == to {
                    return None;
                }
                self.channel_mut(key.channel)?.curve.key_mut(key.key)?.key.v = to;
                Some(DopeChange::KeyValueChanged { key, from, to })
            }
            DopeAction::SetSegmentInterp { key, interp } => {
                let c = self.channel(key.channel)?;
                if c.kind != ChannelKind::Value || !self.editable(key.channel) {
                    return None;
                }
                if c.curve.key(key.key)?.key.interp == interp {
                    return None;
                }
                self.channel_mut(key.channel)?
                    .curve
                    .key_mut(key.key)?
                    .key
                    .interp = interp;
                Some(DopeChange::SegmentInterpChanged { key, interp })
            }
            DopeAction::BendSegment { key, amount } => {
                let c = self.channel(key.channel)?;
                if c.kind != ChannelKind::Value
                    || !amount.is_finite()
                    || !self.editable(key.channel)
                {
                    return None;
                }
                let amount = amount.clamp(-1.0, 1.0);
                let i = c.curve.index_of(key.key)?;
                let (a, b) = (c.curve.keys.get(i)?.key, c.curve.keys.get(i + 1)?.key);
                let (dt, dv) = (b.t - a.t, b.v - a.v);
                // Straight at 0; positive bends toward the end value early (ease out),
                // negative late (ease in).
                let out_h = (dt / 3.0, dv / 3.0 * (1.0 + 2.0 * amount));
                let in_h = (-dt / 3.0, -dv / 3.0 * (1.0 - 2.0 * amount));
                if a.interp == Interp::Bezier && a.out_handle == out_h && b.in_handle == in_h {
                    return None;
                }
                let curve = &mut self.channel_mut(key.channel)?.curve;
                curve.keys[i].key.interp = Interp::Bezier;
                curve.keys[i].key.out_handle = out_h;
                curve.keys[i + 1].key.in_handle = in_h;
                Some(DopeChange::SegmentBent { key, amount })
            }
            DopeAction::ToggleSpan { channel, t } => {
                let c = self.channel(channel)?;
                if c.kind != ChannelKind::Toggle
                    || !t.is_finite()
                    || t < 0.0
                    || !self.editable(channel)
                {
                    return None;
                }
                let on = !c.curve.is_on(t);
                let existing = c.curve.at_time(t).map(|k| k.id);
                let fresh = existing.is_none().then(|| self.fresh_key());
                let curve = &mut self.channel_mut(channel)?.curve;
                let v = if on { 1.0 } else { 0.0 };
                match (existing, fresh) {
                    (Some(id), _) => curve.key_mut(id)?.key.v = v,
                    (None, Some(id)) => {
                        curve.insert(DopeKey {
                            id,
                            key: Key::new(t, v, Interp::Constant),
                        });
                    }
                    (None, None) => return None,
                }
                // Merge: a switch that repeats the state before it is redundant.
                let mut state = 0.0;
                curve.keys.retain(|k| {
                    let keep = k.key.v != state;
                    state = k.key.v;
                    keep
                });
                self.prune_selection();
                Some(DopeChange::SpanToggled { channel, t, on })
            }
            DopeAction::SetCurve { channel, keys } => {
                let c = self.channel(channel)?;
                if c.kind != ChannelKind::Value || !self.editable(channel) {
                    return None;
                }
                let old = c.curve.keys().to_vec();
                let bounds = c.curve.bounds;
                let same_len = old.len() == keys.len();
                let mut used = BTreeSet::new();
                let mut next = Vec::with_capacity(keys.len());
                for (i, mut key) in keys.into_iter().enumerate() {
                    if !(key.t.is_finite() && key.v.is_finite()) {
                        return None;
                    }
                    key.v = bounds.clamp(key.v);
                    let reuse = old
                        .iter()
                        .find(|k| k.key.t == key.t && !used.contains(&k.id))
                        .or_else(|| same_len.then(|| &old[i]).filter(|k| !used.contains(&k.id)))
                        .map(|k| k.id);
                    let id = match reuse {
                        Some(id) => id,
                        None => self.fresh_key(),
                    };
                    used.insert(id);
                    next.push(DopeKey { id, key });
                }
                let curve =
                    ChannelCurve::new(next, bounds, self.channel(channel)?.curve.default_interp);
                if curve.keys() == old.as_slice() {
                    return None;
                }
                self.channel_mut(channel)?.curve = curve;
                self.prune_selection();
                Some(DopeChange::CurveEdited { channel })
            }
            DopeAction::Select { keys, add } => {
                let keys = keys.into_iter().filter(|k| self.key(*k).is_some());
                let next: BTreeSet<KeyRef> = if add {
                    let mut set = (*self.selection).clone();
                    for k in keys {
                        if !set.remove(&k) {
                            set.insert(k);
                        }
                    }
                    set
                } else {
                    keys.collect()
                };
                if next == *self.selection {
                    return None;
                }
                self.selection = Arc::new(next);
                Some(DopeChange::SelectionChanged)
            }
            DopeAction::SetChannelFlag { channel, flag, on } => {
                let c = self.channel(channel)?;
                let current = match flag {
                    ChannelFlag::Collapsed => c.collapsed,
                    ChannelFlag::Muted => c.muted,
                    ChannelFlag::Locked => c.locked,
                    ChannelFlag::Visible => c.visible,
                    ChannelFlag::Expanded => c.expanded,
                };
                if current == on || (flag == ChannelFlag::Expanded && c.kind != ChannelKind::Value)
                {
                    return None;
                }
                let c = self.channel_mut(channel)?;
                match flag {
                    ChannelFlag::Collapsed => c.collapsed = on,
                    ChannelFlag::Muted => c.muted = on,
                    ChannelFlag::Locked => c.locked = on,
                    ChannelFlag::Visible => c.visible = on,
                    ChannelFlag::Expanded => c.expanded = on,
                }
                Some(DopeChange::ChannelChanged { channel, flag, on })
            }
            DopeAction::Seek(time) => {
                let time = time.is_finite().then_some(time.max(0.0))?;
                (self.playhead != time).then(|| {
                    self.playhead = time;
                    DopeChange::Seek { time }
                })
            }
        }
    }

    /// The keys that exist and whose channels are editable, deduplicated.
    fn movable(&self, keys: Vec<KeyRef>) -> Vec<KeyRef> {
        keys.into_iter()
            .filter(|k| self.key(*k).is_some() && self.editable(k.channel))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Destinations for moving `keys` by `delta`, or `None` if any is invalid or lands on
    /// a key that isn't moving.
    fn check_moves(&self, keys: &[KeyRef], delta: f64) -> Option<Vec<(KeyRef, f64)>> {
        if keys.is_empty() || !delta.is_finite() || delta == 0.0 {
            return None;
        }
        let moving: BTreeSet<KeyRef> = keys.iter().copied().collect();
        let mut landing = BTreeSet::new();
        let mut moves = Vec::with_capacity(keys.len());
        for k in keys {
            let to = self.key(*k)?.key.t + delta;
            if !to.is_finite() || to < 0.0 || !landing.insert((k.channel, to.to_bits())) {
                return None;
            }
            let blocked = self
                .channel(k.channel)?
                .curve
                .at_time(to)
                .is_some_and(|other| {
                    !moving.contains(&KeyRef {
                        channel: k.channel,
                        key: other.id,
                    })
                });
            if blocked {
                return None;
            }
            moves.push((*k, to));
        }
        Some(moves)
    }
}

/// Erased action names (the payload is a record; key lists are `[{channel, key}]`).
pub mod action_names {
    pub const INSERT_KEY: &str = "insert_key";
    pub const REMOVE_KEYS: &str = "remove_keys";
    pub const RETIME_KEYS: &str = "retime_keys";
    pub const DUPLICATE_KEYS: &str = "duplicate_keys";
    pub const SET_KEY_VALUE: &str = "set_key_value";
    pub const SET_SEGMENT_INTERP: &str = "set_segment_interp";
    pub const BEND_SEGMENT: &str = "bend_segment";
    pub const TOGGLE_SPAN: &str = "toggle_span";
    /// `{channel, keys: [{time, value, interpolation, inHandle: [dt, dv], outHandle}]}`
    pub const SET_CURVE: &str = "set_curve";
    pub const SELECT: &str = guiraffe_core::protocol::names::SELECT;
    pub const SET_CHANNEL_FLAG: &str = "set_channel_flag";
    pub const SEEK: &str = "seek";
}

fn record(fields: Vec<(&'static str, Value)>) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(k, v)| (SharedString::new_static(k), v))
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn get<'a>(value: &'a Value, name: &'static str) -> &'a Value {
    value.field(&[SharedString::new_static(name)])
}

fn num(value: &Value, name: &'static str) -> Result<f64, ActionError> {
    get(value, name)
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(ActionError::Invalid(name))
}

fn id(value: &Value, name: &'static str) -> Result<u64, ActionError> {
    let v = num(value, name)?;
    (v >= 0.0 && v.fract() == 0.0 && v <= 9_007_199_254_740_992.0)
        .then_some(v as u64)
        .ok_or(ActionError::Invalid(name))
}

fn key_ref(value: &Value) -> Result<KeyRef, ActionError> {
    Ok(KeyRef::new(id(value, "channel")?, id(value, "key")?))
}

fn key_fields(k: KeyRef) -> Vec<(&'static str, Value)> {
    vec![
        ("channel", Value::Num(k.channel.0 as f64)),
        ("key", Value::Num(k.key.0 as f64)),
    ]
}

fn key_list(keys: &[KeyRef]) -> Value {
    Value::List(keys.iter().map(|k| record(key_fields(*k))).collect())
}

fn parse_keys(value: &Value) -> Result<Vec<KeyRef>, ActionError> {
    let list = get(value, "keys");
    let n = list.list_len().ok_or(ActionError::Invalid("keys"))?;
    (0..n)
        .map(|i| {
            list.list_get(i)
                .ok_or(ActionError::Invalid("keys"))
                .and_then(key_ref)
        })
        .collect()
}

fn flag_name(flag: ChannelFlag) -> &'static str {
    match flag {
        ChannelFlag::Collapsed => "collapsed",
        ChannelFlag::Muted => "muted",
        ChannelFlag::Locked => "locked",
        ChannelFlag::Visible => "visible",
        ChannelFlag::Expanded => "expanded",
    }
}

fn parse_flag(name: Option<&str>) -> Result<ChannelFlag, ActionError> {
    Ok(match name {
        Some("collapsed") => ChannelFlag::Collapsed,
        Some("muted") => ChannelFlag::Muted,
        Some("locked") => ChannelFlag::Locked,
        Some("visible") => ChannelFlag::Visible,
        Some("expanded") => ChannelFlag::Expanded,
        _ => return Err(ActionError::Invalid("flag")),
    })
}

impl DopeAction {
    /// The erased form a bus client sends.
    pub fn to_action(&self) -> Action {
        use action_names::*;
        let (name, payload) = match self {
            Self::InsertKey { channel, t, value } => {
                let mut f = vec![
                    ("channel", Value::Num(channel.0 as f64)),
                    ("time", Value::Num(*t)),
                ];
                if let Some(v) = value {
                    f.push(("value", Value::Num(*v)));
                }
                (INSERT_KEY, record(f))
            }
            Self::RemoveKeys(keys) => (REMOVE_KEYS, record(vec![("keys", key_list(keys))])),
            Self::RetimeKeys { keys, delta } => (
                RETIME_KEYS,
                record(vec![
                    ("keys", key_list(keys)),
                    ("delta", Value::Num(*delta)),
                ]),
            ),
            Self::DuplicateKeys { keys, delta } => (
                DUPLICATE_KEYS,
                record(vec![
                    ("keys", key_list(keys)),
                    ("delta", Value::Num(*delta)),
                ]),
            ),
            Self::SetKeyValue { key, value } => {
                let mut f = key_fields(*key);
                f.push(("value", Value::Num(*value)));
                (SET_KEY_VALUE, record(f))
            }
            Self::SetSegmentInterp { key, interp } => {
                let mut f = key_fields(*key);
                f.push(("interpolation", Value::Str(interp.name().into())));
                (SET_SEGMENT_INTERP, record(f))
            }
            Self::BendSegment { key, amount } => {
                let mut f = key_fields(*key);
                f.push(("amount", Value::Num(*amount)));
                (BEND_SEGMENT, record(f))
            }
            Self::ToggleSpan { channel, t } => (
                TOGGLE_SPAN,
                record(vec![
                    ("channel", Value::Num(channel.0 as f64)),
                    ("time", Value::Num(*t)),
                ]),
            ),
            Self::SetCurve { channel, keys } => (
                SET_CURVE,
                record(vec![
                    ("channel", Value::Num(channel.0 as f64)),
                    (
                        "keys",
                        Value::List(
                            keys.iter()
                                .map(|k| {
                                    let pair = |(a, b): (f64, f64)| {
                                        Value::List([Value::Num(a), Value::Num(b)].into())
                                    };
                                    record(vec![
                                        ("time", Value::Num(k.t)),
                                        ("value", Value::Num(k.v)),
                                        ("interpolation", Value::Str(k.interp.name().into())),
                                        ("inHandle", pair(k.in_handle)),
                                        ("outHandle", pair(k.out_handle)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ]),
            ),
            Self::Select { keys, add } => (
                SELECT,
                record(vec![("keys", key_list(keys)), ("add", Value::Bool(*add))]),
            ),
            Self::SetChannelFlag { channel, flag, on } => (
                SET_CHANNEL_FLAG,
                record(vec![
                    ("channel", Value::Num(channel.0 as f64)),
                    ("flag", Value::Str(flag_name(*flag).into())),
                    ("on", Value::Bool(*on)),
                ]),
            ),
            Self::Seek(t) => (SEEK, record(vec![("time", Value::Num(*t))])),
        };
        Action {
            name: SharedString::new_static(name),
            payload,
        }
    }

    pub fn decode(action: &Action) -> Result<Self, ActionError> {
        use action_names::*;
        let p = &action.payload;
        Ok(match action.name.as_ref() {
            INSERT_KEY => Self::InsertKey {
                channel: ChannelId(id(p, "channel")?),
                t: num(p, "time")?,
                value: get(p, "value").as_f64().filter(|v| v.is_finite()),
            },
            REMOVE_KEYS => Self::RemoveKeys(parse_keys(p)?),
            RETIME_KEYS => Self::RetimeKeys {
                keys: parse_keys(p)?,
                delta: num(p, "delta")?,
            },
            DUPLICATE_KEYS => Self::DuplicateKeys {
                keys: parse_keys(p)?,
                delta: num(p, "delta")?,
            },
            SET_KEY_VALUE => Self::SetKeyValue {
                key: key_ref(p)?,
                value: num(p, "value")?,
            },
            SET_SEGMENT_INTERP => Self::SetSegmentInterp {
                key: key_ref(p)?,
                interp: get(p, "interpolation")
                    .as_str()
                    .and_then(Interp::from_name)
                    .ok_or(ActionError::Invalid("interpolation"))?,
            },
            BEND_SEGMENT => Self::BendSegment {
                key: key_ref(p)?,
                amount: num(p, "amount")?,
            },
            TOGGLE_SPAN => Self::ToggleSpan {
                channel: ChannelId(id(p, "channel")?),
                t: num(p, "time")?,
            },
            SET_CURVE => Self::SetCurve {
                channel: ChannelId(id(p, "channel")?),
                keys: {
                    let list = get(p, "keys");
                    let n = list.list_len().ok_or(ActionError::Invalid("keys"))?;
                    let pair = |v: &Value| -> (f64, f64) {
                        let at = |i| v.list_get(i).and_then(Value::as_f64).unwrap_or(0.0);
                        (at(0), at(1))
                    };
                    (0..n)
                        .map(|i| {
                            let k = list.list_get(i).ok_or(ActionError::Invalid("keys"))?;
                            let mut key = Key::new(
                                num(k, "time")?,
                                num(k, "value")?,
                                get(k, "interpolation")
                                    .as_str()
                                    .and_then(Interp::from_name)
                                    .unwrap_or(Interp::Linear),
                            );
                            key.in_handle = pair(get(k, "inHandle"));
                            key.out_handle = pair(get(k, "outHandle"));
                            Ok(key)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                },
            },
            SELECT => Self::Select {
                keys: parse_keys(p)?,
                add: get(p, "add").as_bool().unwrap_or(false),
            },
            SET_CHANNEL_FLAG => Self::SetChannelFlag {
                channel: ChannelId(id(p, "channel")?),
                flag: parse_flag(get(p, "flag").as_str())?,
                on: get(p, "on").as_bool().ok_or(ActionError::Invalid("on"))?,
            },
            SEEK => Self::Seek(match p.as_f64() {
                Some(t) if t.is_finite() => t,
                _ => num(p, "time")?,
            }),
            _ => return Err(ActionError::Unsupported),
        })
    }
}

impl From<&DopeChange> for Value {
    fn from(change: &DopeChange) -> Self {
        let kind = |k: &'static str| ("kind", Value::Str(SharedString::new_static(k)));
        match change {
            DopeChange::KeyInserted { key, t, v } => {
                let mut f = vec![kind("keyInserted")];
                f.extend(key_fields(*key));
                f.extend([("time", Value::Num(*t)), ("value", Value::Num(*v))]);
                record(f)
            }
            DopeChange::KeysRemoved { keys } => {
                record(vec![kind("keysRemoved"), ("keys", key_list(keys))])
            }
            DopeChange::KeysRetimed { keys, delta } => record(vec![
                kind("keysRetimed"),
                ("keys", key_list(keys)),
                ("delta", Value::Num(*delta)),
            ]),
            DopeChange::KeysDuplicated { keys, delta } => record(vec![
                kind("keysDuplicated"),
                ("keys", key_list(keys)),
                ("delta", Value::Num(*delta)),
            ]),
            DopeChange::KeyValueChanged { key, from, to } => {
                let mut f = vec![kind("keyValueChanged")];
                f.extend(key_fields(*key));
                f.extend([("from", Value::Num(*from)), ("to", Value::Num(*to))]);
                record(f)
            }
            DopeChange::SegmentInterpChanged { key, interp } => {
                let mut f = vec![kind("segmentInterpChanged")];
                f.extend(key_fields(*key));
                f.push(("interpolation", Value::Str(interp.name().into())));
                record(f)
            }
            DopeChange::SegmentBent { key, amount } => {
                let mut f = vec![kind("segmentBent")];
                f.extend(key_fields(*key));
                f.push(("amount", Value::Num(*amount)));
                record(f)
            }
            DopeChange::SpanToggled { channel, t, on } => record(vec![
                kind("spanToggled"),
                ("channel", Value::Num(channel.0 as f64)),
                ("time", Value::Num(*t)),
                ("on", Value::Bool(*on)),
            ]),
            DopeChange::CurveEdited { channel } => record(vec![
                kind("curveEdited"),
                ("channel", Value::Num(channel.0 as f64)),
            ]),
            DopeChange::SelectionChanged => record(vec![kind("selectionChanged")]),
            DopeChange::ChannelChanged { channel, flag, on } => record(vec![
                kind("channelChanged"),
                ("channel", Value::Num(channel.0 as f64)),
                ("flag", Value::Str(flag_name(*flag).into())),
                ("on", Value::Bool(*on)),
            ]),
            DopeChange::Seek { time } => record(vec![kind("seek"), ("time", Value::Num(*time))]),
        }
    }
}

/// A store with lens undo over the channels' content, so selection, playhead and view flags
/// are never undone.
pub fn dope_store(doc: DopeDoc, undo: impl Into<HistoryLimits>, cx: &mut App) -> Store<DopeDoc> {
    let store = Store::new(doc, cx);
    store.enable_undo_lens(
        undo,
        |d: &DopeDoc| Arc::clone(&d.channels),
        |d: &mut DopeDoc, channels| {
            // Keep the current view flags; undo restores content only.
            let flags: HashMap<ChannelId, (bool, bool, bool, bool, bool)> = d
                .channels
                .iter()
                .map(|c| {
                    (
                        c.id,
                        (c.collapsed, c.expanded, c.muted, c.locked, c.visible),
                    )
                })
                .collect();
            let restored = channels
                .iter()
                .map(|c| match flags.get(&c.id) {
                    Some(&f) if f != (c.collapsed, c.expanded, c.muted, c.locked, c.visible) => {
                        let mut c = (**c).clone();
                        (c.collapsed, c.expanded, c.muted, c.locked, c.visible) = f;
                        Arc::new(c)
                    }
                    _ => Arc::clone(c),
                })
                .collect();
            d.channels = Arc::new(restored);
            d.prune_selection();
        },
        cx,
    );
    store
}

/// Apply a user action to the store (undoable unless it selects, seeks or flips a flag).
#[derive(Debug, Clone)]
pub struct ApplyDopeAction(pub DopeAction);

impl Event<DopeDoc> for ApplyDopeAction {
    fn handle(self, fx: &mut Fx<'_, DopeDoc>) {
        let mut next = fx.state().clone();
        if next.apply(self.0).is_some() {
            fx.set(next);
        }
    }
    fn undoable(&self) -> bool {
        self.0.undoable()
    }
}

macro_rules! host_event {
    ($(#[$doc:meta])* $name:ident($($field:ty),*), |$m:ident, $($arg:ident),*| $body:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $name($(pub $field),*);
        impl Event<DopeDoc> for $name {
            fn handle(self, fx: &mut Fx<'_, DopeDoc>) {
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
    /// Host write: replace the channels, diffed by id (see [`DopeDoc::set_channels`]).
    SetChannels(Vec<DopeChannel>),
    |m, channels| m.set_channels(channels)
);
host_event!(
    /// Host write: the playhead (the transport).
    SetPlayhead(f64),
    |m, time| if time.is_finite() {
        m.playhead = time.max(0.0)
    }
);
host_event!(
    /// Host write: the selection (keys that don't exist are dropped).
    SetDopeSelection(Vec<KeyRef>),
    |m, keys| {
        let keys: BTreeSet<KeyRef> = keys.into_iter().filter(|k| m.key(*k).is_some()).collect();
        if keys != *m.selection {
            m.selection = Arc::new(keys);
        }
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    fn value(id: u64, keys: &[(u64, f64, f64, Interp)]) -> DopeChannel {
        DopeChannel::value(
            id,
            format!("v{id}"),
            keys.iter()
                .map(|&(k, t, v, i)| DopeKey::new(k, t, v, i))
                .collect(),
            ValueBounds::new(0.0, 10.0),
        )
    }

    fn doc() -> DopeDoc {
        DopeDoc::new(vec![
            DopeChannel::group(1, "Group"),
            value(
                10,
                &[
                    (1, 0.0, 0.0, Interp::Linear),
                    (2, 2.0, 10.0, Interp::Constant),
                    (3, 4.0, 4.0, Interp::Linear),
                ],
            )
            .with_parent(1),
            DopeChannel::toggle(
                20,
                "Bypass",
                vec![(KeyId(5), 1.0, true), (KeyId(6), 3.0, false)],
            )
            .with_parent(1),
            DopeChannel::keys(30, "Notes", vec![DopeKey::new(7, 0.5, 0.0, Interp::Linear)]),
        ])
    }

    fn k(c: u64, key: u64) -> KeyRef {
        KeyRef::new(c, key)
    }

    #[test]
    fn value_at_follows_each_segment_mode() {
        let d = doc();
        let curve = &d
            .channel(ChannelId(10))
            .map(|c| c.curve.clone())
            .unwrap_or_default();
        assert_eq!(curve.value_at(-1.0), 0.0);
        assert_eq!(curve.value_at(1.0), 5.0, "linear");
        assert_eq!(curve.value_at(3.0), 10.0, "constant holds");
        assert_eq!(curve.value_at(9.0), 4.0, "held after the last key");
        let toggle = &d
            .channel(ChannelId(20))
            .map(|c| c.curve.clone())
            .unwrap_or_default();
        assert!(!toggle.is_on(0.5) && toggle.is_on(1.0) && toggle.is_on(2.9) && !toggle.is_on(3.0));
    }

    #[test]
    fn insert_key_takes_the_curve_value_and_a_fresh_id() {
        let mut d = doc();
        let change = d.apply(DopeAction::InsertKey {
            channel: ChannelId(10),
            t: 1.0,
            value: None,
        });
        let Some(DopeChange::KeyInserted { key, v, .. }) = change else {
            panic!("{change:?}");
        };
        assert_eq!(v, 5.0, "keeps the curve");
        assert_eq!(key.key, KeyId(8), "fresh: above every id in use");
        assert!(d.key(key).is_some());
        assert_eq!(
            d.apply(DopeAction::InsertKey {
                channel: ChannelId(10),
                t: 1.0,
                value: Some(2.0)
            }),
            None,
            "a key already sits there"
        );
        assert_eq!(
            d.apply(DopeAction::InsertKey {
                channel: ChannelId(1),
                t: 5.0,
                value: None
            }),
            None,
            "groups hold no keys"
        );
        let Some(DopeChange::KeyInserted { v, .. }) = d.apply(DopeAction::InsertKey {
            channel: ChannelId(10),
            t: 6.0,
            value: Some(99.0),
        }) else {
            panic!("insert");
        };
        assert_eq!(v, 10.0, "clamped to the bounds");
    }

    #[test]
    fn remove_retime_and_duplicate_keep_ids_and_refuse_collisions() {
        let mut d = doc();
        assert_eq!(
            d.apply(DopeAction::RetimeKeys {
                keys: vec![k(10, 1)],
                delta: 2.0
            }),
            None,
            "lands on key 2"
        );
        assert_eq!(
            d.apply(DopeAction::RetimeKeys {
                keys: vec![k(10, 1), k(10, 2)],
                delta: 2.0
            }),
            None,
            "key 2 would land on key 3, which isn't moving"
        );
        assert!(matches!(
            d.apply(DopeAction::RetimeKeys {
                keys: vec![k(10, 1), k(10, 2)],
                delta: 1.0
            }),
            Some(DopeChange::KeysRetimed { .. })
        ));
        let times: Vec<(u64, f64)> = d
            .channel(ChannelId(10))
            .map(|c| c.curve.keys().iter().map(|k| (k.id.0, k.key.t)).collect())
            .unwrap_or_default();
        assert_eq!(
            times,
            [(1, 1.0), (2, 3.0), (3, 4.0)],
            "ids travel with their keys"
        );
        assert_eq!(
            d.apply(DopeAction::RetimeKeys {
                keys: vec![k(10, 1)],
                delta: -5.0
            }),
            None,
            "before 0"
        );
        let Some(DopeChange::KeysDuplicated { keys, .. }) = d.apply(DopeAction::DuplicateKeys {
            keys: vec![k(30, 7)],
            delta: 1.0,
        }) else {
            panic!("duplicate");
        };
        assert_eq!(
            d.selection.iter().copied().collect::<Vec<_>>(),
            keys,
            "the copies are selected"
        );
        assert_eq!(d.key(keys[0]).map(|k| k.key.t), Some(1.5));
        d.selection = Arc::new([k(10, 1)].into_iter().collect());
        assert_eq!(
            d.apply(DopeAction::RemoveKeys(vec![k(10, 1), k(10, 99)])),
            Some(DopeChange::KeysRemoved {
                keys: vec![k(10, 1)]
            })
        );
        assert!(d.selection.is_empty(), "removed keys leave the selection");
    }

    #[test]
    fn values_interpolation_and_bends_edit_one_segment() {
        let mut d = doc();
        assert_eq!(
            d.apply(DopeAction::SetKeyValue {
                key: k(10, 3),
                value: 7.0
            }),
            Some(DopeChange::KeyValueChanged {
                key: k(10, 3),
                from: 4.0,
                to: 7.0
            })
        );
        assert_eq!(
            d.apply(DopeAction::SetKeyValue {
                key: k(30, 7),
                value: 1.0
            }),
            None,
            "markers hold no value"
        );
        assert_eq!(
            d.apply(DopeAction::SetSegmentInterp {
                key: k(10, 1),
                interp: Interp::EaseInOut
            }),
            Some(DopeChange::SegmentInterpChanged {
                key: k(10, 1),
                interp: Interp::EaseInOut
            })
        );
        let curve = d
            .channel(ChannelId(10))
            .map(|c| c.curve.clone())
            .unwrap_or_default();
        assert_eq!(curve.value_at(1.0), 5.0, "symmetric ease: midpoint");
        assert!(curve.value_at(0.5) < 2.5, "eases in");
        assert!(matches!(
            d.apply(DopeAction::BendSegment {
                key: k(10, 1),
                amount: 0.8
            }),
            Some(DopeChange::SegmentBent { .. })
        ));
        let curve = d
            .channel(ChannelId(10))
            .map(|c| c.curve.clone())
            .unwrap_or_default();
        assert!(curve.value_at(0.5) > 2.5, "bent toward the end value early");
        assert_eq!(curve.value_at(0.0), 0.0);
        assert!((curve.value_at(2.0) - 10.0).abs() < 1e-9, "ends stay put");
        assert_eq!(
            d.apply(DopeAction::BendSegment {
                key: k(10, 3),
                amount: 0.5
            }),
            None,
            "the last key starts no segment"
        );
    }

    #[test]
    fn toggling_a_span_splits_at_the_time_and_merges_repeats() {
        let mut d = doc(); // on 1..3
        let on = |d: &DopeDoc, t| d.channel(ChannelId(20)).is_some_and(|c| c.curve.is_on(t));
        assert_eq!(
            d.apply(DopeAction::ToggleSpan {
                channel: ChannelId(20),
                t: 2.0
            }),
            Some(DopeChange::SpanToggled {
                channel: ChannelId(20),
                t: 2.0,
                on: false
            })
        );
        assert!(on(&d, 1.5) && !on(&d, 2.0) && !on(&d, 2.9));
        let switches = |d: &DopeDoc| {
            d.channel(ChannelId(20))
                .map(|c| c.curve.keys().len())
                .unwrap_or(0)
        };
        assert_eq!(switches(&d), 2, "the off at 3 merged into the new off at 2");
        assert!(
            d.apply(DopeAction::ToggleSpan {
                channel: ChannelId(20),
                t: 0.0
            })
            .is_some()
        );
        assert!(on(&d, 0.0) && on(&d, 1.9) && !on(&d, 2.0));
        assert_eq!(switches(&d), 2, "on at 0 merged with on at 1");
        assert_eq!(
            d.apply(DopeAction::ToggleSpan {
                channel: ChannelId(10),
                t: 1.0
            }),
            None,
            "value channel"
        );
    }

    #[test]
    fn set_curve_keeps_ids_by_time_then_position() {
        let mut d = doc();
        let curve = |d: &DopeDoc| -> Vec<(u64, f64, f64)> {
            d.channel(ChannelId(10))
                .map(|c| {
                    c.curve
                        .keys()
                        .iter()
                        .map(|k| (k.id.0, k.key.t, k.key.v))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut keys: Vec<Key> = curve(&d)
            .iter()
            .map(|&(_, t, v)| Key::new(t, v, Interp::Linear))
            .collect();
        keys[1].v = 6.0; // same time: keeps id 2
        keys[2].t = 5.0; // moved, same count: keeps id 3 by position
        assert_eq!(
            d.apply(DopeAction::SetCurve {
                channel: ChannelId(10),
                keys: keys.clone()
            }),
            Some(DopeChange::CurveEdited {
                channel: ChannelId(10)
            })
        );
        assert_eq!(curve(&d), [(1, 0.0, 0.0), (2, 2.0, 6.0), (3, 5.0, 4.0)]);
        keys.push(Key::new(7.0, 1.0, Interp::Linear));
        assert!(
            d.apply(DopeAction::SetCurve {
                channel: ChannelId(10),
                keys
            })
            .is_some()
        );
        let after = curve(&d);
        assert_eq!(
            after[..3],
            [(1, 0.0, 0.0), (2, 2.0, 6.0), (3, 5.0, 4.0)],
            "kept by time"
        );
        assert!(
            after[3].0 > 7 && after[3].1 == 7.0,
            "a new key gets a fresh id"
        );
    }

    #[test]
    fn locks_refuse_edits_through_groups() {
        let mut d = doc();
        assert!(
            d.apply(DopeAction::SetChannelFlag {
                channel: ChannelId(1),
                flag: ChannelFlag::Locked,
                on: true
            })
            .is_some()
        );
        assert_eq!(
            d.apply(DopeAction::SetKeyValue {
                key: k(10, 1),
                value: 3.0
            }),
            None
        );
        assert_eq!(
            d.apply(DopeAction::ToggleSpan {
                channel: ChannelId(20),
                t: 2.0
            }),
            None
        );
        assert!(
            d.apply(DopeAction::InsertKey {
                channel: ChannelId(30),
                t: 3.0,
                value: None
            })
            .is_some()
        );
        assert!(
            d.apply(DopeAction::Select {
                keys: vec![k(10, 1)],
                add: false
            })
            .is_some(),
            "selection still works"
        );
    }

    #[test]
    fn set_channels_diffs_by_id_and_keeps_flags_and_selection() {
        let mut d = doc();
        d.apply(DopeAction::SetChannelFlag {
            channel: ChannelId(10),
            flag: ChannelFlag::Expanded,
            on: true,
        });
        d.apply(DopeAction::Select {
            keys: vec![k(10, 2), k(30, 7)],
            add: false,
        });
        let before = d.clone();
        let mut next: Vec<DopeChannel> = d.channels().iter().map(|c| (**c).clone()).collect();
        next.reverse();
        next.insert(1, DopeChannel::keys(40, "New", vec![]));
        next.retain(|c| c.id != ChannelId(30));
        for c in &mut next {
            c.expanded = false; // hosts don't know view flags
        }
        assert!(d.set_channels(next));
        let ids: Vec<u64> = d.channels().iter().map(|c| c.id.0).collect();
        assert_eq!(ids, [40, 20, 10, 1]);
        assert!(
            d.channel(ChannelId(10)).is_some_and(|c| c.expanded),
            "view flag kept"
        );
        assert!(
            Arc::ptr_eq(&d.channels()[1], &before.channels()[2]),
            "channel 20 is unchanged: its allocation moves with it"
        );
        assert_eq!(
            d.selection.iter().copied().collect::<Vec<_>>(),
            [k(10, 2)],
            "selection follows keys; removed channel's key dropped"
        );
    }

    #[test]
    fn erased_actions_round_trip() {
        for action in [
            DopeAction::InsertKey {
                channel: ChannelId(3),
                t: 1.5,
                value: Some(0.25),
            },
            DopeAction::InsertKey {
                channel: ChannelId(3),
                t: 1.5,
                value: None,
            },
            DopeAction::RemoveKeys(vec![k(1, 2), k(3, 4)]),
            DopeAction::RetimeKeys {
                keys: vec![k(1, 2)],
                delta: -0.5,
            },
            DopeAction::DuplicateKeys {
                keys: vec![k(1, 2)],
                delta: 1.0,
            },
            DopeAction::SetKeyValue {
                key: k(1, 2),
                value: 3.0,
            },
            DopeAction::SetSegmentInterp {
                key: k(1, 2),
                interp: Interp::EaseOut,
            },
            DopeAction::BendSegment {
                key: k(1, 2),
                amount: -0.5,
            },
            DopeAction::ToggleSpan {
                channel: ChannelId(9),
                t: 2.0,
            },
            DopeAction::SetCurve {
                channel: ChannelId(9),
                keys: vec![Key::new(0.0, 1.0, Interp::EaseIn), {
                    let mut k = Key::new(2.0, 3.0, Interp::Bezier);
                    k.in_handle = (-0.5, 0.25);
                    k
                }],
            },
            DopeAction::Select {
                keys: vec![k(1, 2)],
                add: true,
            },
            DopeAction::SetChannelFlag {
                channel: ChannelId(9),
                flag: ChannelFlag::Expanded,
                on: true,
            },
            DopeAction::Seek(4.0),
        ] {
            assert_eq!(DopeAction::decode(&action.to_action()), Ok(action));
        }
    }

    #[test]
    fn number_format_writes_units_into_a_reused_buffer() {
        let mut out = String::with_capacity(16);
        NumberFormat::new(1, "dB").write(-6.02, &mut out);
        assert_eq!(out, "-6.0 dB");
        let capacity = out.capacity();
        NumberFormat::new(0, "%")
            .with_scale(100.0)
            .write(0.5, &mut out);
        assert_eq!(out, "50 %");
        assert_eq!(out.capacity(), capacity);
    }
}
