//! `KnobControl` and `NumberFieldControl`: compact parameter controls over a
//! [`ValueMapping`], shared with `SliderControl`.
//!
//! Both are protocol control entities like the rest of this module. Actions: `set_value`
//! (num), `increment`, `decrement`. A drag previews locally and commits one `set_value` in a
//! `dispatch_batch` on release, so a drag is one undo step. Host setters never emit, and
//! every event carries the input's [`Source`].

use super::{Identified, Identity, action, build, enabled, mount, num_payload, route};
use crate::mapping::{ValueMap, ValueMapping};
use crate::style::{hsla, surfaces};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, DispatchPhase, ElementId, Entity,
    EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels,
    Render, Role, ScrollDelta, ScrollEvent, SharedString, StatefulInteractiveElement, Styled,
    Subscription, WeakEntity, Window, canvas, div, point, px,
};
use guiraffe_core::GuiraffeTheme;
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, Capabilities, ControlSnapshot, Drive, Observe, Outcome, Source,
    names,
};
use guiraffe_core::widget_value::Value;
use std::rc::Rc;

/// Travel per pixel of vertical drag: 200 px covers the whole range.
const TRAVEL_PER_PX: f64 = 1.0 / 200.0;
/// Fine drag (Shift) moves a tenth as far.
const FINE: f64 = 0.1;
/// A bipolar drag within this much travel of the centre snaps to it.
const DETENT: f64 = 0.02;
/// The knob's sweep: 270° clockwise from the bottom left, leaving the gap at the bottom.
const SWEEP: f32 = 0.75;
const START: f32 = 0.625;

/// The travel a drag reaches from `start` after `dy` pixels (up is positive), with the fine
/// modifier and the bipolar centre detent.
pub fn drag_travel(start: f64, dy: f64, fine: bool, detent: Option<f64>) -> f64 {
    let scale = if fine {
        TRAVEL_PER_PX * FINE
    } else {
        TRAVEL_PER_PX
    };
    let next = (start + dy * scale).clamp(0.0, 1.0);
    match detent {
        Some(centre) if !fine && (next - centre).abs() < DETENT => centre,
        _ => next,
    }
}

/// The angle, in turns from the top, that travel `t` points at on the knob.
pub fn knob_angle(t: f64) -> f32 {
    START + SWEEP * t.clamp(0.0, 1.0) as f32
}

/// The `(from, to)` travel the value arc covers: from the start, or from the centre when
/// bipolar.
pub fn value_span(travel: f64, centre: Option<f64>) -> (f64, f64) {
    match centre {
        Some(c) => (c.min(travel), c.max(travel)),
        None => (0.0, travel),
    }
}

/// One modulation route shown on a knob: the span it sweeps in normalised travel (0–1), in
/// the source's colour. Arcs draw as thin rings outside the value arc, stacked outwards in
/// order (index 0 innermost); events name an arc by that index.
#[derive(Debug, Clone, PartialEq)]
pub struct ModArc {
    pub from: f64,
    pub to: f64,
    /// A theme-resolved colour (the source's identity colour).
    pub color: Hsla,
    /// The host's route id, for its own lookup (`KnobControl::modulation_arcs()[arc].id`).
    pub id: Option<ElementId>,
    /// Shown while the pointer is over the arc (`Envelope 2 → depth 0.40`).
    pub tooltip: Option<SharedString>,
}

impl ModArc {
    pub fn new(from: f64, to: f64, color: Hsla) -> Self {
        Self {
            from,
            to,
            color,
            id: None,
            tooltip: None,
        }
    }
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
    pub fn tooltip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tip.into());
        self
    }
    /// `(lo, hi)`, clamped to travel and ordered.
    pub fn span(&self) -> (f64, f64) {
        let (a, b) = (self.from.clamp(0.0, 1.0), self.to.clamp(0.0, 1.0));
        (a.min(b), a.max(b))
    }
}

/// Ring layout of a knob dial of `diameter` px: the value arc, then the modulation rings
/// stacked outwards. The first rings fit in the dial's margin; more paint past its box
/// (without changing layout).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialGeometry {
    pub diameter: f32,
}

impl DialGeometry {
    pub fn new(diameter: f32) -> Self {
        Self { diameter }
    }
    /// Value arc and track stroke width.
    pub fn width(&self) -> f32 {
        (self.diameter / 10.0).max(2.0)
    }
    /// Radius of the value arc's centreline.
    pub fn value_radius(&self) -> f32 {
        self.diameter / 2.0 - self.width() * 1.5 - 1.0
    }
    /// Modulation ring stroke width.
    pub fn ring_width(&self) -> f32 {
        (self.width() / 3.0).max(1.5)
    }
    /// Centre-to-centre distance between rings.
    pub fn ring_pitch(&self) -> f32 {
        self.ring_width() + 1.0
    }
    /// Radius of ring `i`'s centreline.
    pub fn ring_radius(&self, i: usize) -> f32 {
        self.value_radius()
            + self.width() / 2.0
            + 1.0
            + self.ring_width() / 2.0
            + i as f32 * self.ring_pitch()
    }
    /// How far from a ring's centreline a press still hits it: its own lane, so rings never
    /// steal each other's presses. The outermost ring also takes [`OUTER_SLACK`] px outwards.
    pub fn ring_hit(&self) -> f32 {
        self.ring_pitch() / 2.0
    }
}

/// The travel (0–1) at a point `(dx, dy)` px from the dial centre (y down), or `None` in the
/// gap at the bottom.
pub fn travel_at(dx: f32, dy: f32) -> Option<f64> {
    let turns = dx.atan2(-dy) / std::f32::consts::TAU;
    let t = f64::from((turns - START).rem_euclid(1.0) / SWEEP);
    (t <= 1.0).then_some(t)
}

/// Extra px outside the outermost ring that still hit it.
pub const OUTER_SLACK: f32 = 2.0;

/// Travel slack when hit-testing an arc's span, so zero-depth arcs stay reachable.
const SPAN_SLACK: f64 = 0.03;

/// The modulation arc under `(dx, dy)` px from the dial centre: the nearest ring within
/// [`DialGeometry::ring_hit`] whose span covers the pointer's angle.
pub fn mod_arc_at(geometry: DialGeometry, arcs: &[ModArc], dx: f32, dy: f32) -> Option<usize> {
    let t = travel_at(dx, dy)?;
    let r = dx.hypot(dy);
    let hit = geometry.ring_hit();
    arcs.iter()
        .enumerate()
        .filter_map(|(i, arc)| {
            let (lo, hi) = arc.span();
            let d = r - geometry.ring_radius(i);
            let reach = if i + 1 == arcs.len() && d > 0.0 {
                hit + OUTER_SLACK
            } else {
                hit
            };
            let off = d.abs();
            (off <= reach && t >= lo - SPAN_SLACK && t <= hi + SPAN_SLACK).then_some((i, off))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// The depth change, in travel, of an arc drag moving `dy` px (up is positive).
pub fn mod_depth_delta(dy: f64, fine: bool) -> f64 {
    dy * if fine {
        TRAVEL_PER_PX * FINE
    } else {
        TRAVEL_PER_PX
    }
}

/// Emitted when a knob or number field commits a value, or (knob) a modulation arc is
/// dragged or right-clicked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamEvent {
    Changed(f64, Source),
    /// Alt- or Ctrl-drag on arc `arc`: change its depth by `delta_normal` travel since the
    /// previous `ModDepth` (Shift for fine). The knob's value drag does not start.
    ModDepth {
        arc: usize,
        delta_normal: f64,
        source: Source,
    },
    /// The arc drag ended (one undo step for the host).
    ModDepthEnd {
        arc: usize,
        source: Source,
    },
    /// Right-click on arc `arc` (to remove the route, say), at a window position.
    ModContext {
        arc: usize,
        position: gpui_kit::Point<Pixels>,
        source: Source,
    },
}

/// Typed actions of a knob or number field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamAction {
    Set(f64),
    Increment,
    Decrement,
}

fn decode_param(action: &Action) -> Result<ParamAction, ActionError> {
    match action.name.as_ref() {
        names::SET_VALUE => num_payload(action).map(ParamAction::Set),
        names::INCREMENT => Ok(ParamAction::Increment),
        names::DECREMENT => Ok(ParamAction::Decrement),
        _ => Err(ActionError::Unsupported),
    }
}

/// What both parameter controls hold: the mapping, the committed value and a drag preview.
struct Param {
    identity: Identity,
    map: Rc<dyn ValueMapping>,
    value: f64,
    label: SharedString,
    disabled: bool,
    /// The value shown while a drag is under way, committed on release.
    preview: Option<f64>,
    drag: Option<Drag>,
    focus: FocusHandle,
}

#[derive(Clone, Copy)]
struct Drag {
    y: f32,
    x: f32,
    travel: f64,
}

impl Param {
    fn shown(&self) -> f64 {
        self.preview.unwrap_or(self.value)
    }

    fn detent(&self) -> Option<f64> {
        self.map
            .bipolar()
            .then(|| self.map.to_normal(centre(&*self.map)))
    }

    fn apply(&mut self, action: ParamAction) -> Result<Option<f64>, ActionError> {
        enabled(self.disabled)?;
        let next = match action {
            ParamAction::Set(value) => self.map.constrain(value),
            ParamAction::Increment => self.map.step_from(self.value, true, false),
            ParamAction::Decrement => self.map.step_from(self.value, false, false),
        };
        self.preview = None;
        if next == self.value {
            return Ok(None);
        }
        self.value = next;
        Ok(Some(next))
    }

    fn set(&mut self, value: f64) -> bool {
        let value = self.map.constrain(value);
        if value == self.value {
            return false;
        }
        self.value = value;
        true
    }

    fn snapshot(&self) -> ControlSnapshot {
        ControlSnapshot {
            value: Some(Value::Num(self.value)),
            label: Some(self.accessible_label().into()),
            disabled: Some(self.disabled),
            ..ControlSnapshot::default()
        }
    }

    /// `Cutoff: 1.25 kHz`.
    fn accessible_label(&self) -> String {
        self.label_with(&self.map.format(self.shown()))
    }

    fn label_with(&self, value: &str) -> String {
        if self.label.is_empty() {
            value.to_owned()
        } else {
            format!("{}: {value}", self.label)
        }
    }
}

/// The bipolar centre of a mapping: 0 when the range spans it, else the middle of travel.
fn centre(map: &dyn ValueMapping) -> f64 {
    if map.min() < 0.0 && map.max() > 0.0 {
        0.0
    } else {
        map.value_at(0.5)
    }
}

/// Route a committed edit as a bus client would send it: a drag commit as a one-action
/// `dispatch_batch` (one undo step), anything else as a plain dispatch.
fn commit<T: Observe + Drive + Identified>(
    this: &WeakEntity<T>,
    value: f64,
    source: Source,
    batch: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let action = action(names::SET_VALUE, Value::Num(value));
    let Some(entity) = this.upgrade() else { return };
    match entity.read(cx).identity().id {
        Some(id) if batch => {
            let _ =
                ActionBus::dispatch_batch(id, std::slice::from_ref(&action), source, window, cx);
        }
        _ => {
            let _ = route(this, action, source, window, cx);
        }
    }
}

fn send<T: Observe + Drive + Identified>(
    this: &WeakEntity<T>,
    name: &'static str,
    payload: Value,
    source: Source,
    window: &mut Window,
    cx: &mut App,
) {
    let _ = route(this, action(name, payload), source, window, cx);
}

/// Shared builder state of both parameter controls.
struct ParamBuilder {
    map: Rc<dyn ValueMapping>,
    value: Option<f64>,
    label: SharedString,
    disabled: bool,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl Default for ParamBuilder {
    fn default() -> Self {
        Self {
            map: Rc::new(ValueMap::default()),
            value: None,
            label: SharedString::default(),
            disabled: false,
            path: None,
            name: None,
        }
    }
}

macro_rules! param_builder {
    ($builder:ident) => {
        impl $builder {
            /// The value mapping: range, curve, step, default, units (shared with sliders).
            pub fn mapping(mut self, map: impl ValueMapping) -> Self {
                self.param.map = Rc::new(map);
                self
            }
            pub fn value(mut self, value: f64) -> Self {
                self.param.value = Some(value);
                self
            }
            /// Accessible name, read with the formatted value (`Cutoff: 1.25 kHz`).
            pub fn label(mut self, label: impl Into<SharedString>) -> Self {
                self.param.label = label.into();
                self
            }
            pub fn disabled(mut self, disabled: bool) -> Self {
                self.param.disabled = disabled;
                self
            }
            /// Register on the window's action bus under this inspect path.
            pub fn path(mut self, path: impl Into<SharedString>) -> Self {
                self.param.path = Some(path.into());
                self
            }
            /// Element id. Without it, the path (if any) names the element; with both, this
            /// wins.
            pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
                self.param.name = Some(name.into());
                self
            }
        }
    };
}

fn make_param(builder: ParamBuilder, identity: Identity, cx: &mut App) -> Param {
    let map = builder.map;
    let value = map.constrain(builder.value.unwrap_or_else(|| map.default_value()));
    Param {
        identity,
        map,
        value,
        label: builder.label,
        disabled: builder.disabled,
        preview: None,
        drag: None,
        focus: cx.focus_handle(),
    }
}

// --- Knob -----------------------------------------------------------------------

/// Builder for [`KnobControl`].
#[derive(Default)]
pub struct KnobBuilder {
    param: ParamBuilder,
    diameter: Option<f32>,
    modulation: Option<(f64, f64)>,
    arcs: Vec<ModArc>,
    ink: Option<Hsla>,
    compact: bool,
}

param_builder!(KnobBuilder);

impl KnobBuilder {
    /// The dial only, for dense rows: the value shows in the tooltip and above the dial while
    /// dragging. The accessible value is unchanged.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
    /// Outer diameter in pixels (default 40).
    pub fn diameter(mut self, px: f32) -> Self {
        self.diameter = px.is_finite().then(|| px.clamp(16.0, 400.0));
        self
    }
    /// A modulation range drawn as an outer arc, in value units.
    pub fn modulation(mut self, range: Option<(f64, f64)>) -> Self {
        self.modulation = range;
        self
    }
    /// Per-route modulation arcs (see [`ModArc`]).
    pub fn modulation_arcs(mut self, arcs: Vec<ModArc>) -> Self {
        self.arcs = arcs;
        self
    }
    /// A theme-resolved colour for the value arc (the theme accent when `None`).
    pub fn value_ink(mut self, ink: Option<Hsla>) -> Self {
        self.ink = ink;
        self
    }
    /// Build the control, registering its path (if any) in `window`.
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<KnobControl> {
        self.build_with(Some(window), cx)
    }
    /// Build the control without a window. A path is reserved now and registers in the
    /// window the control first renders in, following it to another window; until then,
    /// the bus answers queries for it with `ActionError::NotMounted`.
    pub fn build_later(self, cx: &mut App) -> Entity<KnobControl> {
        self.build_with(None, cx)
    }
    fn build_with(self, window: Option<&mut Window>, cx: &mut App) -> Entity<KnobControl> {
        build(
            "knob",
            Capabilities::SET_VALUE | Capabilities::INCREMENT | Capabilities::DECREMENT,
            self.param.path.clone(),
            self.param.name.clone(),
            window,
            cx,
            |identity, _, cx| KnobControl {
                param: make_param(self.param, identity, cx),
                diameter: self.diameter.unwrap_or(40.0),
                modulation: self.modulation,
                arcs: (!self.arcs.is_empty()).then(|| Rc::from(self.arcs)),
                ink: self.ink,
                hovered: None,
                mod_drag: None,
                live: None,
                compact: self.compact,
            },
        )
    }
}

/// A rotary parameter control over a [`ValueMapping`].
///
/// Drag vertically (Shift for fine), scroll, or use the arrow keys (Shift for fine,
/// Home/End for the ends). Double-click resets to the mapping's default. A bipolar mapping
/// draws its value from the centre and detents there. A modulation range draws as an outer
/// arc. The element exposes a slider role with the formatted value (`Cutoff: 1.25 kHz`).
pub struct KnobControl {
    param: Param,
    diameter: f32,
    modulation: Option<(f64, f64)>,
    arcs: Option<Rc<[ModArc]>>,
    ink: Option<Hsla>,
    hovered: Option<usize>,
    mod_drag: Option<ModDrag>,
    live: Option<KnobLiveFeed>,
    compact: bool,
}

#[derive(Clone, Copy)]
struct ModDrag {
    arc: usize,
    y: f32,
}

/// The host's handle for a knob's live-value marker: set the effective value and publish,
/// and only the marker repaints (no knob or parent render). Get it with
/// [`KnobControl::live_feed`].
#[derive(Clone)]
pub struct KnobLiveFeed {
    travel: Rc<std::cell::Cell<Option<f64>>>,
    map: Rc<dyn ValueMapping>,
    view: gpui_kit::EntityId,
    marker: Entity<KnobLive>,
}

impl KnobLiveFeed {
    /// Set the live value in parameter units (`None` hides the marker).
    pub fn set(&self, value: Option<f64>) {
        self.travel.set(
            value
                .filter(|v| v.is_finite())
                .map(|v| self.map.to_normal(v)),
        );
    }
    /// The marker's travel now.
    pub fn travel(&self) -> Option<f64> {
        self.travel.get()
    }
    /// Repaint the marker, and only the marker.
    pub fn publish(&self, cx: &mut App) {
        cx.notify(self.view);
    }
}

/// The live marker view: one dot on the value arc's radius.
struct KnobLive {
    travel: Rc<std::cell::Cell<Option<f64>>>,
    diameter: f32,
}

impl Render for KnobLive {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let travel = self.travel.get();
        let g = DialGeometry::new(self.diameter);
        let s = surfaces(cx);
        let (dot, halo) = (hsla(s.text), hsla(s.body));
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let Some(t) = travel else { return };
                let a = knob_angle(t) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let c = bounds.center();
                let r = g.value_radius();
                let at = point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
                let size = (g.width() * 0.6).max(2.0);
                for (s, color) in [(size + 1.0, halo), (size, dot)] {
                    window.paint_quad(
                        gpui_kit::fill(
                            Bounds::centered_at(at, gpui_kit::size(px(s * 2.0), px(s * 2.0))),
                            color,
                        )
                        .corner_radii(px(s)),
                    );
                }
            },
        )
        .size(px(self.diameter))
    }
}

impl Identified for KnobControl {
    fn identity_mut(&mut self) -> &mut Identity {
        &mut self.param.identity
    }
    fn identity(&self) -> &Identity {
        &self.param.identity
    }
}

impl KnobControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> KnobBuilder {
        KnobBuilder::default()
    }
    pub fn value(&self) -> f64 {
        self.param.value
    }
    /// The value as the mapping formats it.
    pub fn formatted(&self) -> String {
        self.param.map.format(self.param.shown())
    }
    pub fn mapping(&self) -> &dyn ValueMapping {
        &*self.param.map
    }
    pub fn control_id(&self) -> Option<guiraffe_core::protocol::ControlId> {
        self.param.identity.id
    }
    pub fn element_id(&self) -> ElementId {
        self.param.identity.element()
    }
    /// Host write: the value, without an event. Needs no window.
    pub fn set_value(&mut self, value: f64, cx: &mut Context<Self>) {
        if self.param.set(value) {
            cx.notify();
        }
    }
    /// Whether the knob shows the dial only.
    pub fn is_compact(&self) -> bool {
        self.compact
    }
    /// Whether a drag is under way (a compact knob shows its value then).
    pub fn dragging(&self) -> bool {
        self.param.drag.is_some()
    }
    /// Host write: the modulation range, in value units.
    pub fn set_modulation(&mut self, range: Option<(f64, f64)>, cx: &mut Context<Self>) {
        if range != self.modulation {
            self.modulation = range;
            cx.notify();
        }
    }
    /// Host write: the per-route modulation arcs. Needs no window.
    pub fn set_modulation_arcs(&mut self, arcs: Vec<ModArc>, cx: &mut Context<Self>) {
        if self.arcs.as_deref().unwrap_or(&[]) == arcs.as_slice() {
            return;
        }
        let len = arcs.len();
        self.arcs = (!arcs.is_empty()).then(|| Rc::from(arcs));
        self.hovered = self.hovered.filter(|&i| i < len);
        if self.mod_drag.is_some_and(|d| d.arc >= len) {
            self.mod_drag = None;
        }
        cx.notify();
    }
    /// The modulation arcs shown now.
    pub fn modulation_arcs(&self) -> &[ModArc] {
        self.arcs.as_deref().unwrap_or(&[])
    }
    /// The arc under the pointer, if any.
    pub fn hovered_arc(&self) -> Option<usize> {
        self.hovered
    }
    /// Host write: the value arc's colour (`None`: the theme accent). Needs no window.
    pub fn set_value_ink(&mut self, ink: Option<Hsla>, cx: &mut Context<Self>) {
        if ink != self.ink {
            self.ink = ink;
            cx.notify();
        }
    }
    /// The live-value marker's feed, made on first use. Writes through it repaint only the
    /// marker. Needs no window.
    pub fn live_feed(&mut self, cx: &mut Context<Self>) -> KnobLiveFeed {
        if let Some(feed) = &self.live {
            return feed.clone();
        }
        let travel = Rc::new(std::cell::Cell::new(None));
        let diameter = self.diameter;
        let marker = cx.new(|_| KnobLive {
            travel: travel.clone(),
            diameter,
        });
        let feed = KnobLiveFeed {
            travel,
            map: self.param.map.clone(),
            view: marker.entity_id(),
            marker,
        };
        self.live = Some(feed.clone());
        cx.notify();
        feed
    }
    /// Host write: the live (post-modulation) value in parameter units, `None` to hide the
    /// marker. Repaints only the marker; for per-frame telemetry keep the
    /// [`KnobControl::live_feed`] and call `set` + `publish`. Needs no window.
    pub fn set_live(&mut self, value: Option<f64>, cx: &mut Context<Self>) {
        if self.live.is_none() && value.is_none() {
            return;
        }
        let feed = self.live_feed(cx);
        feed.set(value);
        feed.publish(cx);
    }
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if disabled != self.param.disabled {
            self.param.disabled = disabled;
            cx.notify();
        }
    }
    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        let label = label.into();
        if label != self.param.label {
            self.param.label = label;
            cx.notify();
        }
    }
}

impl Focusable for KnobControl {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.param.focus.clone()
    }
}

impl EventEmitter<ParamEvent> for KnobControl {}

impl Observe for KnobControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        self.param.snapshot()
    }
}

impl Drive for KnobControl {
    type Action = ParamAction;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<ParamAction, ActionError> {
        decode_param(action)
    }
    fn apply(
        &mut self,
        action: ParamAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        if let Some(next) = self.param.apply(action)? {
            cx.emit(ParamEvent::Changed(next, self.param.identity.source(cx)));
        }
        cx.notify();
        Ok(Outcome(Value::Num(self.param.value)))
    }
}

/// Arc colours of a knob, from the theme.
#[derive(Clone, Copy)]
struct KnobColors {
    track: Hsla,
    value: Hsla,
    modulation: Hsla,
    pointer: Hsla,
}

fn arc(
    bounds: Bounds<Pixels>,
    inset: f32,
    width: f32,
    from: f64,
    to: f64,
    color: Hsla,
    window: &mut Window,
) {
    if to - from <= 1e-4 {
        return;
    }
    let c = bounds.center();
    let r = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0 - inset - width / 2.0;
    if r <= 0.0 {
        return;
    }
    let at = |t: f64| {
        let a = knob_angle(t) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
    };
    // Arc commands stay under 180° each: split into thirds of at most 90°.
    let mut b = PathBuilder::stroke(px(width));
    b.move_to(at(from));
    for i in 1..=3 {
        let t = from + (to - from) * f64::from(i) / 3.0;
        b.arc_to(point(px(r), px(r)), px(0.0), false, true, at(t));
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Paint the modulation rings and listen for arc presses, drags and hovers. The listeners
/// hit-test the rings' own geometry, since outer rings paint past the dial's box.
fn paint_rings(bounds: Bounds<Pixels>, g: DialGeometry, rings: &Rings, window: &mut Window) {
    let half = g.diameter / 2.0;
    for (i, a) in rings.arcs.iter().enumerate() {
        let (mut lo, mut hi) = a.span();
        if hi - lo < 0.01 {
            // A zero-depth route still shows as a tick.
            let mid = (lo + hi) / 2.0;
            (lo, hi) = ((mid - 0.005).max(0.0), (mid + 0.005).min(1.0));
        }
        let w = if rings.hovered == Some(i) {
            g.ring_width() + 1.0
        } else {
            g.ring_width()
        };
        let inset = half - g.ring_radius(i) - w / 2.0;
        arc(bounds, inset, w, lo, hi, a.color, window);
    }
    let centre = bounds.center();
    let offset =
        move |p: gpui_kit::Point<Pixels>| (f32::from(p.x - centre.x), f32::from(p.y - centre.y));
    let (arcs, knob) = (rings.arcs.clone(), rings.knob.clone());
    window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        let (dx, dy) = offset(e.position);
        let Some(i) = mod_arc_at(g, &arcs, dx, dy) else {
            return;
        };
        let Some(entity) = knob.upgrade() else { return };
        let handled = entity.update(cx, |k, cx| {
            if k.param.disabled {
                return false;
            }
            match e.button {
                MouseButton::Right => {
                    cx.emit(ParamEvent::ModContext {
                        arc: i,
                        position: e.position,
                        source: Source::Pointer,
                    });
                    true
                }
                MouseButton::Left if e.modifiers.alt || e.modifiers.control => {
                    k.mod_drag = Some(ModDrag {
                        arc: i,
                        y: f32::from(e.position.y),
                    });
                    cx.notify();
                    true
                }
                _ => false,
            }
        });
        if handled {
            window.prevent_default();
            cx.stop_propagation();
        }
    });
    let (arcs, knob) = (rings.arcs.clone(), rings.knob.clone());
    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        let Some(entity) = knob.upgrade() else { return };
        entity.update(cx, |k, cx| {
            if let Some(drag) = k.mod_drag {
                let y = f32::from(e.position.y);
                let delta = mod_depth_delta(f64::from(drag.y - y), e.modifiers.shift);
                if delta != 0.0 {
                    k.mod_drag = Some(ModDrag { y, ..drag });
                    cx.emit(ParamEvent::ModDepth {
                        arc: drag.arc,
                        delta_normal: delta,
                        source: Source::Pointer,
                    });
                }
                return;
            }
            let (dx, dy) = offset(e.position);
            let hovered = mod_arc_at(g, &arcs, dx, dy);
            if hovered != k.hovered {
                k.hovered = hovered;
                cx.notify();
            }
        });
    });
    let knob = rings.knob.clone();
    window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
        if phase != DispatchPhase::Bubble || e.button != MouseButton::Left {
            return;
        }
        let Some(entity) = knob.upgrade() else { return };
        entity.update(cx, |k, cx| {
            if let Some(drag) = k.mod_drag.take() {
                cx.emit(ParamEvent::ModDepthEnd {
                    arc: drag.arc,
                    source: Source::Pointer,
                });
                cx.notify();
            }
        });
    });
}

fn pointer(bounds: Bounds<Pixels>, travel: f64, width: f32, color: Hsla, window: &mut Window) {
    let c = bounds.center();
    let r = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0;
    let a = knob_angle(travel) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let mut b = PathBuilder::stroke(px(width));
    b.move_to(point(
        c.x + px(r * 0.25 * a.cos()),
        c.y + px(r * 0.25 * a.sin()),
    ));
    b.line_to(point(
        c.x + px(r * 0.7 * a.cos()),
        c.y + px(r * 0.7 * a.sin()),
    ));
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// The painted dial of a knob: track, value arc, modulation arc and pointer. Also used by
/// the unbound-cost gate as the plain element of the same shape.
pub fn knob_dial(
    travel: f64,
    centre: Option<f64>,
    modulation: Option<(f64, f64)>,
    diameter: f32,
    cx: &App,
) -> impl IntoElement + use<> {
    dial(travel, centre, modulation, diameter, None, None, cx)
}

/// Modulation rings and their pointer handling, painted with the dial.
struct Rings {
    arcs: Rc<[ModArc]>,
    hovered: Option<usize>,
    knob: WeakEntity<KnobControl>,
}

fn dial(
    travel: f64,
    centre: Option<f64>,
    modulation: Option<(f64, f64)>,
    diameter: f32,
    ink: Option<Hsla>,
    rings: Option<Rings>,
    cx: &App,
) -> impl IntoElement + use<> {
    let s = surfaces(cx);
    let primary = GuiraffeTheme::global(cx).primary_color().to_hsla();
    let colors = KnobColors {
        track: hsla(s.track),
        value: ink.unwrap_or(primary),
        modulation: primary.opacity(0.45),
        pointer: hsla(s.text),
    };
    let g = DialGeometry::new(diameter);
    let width = g.width();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            arc(bounds, width + 1.0, width, 0.0, 1.0, colors.track, window);
            let (from, to) = value_span(travel, centre);
            arc(bounds, width + 1.0, width, from, to, colors.value, window);
            if let Some(rings) = &rings {
                paint_rings(bounds, g, rings, window);
            }
            if let Some((lo, hi)) = modulation {
                arc(
                    bounds,
                    0.0,
                    1.0_f32.max(width / 3.0),
                    lo.min(hi),
                    lo.max(hi),
                    colors.modulation,
                    window,
                );
            }
            pointer(
                bounds,
                travel,
                (width * 0.6).max(1.5),
                colors.pointer,
                window,
            );
        },
    )
    .size(px(diameter))
}

impl Render for KnobControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        mount(self, window, cx);
        let p = &self.param;
        let map = p.map.clone();
        let travel = map.to_normal(p.shown());
        let detent = p.detent();
        let modulation = self
            .modulation
            .map(|(lo, hi)| (map.to_normal(lo), map.to_normal(hi)));
        let this = cx.weak_entity();
        let handle = window.window_handle();
        let s = surfaces(cx);
        let rings = self.arcs.clone().map(|arcs| Rings {
            arcs,
            hovered: self.hovered,
            knob: this.clone(),
        });
        let dial = dial(
            travel,
            detent,
            modulation,
            self.diameter,
            self.ink,
            rings,
            cx,
        );
        let live = self.live.as_ref().map(|l| l.marker.clone());
        let arc_tip = self
            .hovered
            .and_then(|i| self.arcs.as_ref()?.get(i)?.tooltip.clone());
        let dragging = p.drag.is_some();
        let compact = self.compact;
        let fine_keys = this.clone();
        let wheel = this.clone();
        let down = this.clone();
        let watch = this.clone();
        // Formatted once: the visible value, the accessible value and its label share it.
        let text = SharedString::from(map.format(p.shown()));
        let label: SharedString = if p.label.is_empty() {
            text.clone()
        } else {
            p.label_with(&text).into()
        };
        div()
            .id(p.identity.element())
            .track_focus(&p.focus)
            .role(Role::Slider)
            .aria_label(label)
            .aria_value(text.clone())
            .aria_numeric_value(p.shown())
            .aria_min_numeric_value(map.min())
            .aria_max_numeric_value(map.max())
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(hsla(s.text))
            .cursor_ns_resize()
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
                knob_down(&down, e, window, cx);
            })
            .on_scroll(move |e: &ScrollEvent, window, cx| {
                let dy = match e.delta {
                    ScrollDelta::Lines(d) => d.y,
                    ScrollDelta::Pixels(d) => f32::from(d.y) / 20.0,
                };
                if dy.abs() > f32::EPSILON {
                    let name = if dy > 0.0 {
                        names::INCREMENT
                    } else {
                        names::DECREMENT
                    };
                    send(&wheel, name, Value::Null, Source::Pointer, window, cx);
                }
            })
            .on_key_down(move |e: &KeyDownEvent, window, cx| {
                param_key(&fine_keys, e, window, cx);
            })
            .when(self.compact, |el| {
                let tip = text.clone();
                el.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
            })
            .child(
                div()
                    .relative()
                    .child(dial)
                    .when_some(live, |el, live| {
                        el.child(div().absolute().top_0().left_0().child(live))
                    })
                    .when_some(arc_tip, |el, tip| {
                        el.child(
                            div()
                                .id("knob-arc-tip")
                                .debug_selector(|| "knob-arc-tip".into())
                                .absolute()
                                .bottom_full()
                                .mb_1()
                                .px_1()
                                .rounded_sm()
                                .bg(hsla(s.surface))
                                .border_1()
                                .border_color(hsla(s.border))
                                .whitespace_nowrap()
                                .test_support()
                                .child(tip),
                        )
                    })
                    .when(dragging, |el| {
                        // While dragging, follow the pointer anywhere in the window.
                        el.child(
                            canvas(
                                |_, _, _| (),
                                move |_, _, window, _| {
                                    let moves = watch.clone();
                                    window.on_mouse_event(
                                        move |e: &MouseMoveEvent, phase, _, cx| {
                                            if phase == DispatchPhase::Bubble {
                                                knob_move(&moves, e, cx);
                                            }
                                        },
                                    );
                                    let ups = watch.clone();
                                    window.on_mouse_event(
                                        move |e: &MouseUpEvent, phase, window, cx| {
                                            if phase == DispatchPhase::Bubble
                                                && e.button == MouseButton::Left
                                            {
                                                drag_release(&ups, handle, window, cx);
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                    }),
            )
            .map(|el| match (compact, dragging) {
                // Compact: the value floats above the dial while dragging, without layout.
                (true, true) => el.relative().child(
                    div()
                        .id("knob-drag-readout")
                        .absolute()
                        .bottom_full()
                        .mb_1()
                        .px_1()
                        .rounded_sm()
                        .bg(hsla(s.surface))
                        .border_1()
                        .border_color(hsla(s.border))
                        .whitespace_nowrap()
                        .test_support()
                        .child(text),
                ),
                (true, false) => el,
                (false, _) => el.child(text),
            })
    }
}

use gpui_kit::TestSupportExt as _;
use gpui_kit::prelude::FluentBuilder;

fn knob_down(
    this: &WeakEntity<KnobControl>,
    e: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = this.upgrade() else { return };
    if e.click_count >= 2 {
        let default = entity.read(cx).param.map.default_value();
        commit(this, default, Source::Pointer, false, window, cx);
        return;
    }
    entity.update(cx, |knob, cx| {
        if knob.param.disabled {
            return;
        }
        window.focus(&knob.param.focus, cx);
        knob.param.drag = Some(Drag {
            y: f32::from(e.position.y),
            x: f32::from(e.position.x),
            travel: knob.param.map.to_normal(knob.param.value),
        });
        cx.notify();
    });
}

fn knob_move(this: &WeakEntity<KnobControl>, e: &MouseMoveEvent, cx: &mut App) {
    let Some(entity) = this.upgrade() else { return };
    entity.update(cx, |knob, cx| {
        let Some(drag) = knob.param.drag else { return };
        let dy = f64::from(drag.y - f32::from(e.position.y));
        let travel = drag_travel(drag.travel, dy, e.modifiers.shift, knob.param.detent());
        let preview = knob.param.map.value_at(travel);
        if knob.param.preview != Some(preview) {
            knob.param.preview = Some(preview);
            cx.notify();
        }
    });
}

/// End a drag: commit the previewed value as one batch (one undo step).
fn drag_release<T: Observe + Drive + Identified + ParamOwner>(
    this: &WeakEntity<T>,
    handle: AnyWindowHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let _ = handle;
    let Some(entity) = this.upgrade() else { return };
    let preview = entity.update(cx, |control, cx| {
        let param = control.param_mut();
        param.drag = None;
        cx.notify();
        param.preview
    });
    if let Some(value) = preview {
        commit(this, value, Source::Pointer, true, window, cx);
    }
    // A vetoed commit leaves the committed value shown.
    entity.update(cx, |control, cx| {
        control.param_mut().preview = None;
        cx.notify();
    });
}

/// Arrow keys step (Shift: fine), Home/End go to the ends.
fn param_key<T: Observe + Drive + Identified + ParamOwner>(
    this: &WeakEntity<T>,
    e: &KeyDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = this.upgrade() else { return };
    let fine = e.keystroke.modifiers.shift;
    let up = match e.keystroke.key.as_str() {
        "up" | "right" => Some(true),
        "down" | "left" => Some(false),
        _ => None,
    };
    let (map, value) = {
        let param = entity.read(cx).param();
        (param.map.clone(), param.value)
    };
    let target = match (up, e.keystroke.key.as_str()) {
        (Some(up), _) if !fine => {
            let name = if up {
                names::INCREMENT
            } else {
                names::DECREMENT
            };
            send(this, name, Value::Null, Source::Keyboard, window, cx);
            cx.stop_propagation();
            return;
        }
        (Some(up), _) => map.step_from(value, up, true),
        (None, "home") => map.min(),
        (None, "end") => map.max(),
        _ => return,
    };
    commit(this, target, Source::Keyboard, false, window, cx);
    cx.stop_propagation();
}

/// Access to the shared parameter state, for the input helpers.
trait ParamOwner {
    fn param(&self) -> &Param;
    fn param_mut(&mut self) -> &mut Param;
}

impl ParamOwner for KnobControl {
    fn param(&self) -> &Param {
        &self.param
    }
    fn param_mut(&mut self) -> &mut Param {
        &mut self.param
    }
}

// --- NumberField --------------------------------------------------------------------

/// Builder for [`NumberFieldControl`].
pub struct NumberFieldBuilder {
    param: ParamBuilder,
    reset_to: Option<f64>,
    size: guiraffe_core::Size,
}

impl Default for NumberFieldBuilder {
    fn default() -> Self {
        Self {
            param: ParamBuilder::default(),
            reset_to: None,
            size: guiraffe_core::Size::Sm,
        }
    }
}

param_builder!(NumberFieldBuilder);

impl NumberFieldBuilder {
    /// The value an Alt- or Ctrl-click resets to. Without it the field has no reset gesture.
    /// (Double-click and Enter open typed entry, so reset takes a modifier click.)
    pub fn default_value(mut self, value: f64) -> Self {
        self.reset_to = Some(value);
        self
    }
    /// The shared control size: `Xs` for compact toolbars, `Sm` (the default), `Md`.
    pub fn size(mut self, size: guiraffe_core::Size) -> Self {
        self.size = size;
        self
    }
    /// Build the control, registering its path (if any) in `window`.
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<NumberFieldControl> {
        self.build_with(Some(window), cx)
    }
    /// Build the control without a window. A path is reserved now and registers in the
    /// window the control first renders in, following it to another window; until then,
    /// the bus answers queries for it with `ActionError::NotMounted`.
    pub fn build_later(self, cx: &mut App) -> Entity<NumberFieldControl> {
        self.build_with(None, cx)
    }
    fn build_with(self, window: Option<&mut Window>, cx: &mut App) -> Entity<NumberFieldControl> {
        build(
            "number-field",
            Capabilities::SET_VALUE | Capabilities::INCREMENT | Capabilities::DECREMENT,
            self.param.path.clone(),
            self.param.name.clone(),
            window,
            cx,
            |identity, window, cx| {
                // The entry state needs a window; built without one, it is made when typed
                // entry first opens.
                let (entry, subscription) = window
                    .map(|window| new_entry(&identity.window, window, cx))
                    .unzip();
                let param = make_param(self.param, identity, cx);
                let reset_to = self.reset_to.map(|v| param.map.constrain(v));
                NumberFieldControl {
                    param,
                    reset_to,
                    size: self.size,
                    entry,
                    editing: false,
                    error: None,
                    _subscription: subscription,
                }
            },
        )
    }
}

/// A compact numeric field over a [`ValueMapping`]: shows the formatted value with units,
/// scrubs on a horizontal or vertical drag (Shift for fine), steps with the arrow keys, and
/// takes typed entry (double-click or Enter) through the mapping's parser, so `2.5k`,
/// `250 ms` and `C#4` all work where the unit allows.
pub struct NumberFieldControl {
    param: Param,
    reset_to: Option<f64>,
    size: guiraffe_core::Size,
    /// `None` until typed entry first opens, for a control built without a window.
    entry: Option<Entity<InputState>>,
    editing: bool,
    error: Option<SharedString>,
    _subscription: Option<Subscription>,
}

/// The typed-entry state of a [`NumberFieldControl`] living in `window`: Enter or blur
/// commits.
fn new_entry(
    handle: &super::WindowCell,
    window: &mut Window,
    cx: &mut Context<NumberFieldControl>,
) -> (Entity<InputState>, Subscription) {
    let entry = cx.new(|cx| InputState::new(window, cx));
    let this = cx.weak_entity();
    let handle = handle.clone();
    let subscription = cx.subscribe(
        &entry,
        move |field: &mut NumberFieldControl, _, event: &InputEvent, cx| match event {
            InputEvent::PressEnter { .. } | InputEvent::Blur if field.editing => {
                let this = this.clone();
                let handle = handle.clone();
                cx.defer(move |cx| {
                    super::in_window(&handle, cx, |window, cx| {
                        entry_done(&this, true, window, cx)
                    });
                });
            }
            _ => {}
        },
    );
    (entry, subscription)
}

impl Identified for NumberFieldControl {
    fn identity_mut(&mut self) -> &mut Identity {
        &mut self.param.identity
    }
    fn identity(&self) -> &Identity {
        &self.param.identity
    }
}

impl ParamOwner for NumberFieldControl {
    fn param(&self) -> &Param {
        &self.param
    }
    fn param_mut(&mut self) -> &mut Param {
        &mut self.param
    }
}

impl NumberFieldControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> NumberFieldBuilder {
        NumberFieldBuilder::default()
    }
    pub fn value(&self) -> f64 {
        self.param.value
    }
    pub fn formatted(&self) -> String {
        self.param.map.format(self.param.shown())
    }
    pub fn mapping(&self) -> &dyn ValueMapping {
        &*self.param.map
    }
    pub fn control_id(&self) -> Option<guiraffe_core::protocol::ControlId> {
        self.param.identity.id
    }
    pub fn element_id(&self) -> ElementId {
        self.param.identity.element()
    }
    /// The value the reset gesture returns to, if any.
    pub fn reset_value(&self) -> Option<f64> {
        self.reset_to
    }
    pub fn size(&self) -> guiraffe_core::Size {
        self.size
    }
    /// Whether typed entry is open.
    pub fn editing(&self) -> bool {
        self.editing
    }
    /// The last typed entry's parse error, until the next commit.
    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }
    /// The text entry state (tests and focus); `None` before typed entry first opens in a
    /// control built without a window (`build_later`).
    pub fn entry(&self) -> Option<&Entity<InputState>> {
        self.entry.as_ref()
    }
    /// Host write: the value, without an event. Needs no window.
    pub fn set_value(&mut self, value: f64, cx: &mut Context<Self>) {
        if self.param.set(value) {
            cx.notify();
        }
    }
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if disabled != self.param.disabled {
            self.param.disabled = disabled;
            cx.notify();
        }
    }
    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        let label = label.into();
        if label != self.param.label {
            self.param.label = label;
            cx.notify();
        }
    }
    /// Open typed entry with the formatted value selected.
    pub fn begin_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.param.disabled {
            return;
        }
        let text = self.param.map.format(self.param.value);
        let entry = match &self.entry {
            Some(entry) => entry.clone(),
            None => {
                let (entry, subscription) = new_entry(&self.param.identity.window, window, cx);
                self.entry = Some(entry.clone());
                self._subscription = Some(subscription);
                entry
            }
        };
        entry.update(cx, |entry, cx| {
            entry.set_value(text, window, cx);
            entry.focus(window, cx);
        });
        self.editing = true;
        self.error = None;
        cx.notify();
    }
}

/// Close typed entry: parse and commit (`accept`), or cancel. A parse error keeps entry open.
fn entry_done(
    this: &WeakEntity<NumberFieldControl>,
    accept: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = this.upgrade() else { return };
    let parsed = entity.update(cx, |field, cx| {
        if !field.editing {
            return None;
        }
        if !accept {
            field.editing = false;
            cx.notify();
            return None;
        }
        let Some(text) = field
            .entry
            .as_ref()
            .map(|entry| entry.read(cx).value().clone())
        else {
            field.editing = false;
            cx.notify();
            return None;
        };
        match field.param.map.parse(&text) {
            Ok(value) => {
                field.editing = false;
                field.error = None;
                cx.notify();
                Some(value)
            }
            Err(error) => {
                field.error = Some(error.to_string().into());
                cx.notify();
                None
            }
        }
    });
    if let Some(value) = parsed {
        commit(this, value, Source::Keyboard, false, window, cx);
        entity.update(cx, |field, cx| window.focus(&field.param.focus, cx));
    }
}

impl Focusable for NumberFieldControl {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.param.focus.clone()
    }
}

impl EventEmitter<ParamEvent> for NumberFieldControl {}

impl Observe for NumberFieldControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        self.param.snapshot()
    }
}

impl Drive for NumberFieldControl {
    type Action = ParamAction;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<ParamAction, ActionError> {
        decode_param(action)
    }
    fn apply(
        &mut self,
        action: ParamAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        if let Some(next) = self.param.apply(action)? {
            cx.emit(ParamEvent::Changed(next, self.param.identity.source(cx)));
        }
        cx.notify();
        Ok(Outcome(Value::Num(self.param.value)))
    }
}

fn field_move(this: &WeakEntity<NumberFieldControl>, e: &MouseMoveEvent, cx: &mut App) {
    let Some(entity) = this.upgrade() else { return };
    entity.update(cx, |field, cx| {
        let Some(drag) = field.param.drag else { return };
        // Up and right both scrub up.
        let dy = f64::from(drag.y - f32::from(e.position.y))
            + f64::from(f32::from(e.position.x) - drag.x);
        let travel = drag_travel(drag.travel, dy, e.modifiers.shift, field.param.detent());
        let preview = field.param.map.value_at(travel);
        if field.param.preview != Some(preview) {
            field.param.preview = Some(preview);
            cx.notify();
        }
    });
}

impl Render for NumberFieldControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        mount(self, window, cx);
        let s = surfaces(cx);
        let p = &self.param;
        let map = p.map.clone();
        let this = cx.weak_entity();
        let handle = window.window_handle();
        let text = map.format(p.shown());
        let border = if self.error.is_some() {
            hsla(
                crate::style::variant(
                    cx,
                    Some(guiraffe_core::ColorName::Red),
                    guiraffe_core::Variant::Filled,
                )
                .background,
            )
        } else {
            hsla(s.border)
        };
        let base = div()
            .id(p.identity.element())
            .track_focus(&p.focus)
            .role(Role::SpinButton)
            .aria_label(p.accessible_label())
            .aria_value(text.clone())
            .aria_numeric_value(p.shown())
            .aria_min_numeric_value(map.min())
            .aria_max_numeric_value(map.max())
            .min_w(px(self.size.control_height() * 2.))
            .h(px(self.size.control_height()))
            .flex()
            .items_center()
            .px(px(self.size.spacing()))
            .rounded_sm()
            .border_1()
            .border_color(border)
            .text_size(px(self.size.font_size()))
            .text_color(hsla(s.text));
        if self.editing
            && let Some(entry) = &self.entry
        {
            return base
                .child(Input::new(entry))
                .test_support()
                .into_any_element();
        }
        let dragging = p.drag.is_some();
        let down = this.clone();
        let keys = this.clone();
        let watch = this.clone();
        base.cursor_ew_resize()
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
                let Some(entity) = down.upgrade() else { return };
                entity.update(cx, |field, cx| {
                    if field.param.disabled {
                        return;
                    }
                    if e.click_count >= 2 {
                        field.param.drag = None;
                        field.begin_entry(window, cx);
                        return;
                    }
                    // Alt- or Ctrl-click resets: a set_value through the bus, one undo step.
                    if (e.modifiers.alt || e.modifiers.control)
                        && let Some(value) = field.reset_to
                    {
                        let this = cx.weak_entity();
                        window.defer(cx, move |window, cx| {
                            commit(&this, value, Source::Pointer, false, window, cx);
                        });
                        return;
                    }
                    window.focus(&field.param.focus, cx);
                    field.param.drag = Some(Drag {
                        y: f32::from(e.position.y),
                        x: f32::from(e.position.x),
                        travel: field.param.map.to_normal(field.param.value),
                    });
                    cx.notify();
                });
            })
            .on_key_down(move |e: &KeyDownEvent, window, cx| {
                if e.keystroke.key == "enter" {
                    if let Some(entity) = keys.upgrade() {
                        entity.update(cx, |field, cx| field.begin_entry(window, cx));
                    }
                    cx.stop_propagation();
                    return;
                }
                param_key(&keys, e, window, cx);
            })
            .when(dragging, |el| {
                el.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let moves = watch.clone();
                            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    field_move(&moves, e, cx);
                                }
                            });
                            let ups = watch.clone();
                            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                                    drag_release(&ups, handle, window, cx);
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
            .child(text)
            .test_support()
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drags_scale_fine_and_detent() {
        assert!((drag_travel(0.5, 100.0, false, None) - 1.0).abs() < 1e-9);
        assert!((drag_travel(0.5, 100.0, true, None) - 0.55).abs() < 1e-9);
        assert_eq!(drag_travel(0.9, 1_000.0, false, None), 1.0);
        assert_eq!(drag_travel(0.1, -1_000.0, false, None), 0.0);
        assert_eq!(
            drag_travel(0.4, 21.0, false, Some(0.5)),
            0.5,
            "snaps to the centre"
        );
        assert!(
            drag_travel(0.4, 21.0, true, Some(0.5)) < 0.5,
            "fine drags skip the detent"
        );
    }

    #[test]
    fn arcs_sweep_from_the_start_or_the_centre() {
        assert_eq!(knob_angle(0.0), START);
        assert!((knob_angle(1.0) - (START + SWEEP)).abs() < 1e-6);
        assert_eq!(value_span(0.3, None), (0.0, 0.3));
        assert_eq!(value_span(0.3, Some(0.5)), (0.3, 0.5));
        assert_eq!(value_span(0.8, Some(0.5)), (0.5, 0.8));
    }
}
