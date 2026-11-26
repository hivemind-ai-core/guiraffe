//! `KnobControl` and `NumberFieldControl`: compact parameter controls over a
//! [`ValueMapping`], shared with `SliderControl`.
//!
//! Both are protocol control entities like the rest of this module. Actions: `set_value`
//! (num), `increment`, `decrement`. A drag previews locally and commits one `set_value` in a
//! `dispatch_batch` on release, so a drag is one undo step. Host setters never emit, and
//! every event carries the input's [`Source`].

use super::{Identified, Identity, action, build, enabled, num_payload, route};
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

/// Emitted when a knob or number field commits a value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamEvent {
    Changed(f64, Source),
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
        map.from_normal(0.5)
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
}

param_builder!(KnobBuilder);

impl KnobBuilder {
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
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<KnobControl> {
        let _ = window;
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
    /// Host write: the modulation range, in value units.
    pub fn set_modulation(&mut self, range: Option<(f64, f64)>, cx: &mut Context<Self>) {
        if range != self.modulation {
            self.modulation = range;
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
    let s = surfaces(cx);
    let primary = GuiraffeTheme::global(cx).primary_color().to_hsla();
    let colors = KnobColors {
        track: hsla(s.track),
        value: primary,
        modulation: primary.opacity(0.45),
        pointer: hsla(s.text),
    };
    let width = (diameter / 10.0).max(2.0);
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            arc(bounds, width + 1.0, width, 0.0, 1.0, colors.track, window);
            let (from, to) = value_span(travel, centre);
            arc(bounds, width + 1.0, width, from, to, colors.value, window);
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
        let dial = knob_dial(travel, detent, modulation, self.diameter, cx);
        let dragging = p.drag.is_some();
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
            .child(div().relative().child(dial).when(dragging, |el| {
                // While dragging, follow the pointer anywhere in the window.
                el.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let moves = watch.clone();
                            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    knob_move(&moves, e, cx);
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
                    .size_full(),
                )
            }))
            .child(text)
    }
}

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
        let preview = knob.param.map.from_normal(travel);
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
#[derive(Default)]
pub struct NumberFieldBuilder {
    param: ParamBuilder,
}

param_builder!(NumberFieldBuilder);

impl NumberFieldBuilder {
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<NumberFieldControl> {
        build(
            "number-field",
            Capabilities::SET_VALUE | Capabilities::INCREMENT | Capabilities::DECREMENT,
            self.param.path.clone(),
            self.param.name.clone(),
            window,
            cx,
            |identity, window, cx| {
                let entry = cx.new(|cx| InputState::new(window, cx));
                let this = cx.weak_entity();
                let handle = window.window_handle();
                let subscription = cx.subscribe(
                    &entry,
                    move |field: &mut NumberFieldControl, _, event: &InputEvent, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            if field.editing {
                                let this = this.clone();
                                cx.defer(move |cx| {
                                    handle
                                        .update(cx, |_, window, cx| {
                                            entry_done(&this, true, window, cx)
                                        })
                                        .ok();
                                });
                            }
                        }
                        _ => {}
                    },
                );
                NumberFieldControl {
                    param: make_param(self.param, identity, cx),
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
    entry: Entity<InputState>,
    editing: bool,
    error: Option<SharedString>,
    _subscription: Subscription,
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
    /// Whether typed entry is open.
    pub fn editing(&self) -> bool {
        self.editing
    }
    /// The last typed entry's parse error, until the next commit.
    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }
    /// The text entry state (tests and focus).
    pub fn entry(&self) -> &Entity<InputState> {
        &self.entry
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
        self.entry.update(cx, |entry, cx| {
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
        let text = field.entry.read(cx).value().clone();
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
        let preview = field.param.map.from_normal(travel);
        if field.param.preview != Some(preview) {
            field.param.preview = Some(preview);
            cx.notify();
        }
    });
}

impl Render for NumberFieldControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .min_w(px(64.))
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(border)
            .text_sm()
            .text_color(hsla(s.text));
        if self.editing {
            return base.child(Input::new(&self.entry)).into_any_element();
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
