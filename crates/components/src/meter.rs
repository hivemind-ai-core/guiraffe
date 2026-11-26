//! Meter: a level or gain-reduction meter fed with plain `f32` dB values every frame.
//!
//! The host keeps a [`MeterFeed`], writes levels into it (no `cx`, no allocation) and calls
//! [`MeterFeed::publish`], which notifies the meter's bar view alone. The bars read the shared
//! cell while they render and paint in one canvas; the scale, the LUFS readout and the
//! host's own views are not rendered again. Decay and peak hold are pure functions of time
//! ([`ChannelState`]), so they look the same at any frame rate, and the bars keep animating
//! a decay on their own after the host stops feeding.
//!
//! ```ignore
//! let meter = Meter::new(MeterOptions::stereo(), cx);
//! let feed = meter.read(cx).feed();
//! // every audio frame:
//! feed.set_stereo(Level::new(peak_l, rms_l), Level::new(peak_r, rms_r));
//! feed.publish(cx);
//! ```
//!
//! The clip latch resets on a click on the bars, or with the `reset_clip` bus action once the
//! meter is registered ([`Meter::register`]), so shortcuts and scripts can reset it too.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    App, AppContext, Bounds, Context, Element, ElementId, Entity, EntityId, EventEmitter,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton, ParentElement,
    Pixels, Render, SharedString, Style, Styled, WeakEntity, Window, div, point, prelude::*, px,
    relative, size,
};
use guiraffe_core::{
    ColorName, GuiraffeTheme, Variant,
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Source,
    },
    widget_value::Value,
};

/// Decibels of a linear amplitude (`1.0` is 0 dBFS; silence is very low, never `-inf`).
pub fn db(amplitude: f32) -> f32 {
    20.0 * amplitude.abs().max(1e-9).log10()
}

/// One channel's input for a frame, in dBFS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub peak: f32,
    pub rms: f32,
}

impl Level {
    pub const SILENT: Level = Level {
        peak: f32::NEG_INFINITY,
        rms: f32::NEG_INFINITY,
    };

    pub fn new(peak_db: f32, rms_db: f32) -> Self {
        Self {
            peak: peak_db,
            rms: rms_db,
        }
    }

    /// From linear amplitudes.
    pub fn linear(peak: f32, rms: f32) -> Self {
        Self::new(db(peak), db(rms))
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::SILENT
    }
}

/// How the display follows the input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ballistics {
    /// How fast the peak bar falls, in dB per second.
    pub fall_db_per_s: f32,
    /// How long the peak-hold tick stays before it falls.
    pub hold_s: f32,
    /// How fast the tick falls once released, in dB per second.
    pub hold_fall_db_per_s: f32,
    /// A peak at or above this latches the clip indicator.
    pub clip_db: f32,
}

impl Default for Ballistics {
    fn default() -> Self {
        Self {
            fall_db_per_s: 24.0,
            hold_s: 1.5,
            hold_fall_db_per_s: 12.0,
            clip_db: 0.0,
        }
    }
}

/// A channel's displayed state. Values are anchored at a time and evaluated from it, so the
/// display at time `t` depends only on the inputs and `t`, never on how often it was sampled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelState {
    peak: f32,
    peak_at: f64,
    hold: f32,
    hold_at: f64,
    pub rms: f32,
    pub clipped: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            peak: f32::NEG_INFINITY,
            peak_at: 0.0,
            hold: f32::NEG_INFINITY,
            hold_at: 0.0,
            rms: f32::NEG_INFINITY,
            clipped: false,
        }
    }
}

impl ChannelState {
    /// The falling peak at time `now` (seconds), before any new input.
    pub fn peak_at(&self, now: f64, b: &Ballistics) -> f32 {
        let dt = (now - self.peak_at).max(0.0) as f32;
        self.peak - b.fall_db_per_s * dt
    }

    /// The peak-hold tick at time `now`: held for `hold_s`, then falling.
    pub fn hold_at(&self, now: f64, b: &Ballistics) -> f32 {
        let released = self.hold_at + f64::from(b.hold_s);
        let dt = (now - released).max(0.0) as f32;
        (self.hold - b.hold_fall_db_per_s * dt).max(self.peak_at(now, b))
    }

    /// Take `input` at time `now`. Pure: returns the next state.
    pub fn advance(self, input: Level, now: f64, b: &Ballistics) -> Self {
        let mut next = self;
        if !input.peak.is_nan() && input.peak >= self.peak_at(now, b) {
            next.peak = input.peak;
            next.peak_at = now;
        }
        if !input.peak.is_nan() && input.peak >= self.hold_at(now, b) {
            next.hold = input.peak;
            next.hold_at = now;
        }
        next.rms = if input.rms.is_nan() {
            self.rms
        } else {
            input.rms
        };
        next.clipped |= input.peak >= b.clip_db;
        next
    }

    /// Whether anything would still move at `now` without new input.
    pub fn animating(&self, now: f64, floor: f32, b: &Ballistics) -> bool {
        self.peak_at(now, b) > floor || self.hold_at(now, b) > floor
    }

    pub fn reset_clip(&mut self) {
        self.clipped = false;
    }
}

/// Bar direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

/// What the bars show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeterMode {
    /// Peak and RMS levels, filling from the bottom (left).
    Level,
    /// Gain reduction in dB, filling from the top (right) over `0..=range_db`.
    GainReduction { range_db: f32 },
}

/// A colour band of the scale: levels below `below_db` use `color` (a theme color name).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterZone {
    pub below_db: f32,
    pub color: ColorName,
}

/// Everything about a meter except its levels.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterOptions {
    pub orientation: Orientation,
    /// 1 (mono) or 2 (a stereo pair).
    pub channels: usize,
    pub mode: MeterMode,
    /// The scale's bottom and top, in dBFS.
    pub floor_db: f32,
    pub ceiling_db: f32,
    /// Bands from the bottom up; levels above the last band use `clip_color`.
    pub zones: Vec<MeterZone>,
    pub clip_color: ColorName,
    pub ballistics: Ballistics,
    /// dB marks drawn beside the bars (empty: no scale).
    pub ticks: Vec<f32>,
    /// Show a LUFS readout slot.
    pub lufs: bool,
    /// Bar thickness and length in pixels.
    pub thickness: f32,
    pub length: f32,
}

impl Default for MeterOptions {
    fn default() -> Self {
        Self {
            orientation: Orientation::Vertical,
            channels: 1,
            mode: MeterMode::Level,
            floor_db: -60.0,
            ceiling_db: 6.0,
            zones: vec![
                MeterZone {
                    below_db: -6.0,
                    color: ColorName::Green,
                },
                MeterZone {
                    below_db: 0.0,
                    color: ColorName::Yellow,
                },
            ],
            clip_color: ColorName::Red,
            ballistics: Ballistics::default(),
            ticks: vec![0.0, -6.0, -12.0, -24.0, -48.0],
            lufs: false,
            thickness: 10.0,
            length: 160.0,
        }
    }
}

impl MeterOptions {
    pub fn stereo() -> Self {
        Self {
            channels: 2,
            ..Self::default()
        }
    }

    pub fn horizontal(mut self) -> Self {
        self.orientation = Orientation::Horizontal;
        self
    }

    pub fn gain_reduction(mut self, range_db: f32) -> Self {
        self.mode = MeterMode::GainReduction { range_db };
        self
    }

    pub fn with_lufs(mut self) -> Self {
        self.lufs = true;
        self
    }

    /// `db` as a fraction of the scale, clamped to `0..=1`.
    pub fn unit(&self, db: f32) -> f32 {
        if !db.is_finite() {
            return if db > 0.0 { 1.0 } else { 0.0 };
        }
        ((db - self.floor_db) / (self.ceiling_db - self.floor_db)).clamp(0.0, 1.0)
    }

    /// The zone color for a level.
    pub fn zone(&self, db: f32) -> ColorName {
        self.zones
            .iter()
            .find(|z| db < z.below_db)
            .map_or(self.clip_color, |z| z.color)
    }
}

/// The shared cell a host writes and the bars read. Plain `Cell`s: no locking, no
/// allocation per write.
struct MeterCell {
    levels: [Cell<Level>; 2],
    /// Set by every write, cleared by the bars' render: a fed meter needs no animation
    /// frame of its own.
    written: Cell<bool>,
    gain_reduction: Cell<f32>,
    lufs: Cell<Option<f32>>,
}

/// The host's handle: write levels, then [`MeterFeed::publish`] once per frame.
#[derive(Clone)]
pub struct MeterFeed {
    cell: Rc<MeterCell>,
    bars: EntityId,
    lufs: Option<EntityId>,
}

impl MeterFeed {
    /// Channel `channel`'s level (0 or 1).
    pub fn set(&self, channel: usize, level: Level) {
        if let Some(c) = self.cell.levels.get(channel) {
            c.set(level);
            self.cell.written.set(true);
        }
    }

    pub fn set_stereo(&self, left: Level, right: Level) {
        self.set(0, left);
        self.set(1, right);
    }

    /// Gain reduction in dB (positive).
    pub fn set_gain_reduction(&self, db: f32) {
        self.cell.gain_reduction.set(db);
        self.cell.written.set(true);
    }

    /// Integrated loudness for the readout, or `None` to clear it. The readout repaints only
    /// when the value changes.
    pub fn set_lufs(&self, lufs: Option<f32>, cx: &mut App) {
        let rounded = lufs.map(|l| (l * 10.0).round() / 10.0);
        if self.cell.lufs.replace(rounded) != rounded
            && let Some(id) = self.lufs
        {
            cx.notify(id);
        }
    }

    /// Ask the bars, and only the bars, to repaint with the levels written since.
    pub fn publish(&self, cx: &mut App) {
        cx.notify(self.bars);
    }
}

/// Emitted when the clip latch is reset (by a click or the `reset_clip` action).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipReset;

/// The bar view: reads the cell, advances the ballistics and paints. The only part of a
/// meter that renders per frame.
pub(crate) struct MeterBars {
    cell: Rc<MeterCell>,
    options: Rc<MeterOptions>,
    states: [ChannelState; 2],
    /// Time zero for the ballistics.
    epoch: std::time::Instant,
    /// Theme colors resolved for the current theme: zones, clip, gain reduction, track.
    colors: Option<(GuiraffeTheme, Colors)>,
}

#[derive(Clone, Copy)]
struct Colors {
    zones: [(f32, Hsla); 4],
    clip: Hsla,
    gr: Hsla,
    track: Hsla,
}

/// What the bar canvas paints this frame (copied into the paint closure; no allocation).
#[derive(Clone, Copy)]
struct Frame {
    channels: usize,
    peaks: [f32; 2],
    holds: [f32; 2],
    rms: [f32; 2],
    clipped: bool,
    gain_reduction: f32,
}

impl MeterBars {
    /// Seconds since the meter was made, on the executor's clock (deterministic in tests).
    fn now(&self, cx: &App) -> f64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.epoch)
            .as_secs_f64()
    }

    fn reset_clip(&mut self, cx: &mut Context<Self>) -> bool {
        let was = self.states.iter().any(|s| s.clipped);
        for s in &mut self.states {
            s.reset_clip();
        }
        if was {
            cx.notify();
        }
        was
    }

    pub(crate) fn states(&self) -> &[ChannelState; 2] {
        &self.states
    }
}

impl Render for MeterBars {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options = &*self.options;
        let now = self.now(cx);
        let (frame, animating) = advance(&self.cell, options, &mut self.states, now);
        // Keep the decay moving after the host stops publishing.
        if animating && !cx.reduce_motion() {
            window.request_animation_frame();
        }
        let theme = GuiraffeTheme::global(cx);
        let colors = match &self.colors {
            Some((seen, colors)) if seen == theme => *colors,
            _ => {
                let colors = resolve_colors(theme, options);
                self.colors = Some((theme.clone(), colors));
                colors
            }
        };
        BarsElement {
            options: self.options.clone(),
            frame,
            zones: colors.zones,
            clip: colors.clip,
            gr: colors.gr,
            track: colors.track,
        }
    }
}

/// Take a meter's input at `now`: the frame to paint and whether a decay is still moving.
/// Input is taken once: with nothing written since the last render, the peak falls and the
/// RMS stays (a stale reading never latches the clip again).
fn advance(
    cell: &MeterCell,
    options: &MeterOptions,
    states: &mut [ChannelState; 2],
    now: f64,
) -> (Frame, bool) {
    let b = options.ballistics;
    let mut frame = Frame {
        channels: options.channels.clamp(1, 2),
        peaks: [f32::NEG_INFINITY; 2],
        holds: [f32::NEG_INFINITY; 2],
        rms: [f32::NEG_INFINITY; 2],
        clipped: false,
        gain_reduction: cell.gain_reduction.get(),
    };
    let mut animating = false;
    let fresh = cell.written.replace(false);
    for (i, slot) in states.iter_mut().enumerate().take(frame.channels) {
        let input = if fresh {
            cell.levels[i].get()
        } else {
            Level::new(f32::NEG_INFINITY, f32::NAN)
        };
        let state = slot.advance(input, now, &b);
        *slot = state;
        frame.peaks[i] = state.peak_at(now, &b);
        frame.holds[i] = state.hold_at(now, &b);
        frame.rms[i] = state.rms;
        frame.clipped |= state.clipped;
        animating |= state.animating(now, options.floor_db, &b);
    }
    (frame, animating)
}

fn new_cell() -> Rc<MeterCell> {
    Rc::new(MeterCell {
        levels: [Cell::new(Level::SILENT), Cell::new(Level::SILENT)],
        written: Cell::new(false),
        gain_reduction: Cell::new(0.0),
        lufs: Cell::new(None),
    })
}

fn resolve_colors(theme: &GuiraffeTheme, options: &MeterOptions) -> Colors {
    let color = |c: ColorName| theme.variant(Some(c), Variant::Filled).background.to_hsla();
    let mut zones = [(f32::INFINITY, color(options.clip_color)); 4];
    for (slot, z) in zones.iter_mut().zip(options.zones.iter().take(3)) {
        *slot = (z.below_db, color(z.color));
    }
    Colors {
        zones,
        clip: color(options.clip_color),
        gr: color(ColorName::Orange),
        track: theme.surfaces().border.to_hsla(),
    }
}

/// The bars as one element: a plain struct the frame arena holds, so a render allocates
/// nothing on the heap (a `canvas` would box its closures).
struct BarsElement {
    options: Rc<MeterOptions>,
    frame: Frame,
    zones: [(f32, Hsla); 4],
    clip: Hsla,
    gr: Hsla,
    track: Hsla,
}

impl IntoElement for BarsElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for BarsElement {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        paint(
            bounds,
            &self.options,
            self.frame,
            self.zones,
            self.clip,
            self.gr,
            self.track,
            window,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn paint(
    bounds: Bounds<Pixels>,
    options: &MeterOptions,
    frame: Frame,
    zones: [(f32, Hsla); 4],
    clip: Hsla,
    gr: Hsla,
    track: Hsla,
    window: &mut Window,
) {
    let vertical = options.orientation == Orientation::Vertical;
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let length = if vertical { h } else { w };
    let lanes = frame.channels as f32;
    let gap = 2.0;
    let lane = (if vertical { w } else { h } - gap * (lanes - 1.0)).max(1.0) / lanes;
    // A rectangle along the bar: from fraction `a` to `b` of the length, across lane `i`
    // from fraction `c0` to `c1` of its thickness.
    let rect = |i: usize, a: f32, b: f32, c0: f32, c1: f32, color: Hsla, window: &mut Window| {
        if b <= a {
            return;
        }
        let across = i as f32 * (lane + gap);
        let (x, y, rw, rh) = if vertical {
            (
                across + lane * c0,
                h - b * length,
                lane * (c1 - c0),
                (b - a) * length,
            )
        } else {
            (
                a * length,
                across + lane * c0,
                (b - a) * length,
                lane * (c1 - c0),
            )
        };
        window.paint_quad(gpui_kit::fill(
            Bounds::new(
                bounds.origin + point(px(x), px(y)),
                size(px(rw.max(0.0)), px(rh.max(0.0))),
            ),
            color,
        ));
    };
    for i in 0..frame.channels {
        let mut faint = track;
        faint.a *= 0.6;
        rect(i, 0.0, 1.0, 0.0, 1.0, faint, window);
        match options.mode {
            MeterMode::Level => {
                // Peak: translucent zone bands up to the falling peak; RMS: solid, narrower.
                let mut from = 0.0;
                for (below, color) in zones {
                    let to = options.unit(below.min(options.ceiling_db));
                    let mut tint = color;
                    tint.a *= 0.45;
                    rect(
                        i,
                        from,
                        to.min(options.unit(frame.peaks[i])),
                        0.0,
                        1.0,
                        tint,
                        window,
                    );
                    rect(
                        i,
                        from,
                        to.min(options.unit(frame.rms[i])),
                        0.2,
                        0.8,
                        color,
                        window,
                    );
                    from = to;
                    if to >= 1.0 {
                        break;
                    }
                }
                let hold = options.unit(frame.holds[i]);
                if hold > 0.0 {
                    let color = zones
                        .iter()
                        .find(|(below, _)| frame.holds[i] < *below)
                        .map_or(clip, |z| z.1);
                    let tick = 2.0 / length.max(1.0);
                    rect(i, (hold - tick).max(0.0), hold, 0.0, 1.0, color, window);
                }
            }
            MeterMode::GainReduction { range_db } => {
                let fill = (frame.gain_reduction / range_db.max(0.1)).clamp(0.0, 1.0);
                // From the top (vertical) or the right (horizontal).
                rect(i, 1.0 - fill, 1.0, 0.0, 1.0, gr, window);
            }
        }
    }
    if frame.clipped && options.mode == MeterMode::Level {
        let latch = 4.0 / length.max(1.0);
        for i in 0..frame.channels {
            rect(i, 1.0 - latch, 1.0, 0.0, 1.0, clip, window);
        }
    }
}

/// The LUFS readout: renders only when its value changes.
pub(crate) struct LufsLabel {
    cell: Rc<MeterCell>,
    shown: Option<f32>,
    text: SharedString,
}

impl Render for LufsLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.cell.lufs.get();
        if value != self.shown {
            self.shown = value;
            self.text = match value {
                Some(v) => format!("{v:.1} LUFS").into(),
                None => "— LUFS".into(),
            };
        }
        let theme = GuiraffeTheme::global(cx);
        div()
            .text_size(px(10.))
            .text_color(theme.surfaces().dimmed.to_hsla())
            .child(self.text.clone())
    }
}

/// A level or gain-reduction meter (mono or a stereo pair). Renders once; its bars repaint
/// per frame through the [`MeterFeed`].
pub struct Meter {
    options: Rc<MeterOptions>,
    bars: Entity<MeterBars>,
    lufs: Option<Entity<LufsLabel>>,
    feed: MeterFeed,
    id: Option<ControlId>,
}

impl EventEmitter<ClipReset> for Meter {}

impl Meter {
    pub fn new(options: MeterOptions, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let options = Rc::new(options);
            let cell = new_cell();
            let epoch = cx.background_executor().now();
            let bars = cx.new(|_| MeterBars {
                cell: cell.clone(),
                options: options.clone(),
                states: [ChannelState::default(); 2],
                epoch,
                colors: None,
            });
            let lufs = options.lufs.then(|| {
                cx.new(|_| LufsLabel {
                    cell: cell.clone(),
                    shown: Some(f32::NAN),
                    text: SharedString::default(),
                })
            });
            let feed = MeterFeed {
                cell,
                bars: bars.entity_id(),
                lufs: lufs.as_ref().map(|l| l.entity_id()),
            };
            Self {
                options,
                bars,
                lufs,
                feed,
                id: None,
            }
        })
    }

    /// The host's feed: write levels, then publish once per frame.
    pub fn feed(&self) -> MeterFeed {
        self.feed.clone()
    }

    pub fn options(&self) -> &MeterOptions {
        &self.options
    }

    /// Each channel's displayed state as of the last repaint.
    pub fn channel_states(&self, cx: &App) -> [ChannelState; 2] {
        *self.bars.read(cx).states()
    }

    /// Whether the clip latch is set.
    pub fn clipped(&self, cx: &App) -> bool {
        self.bars.read(cx).states.iter().any(|s| s.clipped)
    }

    /// Register on the window's action bus at `path`: `reset_clip` then resets the latch
    /// from shortcuts, scripts or MCP, and a click goes through the bus too.
    pub fn register(view: &Entity<Self>, path: &str, window: &Window, cx: &mut App) -> ControlId {
        let id = ActionBus::register(path, view, "meter", Capabilities::ACTIVATE, window, cx);
        view.update(cx, |m, _| m.id = Some(id));
        id
    }

    /// Reset the clip latch (a user action; emits [`ClipReset`] when it was set).
    pub fn reset_clip(&mut self, cx: &mut Context<Self>) -> bool {
        let reset = self.bars.update(cx, |bars, cx| bars.reset_clip(cx));
        if reset {
            cx.emit(ClipReset);
        }
        reset
    }

    fn route_reset(owner: &WeakEntity<Self>, source: Source, window: &mut Window, cx: &mut App) {
        let Some(meter) = owner.upgrade() else { return };
        match meter.read(cx).id {
            Some(id) => {
                ActionBus::dispatch(id, &reset_clip_action(), source, window, cx).ok();
            }
            None => {
                meter.update(cx, |m, cx| m.reset_clip(cx));
            }
        }
    }
}

/// The erased `reset_clip` action.
pub fn reset_clip_action() -> Action {
    Action {
        name: SharedString::new_static(RESET_CLIP),
        payload: Value::Null,
    }
}

/// The bus action that resets a meter's clip latch.
pub const RESET_CLIP: &str = "reset_clip";

impl Observe for Meter {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        ControlSnapshot {
            checked: Some(self.clipped(cx)),
            ..Default::default()
        }
    }
}

impl Drive for Meter {
    type Action = ();
    type Output = ();
    fn decode(action: &Action) -> Result<(), ActionError> {
        if action.name.as_ref() == RESET_CLIP
            || action.name.as_ref() == guiraffe_core::protocol::names::ACTIVATE
        {
            Ok(())
        } else {
            Err(ActionError::Unsupported)
        }
    }
    fn apply(&mut self, (): (), _: &mut Window, cx: &mut Context<Self>) -> Result<(), ActionError> {
        self.reset_clip(cx);
        Ok(())
    }
}

impl Render for Meter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let o = &*self.options;
        let theme = GuiraffeTheme::global(cx);
        let dim = theme.surfaces().dimmed.to_hsla();
        let vertical = o.orientation == Orientation::Vertical;
        let lanes = o.channels.clamp(1, 2) as f32;
        let thick = o.thickness * lanes + 2.0 * (lanes - 1.0);
        let owner = cx.weak_entity();
        let bars = div()
            .id("meter-bars")
            .debug_selector(|| "meter-bars".into())
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                Meter::route_reset(&owner, Source::Pointer, window, cx);
            })
            .flex_none()
            .when(vertical, |d| d.w(px(thick)).h(px(o.length)))
            .when(!vertical, |d| d.w(px(o.length)).h(px(thick)))
            .child(self.bars.clone());
        // The scale: labels placed along the bar at their dB.
        let scale = (!o.ticks.is_empty()).then(|| {
            div()
                .relative()
                .flex_none()
                .when(vertical, |d| d.w(px(22.)).h(px(o.length)))
                .when(!vertical, |d| d.w(px(o.length)).h(px(12.)))
                .text_size(px(9.))
                .text_color(dim)
                .children(o.ticks.iter().map(|t| {
                    let at = o.unit(*t) * o.length;
                    let label = if *t == 0.0 {
                        "0".to_string()
                    } else {
                        format!("{t:.0}")
                    };
                    div()
                        .absolute()
                        .when(vertical, |d| d.left(px(3.)).top(px(o.length - at - 6.)))
                        .when(!vertical, |d| d.top_0().left(px(at - 6.)))
                        .child(label)
                }))
        });
        div()
            .flex()
            .gap(px(2.))
            .when(vertical, |d| d.flex_col())
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .when(!vertical, |d| d.flex_col())
                    .child(bars)
                    .children(scale),
            )
            .children(self.lufs.clone())
    }
}

/// Many meters in one view: a mixer's worth of meters costs one view's repaint per frame,
/// whatever their number. Each meter has its own [`MeterFeed`]; publishing any of them
/// repaints the bank once. The documented choice for mixers. Meters share one
/// [`MeterOptions`] (without scale labels or LUFS readouts) and sit side by side.
pub struct MeterBank {
    options: Rc<MeterOptions>,
    cells: Vec<Rc<MeterCell>>,
    states: Vec<[ChannelState; 2]>,
    /// Reused every render: no allocation per frame.
    frames: Rc<std::cell::RefCell<Vec<Frame>>>,
    epoch: std::time::Instant,
    colors: Option<(GuiraffeTheme, Colors)>,
    gap: f32,
    id: Option<ControlId>,
}

impl EventEmitter<ClipReset> for MeterBank {}

impl MeterBank {
    /// `count` meters drawn with `options`.
    pub fn new(count: usize, options: MeterOptions, cx: &mut App) -> Entity<Self> {
        let epoch = cx.background_executor().now();
        cx.new(|_| Self {
            options: Rc::new(options),
            cells: (0..count).map(|_| new_cell()).collect(),
            states: vec![[ChannelState::default(); 2]; count],
            frames: Rc::new(std::cell::RefCell::new(Vec::with_capacity(count))),
            epoch,
            colors: None,
            gap: 6.0,
            id: None,
        })
    }

    /// Meter `index`'s feed.
    pub fn feed(&self, index: usize, cx: &Context<Self>) -> Option<MeterFeed> {
        Some(MeterFeed {
            cell: self.cells.get(index)?.clone(),
            bars: cx.entity_id(),
            lufs: None,
        })
    }

    /// Every meter's feed, in order.
    pub fn feeds(view: &Entity<Self>, cx: &App) -> Vec<MeterFeed> {
        let bank = view.read(cx);
        bank.cells
            .iter()
            .map(|cell| MeterFeed {
                cell: cell.clone(),
                bars: view.entity_id(),
                lufs: None,
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Whether meter `index`'s clip latch is set.
    pub fn clipped(&self, index: usize) -> bool {
        self.states
            .get(index)
            .is_some_and(|s| s.iter().any(|c| c.clipped))
    }

    /// Reset meter `index`'s clip latch, or every meter's with `None` (a user action).
    pub fn reset_clip(&mut self, index: Option<usize>, cx: &mut Context<Self>) -> bool {
        let mut reset = false;
        for (i, states) in self.states.iter_mut().enumerate() {
            if index.is_none_or(|n| n == i) {
                for s in states.iter_mut() {
                    reset |= s.clipped;
                    s.reset_clip();
                }
            }
        }
        if reset {
            cx.emit(ClipReset);
            cx.notify();
        }
        reset
    }

    /// Register on the bus: `reset_clip` with a meter index (a number), or `null` for all.
    pub fn register(view: &Entity<Self>, path: &str, window: &Window, cx: &mut App) -> ControlId {
        let id = ActionBus::register(path, view, "meter-bank", Capabilities::ACTIVATE, window, cx);
        view.update(cx, |m, _| m.id = Some(id));
        id
    }

    fn width(&self) -> f32 {
        let lanes = self.options.channels.clamp(1, 2) as f32;
        self.options.thickness * lanes + 2.0 * (lanes - 1.0)
    }
}

impl Observe for MeterBank {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        let clipped: Vec<usize> = (0..self.len()).filter(|i| self.clipped(*i)).collect();
        ControlSnapshot {
            selected: Some(clipped.into()),
            ..Default::default()
        }
    }
}

impl Drive for MeterBank {
    type Action = Option<usize>;
    type Output = ();
    fn decode(action: &Action) -> Result<Option<usize>, ActionError> {
        if action.name.as_ref() != RESET_CLIP {
            return Err(ActionError::Unsupported);
        }
        match &action.payload {
            Value::Null => Ok(None),
            Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 => Ok(Some(*n as usize)),
            _ => Err(ActionError::Invalid("meter index")),
        }
    }
    fn apply(
        &mut self,
        index: Option<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ActionError> {
        self.reset_clip(index, cx);
        Ok(())
    }
}

impl Render for MeterBank {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = cx
            .background_executor()
            .now()
            .saturating_duration_since(self.epoch)
            .as_secs_f64();
        let mut animating = false;
        {
            let mut frames = self.frames.borrow_mut();
            frames.clear();
            for (cell, states) in self.cells.iter().zip(self.states.iter_mut()) {
                let (frame, moving) = advance(cell, &self.options, states, now);
                frames.push(frame);
                animating |= moving;
            }
        }
        if animating && !cx.reduce_motion() {
            window.request_animation_frame();
        }
        let theme = GuiraffeTheme::global(cx);
        let colors = match &self.colors {
            Some((seen, colors)) if seen == theme => *colors,
            _ => {
                let colors = resolve_colors(theme, &self.options);
                self.colors = Some((theme.clone(), colors));
                colors
            }
        };
        BankElement {
            options: self.options.clone(),
            frames: self.frames.clone(),
            colors,
            width: self.width(),
            gap: self.gap,
        }
    }
}

/// All of a bank's meters as one element.
struct BankElement {
    options: Rc<MeterOptions>,
    frames: Rc<std::cell::RefCell<Vec<Frame>>>,
    colors: Colors,
    width: f32,
    gap: f32,
}

impl IntoElement for BankElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for BankElement {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let n = self.frames.borrow().len() as f32;
        let extent = n * self.width + (n - 1.0).max(0.0) * self.gap;
        let style = Style {
            size: if self.options.orientation == Orientation::Vertical {
                size(px(extent).into(), px(self.options.length).into())
            } else {
                size(px(self.options.length).into(), px(extent).into())
            },
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let vertical = self.options.orientation == Orientation::Vertical;
        let step = self.width + self.gap;
        for (i, frame) in self.frames.borrow().iter().enumerate() {
            let offset = i as f32 * step;
            let origin = if vertical {
                bounds.origin + point(px(offset), px(0.))
            } else {
                bounds.origin + point(px(0.), px(offset))
            };
            let extent = if vertical {
                size(px(self.width), bounds.size.height)
            } else {
                size(bounds.size.width, px(self.width))
            };
            let c = self.colors;
            paint(
                Bounds::new(origin, extent),
                &self.options,
                *frame,
                c.zones,
                c.clip,
                c.gr,
                c.track,
                window,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b() -> Ballistics {
        Ballistics::default()
    }

    #[test]
    fn the_peak_falls_at_its_rate_and_rises_at_once() {
        let s = ChannelState::default().advance(Level::new(-6.0, -12.0), 0.0, &b());
        assert_eq!(s.peak_at(0.0, &b()), -6.0);
        assert_eq!(s.peak_at(0.5, &b()), -18.0, "24 dB/s");
        let s = s.advance(Level::new(-20.0, -30.0), 0.5, &b());
        assert_eq!(
            s.peak_at(0.5, &b()),
            -18.0,
            "a lower input doesn't pull it down"
        );
        let s = s.advance(Level::new(-3.0, -10.0), 0.6, &b());
        assert_eq!(s.peak_at(0.6, &b()), -3.0);
        assert_eq!(s.rms, -10.0, "rms follows the input");
    }

    #[test]
    fn hold_stays_then_falls() {
        let s = ChannelState::default().advance(Level::new(-6.0, -12.0), 0.0, &b());
        assert_eq!(s.hold_at(1.0, &b()), -6.0, "held for 1.5 s");
        assert_eq!(s.hold_at(2.5, &b()), -18.0, "then 12 dB/s");
        let s = s.advance(Level::new(-30.0, -40.0), 2.0, &b());
        assert_eq!(
            s.hold_at(2.0, &b()),
            -12.0,
            "a lower input doesn't reset the hold"
        );
    }

    #[test]
    fn decay_is_independent_of_frame_rate() {
        // A burst at 0..0.2 s, then silence, sampled at 30, 60, 144 and 240 Hz.
        let input = |t: f64| {
            if t < 0.2 {
                Level::new(-3.0 - t as f32 * 10.0, -9.0)
            } else {
                Level::SILENT
            }
        };
        let run = |hz: f64| {
            let mut s = ChannelState::default();
            let mut t = 0.0;
            let mut probes = Vec::new();
            for probe in [0.5, 1.0, 2.0, 3.0] {
                while t < probe {
                    s = s.advance(input(t), t, &b());
                    t += 1.0 / hz;
                }
                probes.push((s.peak_at(probe, &b()), s.hold_at(probe, &b())));
            }
            probes
        };
        let reference = run(240.0);
        for hz in [30.0, 60.0, 144.0] {
            for ((p, h), (rp, rh)) in run(hz).iter().zip(&reference) {
                // The burst's top is sampled at slightly different times per rate; after
                // it, the curves are the same lines.
                assert!(
                    (p - rp).abs() < 0.5 && (h - rh).abs() < 0.5,
                    "{hz} Hz: {p} {h} vs {rp} {rh}"
                );
            }
        }
    }

    #[test]
    fn the_clip_latch_holds_until_reset() {
        let mut s = ChannelState::default().advance(Level::new(0.5, -3.0), 0.0, &b());
        assert!(s.clipped);
        s = s.advance(Level::new(-30.0, -40.0), 5.0, &b());
        assert!(s.clipped, "latched");
        s.reset_clip();
        assert!(!s.clipped);
    }

    #[test]
    fn zones_and_units() {
        let o = MeterOptions::default();
        assert_eq!(o.zone(-20.0), ColorName::Green);
        assert_eq!(o.zone(-3.0), ColorName::Yellow);
        assert_eq!(o.zone(0.0), ColorName::Red, "at clip");
        assert_eq!(o.unit(-60.0), 0.0);
        assert_eq!(o.unit(6.0), 1.0);
        assert_eq!(o.unit(f32::NEG_INFINITY), 0.0);
        assert!((o.unit(-27.0) - 0.5).abs() < 1e-6);
        assert!((db(1.0)).abs() < 1e-6);
        assert!((db(0.5) + 6.0206).abs() < 1e-3);
    }
}
