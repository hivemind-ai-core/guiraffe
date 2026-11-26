//! Drivable control entities for the shared control protocol.
//!
//! The kit builders (`Button`, `Checkbox`, ...) stay the zero-cost default for an
//! unbound, unnamed control. These entities are opt-in: build one when a control
//! needs an inspect path or (from item 9) a store binding.
//!
//! ```ignore
//! let theme = CheckboxControl::new().label("Dark mode").path("settings/dark").build(window, cx);
//! ```
//!
//! Every control has the same surface:
//! - `XControl::new()` plus builder props, then `.path(..)` to register it on the
//!   window's action bus at build time. Without a path nothing is registered.
//! - `build(window, cx) -> Entity<XControl>`.
//! - A typed event (`XEvent`) per user-visible change. Bindings subscribe to it for
//!   `control → store`.
//! - Host setters (`set_checked`, `set_value`, ...) for `store → control`. They never
//!   emit, so a host write never echoes as a user action.
//!
//! Pointer and keyboard handlers build the same erased [`Action`] a bus client sends.
//! A registered control dispatches it through [`ActionBus`], tagged `Source::Pointer`
//! or `Source::Keyboard`, so interceptors see physical input too. An unregistered
//! control applies it directly. Both paths run the same [`Drive::apply`].
//!
//! Payloads: `set_value` takes a `Bool`, `Num` or `Str` matching the control.
//! `select` and a menu's `activate` take a `Num` index. `increment` and
//! `decrement` take `Null`.
pub use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::{
    Disableable, IndexPath,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    switch::Switch,
    tab::{Tab, TabBar},
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, ElementId, Entity, EventEmitter, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, WeakEntity, Window,
    div,
};
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
    Outcome, Source, names,
};
use guiraffe_core::widget_value::Value;
use std::rc::Rc;

use crate::mapping::{ValueMap, ValueMapping};

/// Identity shared by every control: its bus handle and element id base.
#[derive(Clone, Debug)]
struct Identity {
    id: Option<ControlId>,
    base: SharedString,
    /// Source of a direct (unregistered) input while its `apply` runs.
    direct: Option<Source>,
}

impl Identity {
    fn element(&self) -> ElementId {
        ElementId::Name(self.base.clone())
    }
    fn child(&self, ix: usize) -> ElementId {
        ElementId::NamedInteger(self.base.clone(), ix as u64)
    }
    /// The source of the edit being applied: the bus dispatch's, else the direct
    /// input's. A host calling `apply` itself is tagged `Host`.
    fn source(&self, cx: &App) -> Source {
        ActionBus::current_source(cx)
            .or(self.direct)
            .unwrap_or(Source::Host)
    }
}

fn build<T: Observe + Drive + Render + Identified>(
    kind: &'static str,
    capabilities: Capabilities,
    path: Option<SharedString>,
    name: Option<SharedString>,
    window: &mut Window,
    cx: &mut App,
    make: impl FnOnce(Identity, &mut Window, &mut Context<T>) -> T,
) -> Entity<T> {
    let entity = cx.new(|cx| {
        // An explicit element id names the element even when a path registers the control,
        // so tests can find it by either.
        let base = name
            .or_else(|| path.clone())
            .unwrap_or_else(|| format!("{kind}-{}", cx.entity_id().as_u64()).into());
        make(
            Identity {
                id: None,
                base,
                direct: None,
            },
            window,
            cx,
        )
    });
    if let Some(path) = path {
        let id = ActionBus::register(&path, &entity, kind, capabilities, window, cx);
        entity.update(cx, |control, _| control.identity_mut().id = Some(id));
    }
    entity
}

/// Access to the shared identity, for the routing helpers.
trait Identified {
    fn identity_mut(&mut self) -> &mut Identity;
    fn identity(&self) -> &Identity;
}

/// Route a physical input exactly as a bus client would send it.
fn route<T: Observe + Drive + Identified>(
    this: &WeakEntity<T>,
    action: Action,
    source: Source,
    window: &mut Window,
    cx: &mut App,
) -> Result<Outcome, ActionError> {
    let entity = this.upgrade().ok_or(ActionError::UnknownTarget)?;
    match entity.read(cx).identity().id {
        Some(id) => ActionBus::dispatch(id, &action, source, window, cx),
        None => entity
            .update(cx, |control, cx| {
                let typed = control.decode_for(&action)?;
                control.identity_mut().direct = Some(source);
                let result = control.apply(typed, window, cx);
                control.identity_mut().direct = None;
                result
            })
            .map(Into::into),
    }
}

/// Route from a kit state subscription, which runs outside any window update.
fn route_in<T: Observe + Drive + Identified>(
    handle: AnyWindowHandle,
    this: &WeakEntity<T>,
    action: Action,
    source: Source,
    cx: &mut App,
) -> Result<Outcome, ActionError> {
    handle
        .update(cx, |_, window, cx| route(this, action, source, window, cx))
        .unwrap_or(Err(ActionError::UnknownTarget))
}

fn action(name: &'static str, payload: Value) -> Action {
    Action {
        name: name.into(),
        payload,
    }
}

fn bool_payload(action: &Action) -> Result<bool, ActionError> {
    match action.payload {
        Value::Bool(value) => Ok(value),
        _ => Err(ActionError::Invalid("set_value expects a bool")),
    }
}

fn num_payload(action: &Action) -> Result<f64, ActionError> {
    match action.payload {
        Value::Num(value) if value.is_finite() => Ok(value),
        _ => Err(ActionError::Invalid("expected a finite number")),
    }
}

fn index_payload(action: &Action, len: usize) -> Result<usize, ActionError> {
    let value = num_payload(action)?;
    if value < 0.0 || value.fract() != 0.0 || value >= len as f64 {
        return Err(ActionError::Invalid("index out of range"));
    }
    Ok(value as usize)
}

fn enabled(disabled: bool) -> Result<(), ActionError> {
    if disabled {
        Err(ActionError::Disabled)
    } else {
        Ok(())
    }
}

macro_rules! identified {
    ($($ty:ty),*) => {$(
        impl Identified for $ty {
            fn identity_mut(&mut self) -> &mut Identity {
                &mut self.identity
            }
            fn identity(&self) -> &Identity {
                &self.identity
            }
        }
        impl $ty {
            /// The bus handle, when built with a path.
            pub fn control_id(&self) -> Option<ControlId> {
                self.identity.id
            }
            /// The id of the rendered root element (the path when registered).
            pub fn element_id(&self) -> ElementId {
                self.identity.element()
            }
        }
    )*};
}

// --- Button -------------------------------------------------------------------

/// Emitted when a [`ButtonControl`] is activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    Activated(Source),
}

/// Builder for [`ButtonControl`].
#[derive(Default)]
pub struct ButtonBuilder {
    label: SharedString,
    variant: ButtonVariant,
    disabled: bool,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl ButtonBuilder {
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    /// The kit's style variant: primary, danger, ghost and so on.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Register on the window's action bus under this inspect path.
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    /// Element id. Without it, the path (if any) names the element; with both, this wins.
    pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<ButtonControl> {
        build(
            "button",
            Capabilities::ACTIVATE,
            self.path,
            self.name,
            window,
            cx,
            |identity, _, _| ButtonControl {
                identity,
                label: self.label,
                variant: self.variant,
                disabled: self.disabled,
            },
        )
    }
}

/// A push button. Actions: `activate`.
pub struct ButtonControl {
    identity: Identity,
    label: SharedString,
    variant: ButtonVariant,
    disabled: bool,
}

impl ButtonControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> ButtonBuilder {
        ButtonBuilder::default()
    }
    /// Host write: one-way.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if disabled != self.disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }
    /// Host write: one-way.
    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        let label = label.into();
        if label != self.label {
            self.label = label;
            cx.notify();
        }
    }
    pub fn variant(&self) -> &ButtonVariant {
        &self.variant
    }
    /// Host write: the style variant. One-way.
    pub fn set_variant(&mut self, variant: ButtonVariant, cx: &mut Context<Self>) {
        if variant != self.variant {
            self.variant = variant;
            cx.notify();
        }
    }
}

impl EventEmitter<ButtonEvent> for ButtonControl {}

impl Observe for ButtonControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            label: Some(self.label.as_ref().into()),
            disabled: Some(self.disabled),
            ..ControlSnapshot::default()
        }
    }
}

impl Drive for ButtonControl {
    type Action = ();
    type Output = ();
    fn decode(action: &Action) -> Result<(), ActionError> {
        if action.name == names::ACTIVATE {
            Ok(())
        } else {
            Err(ActionError::Unsupported)
        }
    }
    fn apply(&mut self, (): (), _: &mut Window, cx: &mut Context<Self>) -> Result<(), ActionError> {
        enabled(self.disabled)?;
        cx.emit(ButtonEvent::Activated(self.identity.source(cx)));
        Ok(())
    }
}

impl Render for ButtonControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        Button::new(self.identity.element())
            .label(self.label.clone())
            .with_variant(self.variant)
            .disabled(self.disabled)
            .on_click(move |_, window, cx| {
                route(
                    &this,
                    action(names::ACTIVATE, Value::Null),
                    Source::Pointer,
                    window,
                    cx,
                )
                .ok();
            })
    }
}

// --- Checkbox and Switch --------------------------------------------------------

/// Emitted when a checkbox or switch changes state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleEvent {
    Changed(bool, Source),
}

/// Typed actions of a checkbox or switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleAction {
    Toggle,
    Set(bool),
}

fn decode_toggle(action: &Action) -> Result<ToggleAction, ActionError> {
    match action.name.as_ref() {
        names::TOGGLE => Ok(ToggleAction::Toggle),
        names::SET_VALUE => bool_payload(action).map(ToggleAction::Set),
        _ => Err(ActionError::Unsupported),
    }
}

macro_rules! toggle_control {
    ($control:ident, $builder:ident, $kind:literal, $kit:ident, $doc:literal) => {
        /// Builder for the control of the same name.
        #[derive(Default)]
        pub struct $builder {
            label: SharedString,
            checked: bool,
            disabled: bool,
            path: Option<SharedString>,
            name: Option<SharedString>,
        }

        impl $builder {
            pub fn label(mut self, label: impl Into<SharedString>) -> Self {
                self.label = label.into();
                self
            }
            pub fn checked(mut self, checked: bool) -> Self {
                self.checked = checked;
                self
            }
            pub fn disabled(mut self, disabled: bool) -> Self {
                self.disabled = disabled;
                self
            }
            /// Register on the window's action bus under this inspect path.
            pub fn path(mut self, path: impl Into<SharedString>) -> Self {
                self.path = Some(path.into());
                self
            }
            /// Element id. Without it, the path (if any) names the element; with both, this wins.
            pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
                self.name = Some(name.into());
                self
            }
            pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<$control> {
                build(
                    $kind,
                    Capabilities::TOGGLE | Capabilities::SET_VALUE,
                    self.path,
                    self.name,
                    window,
                    cx,
                    |identity, _, _| $control {
                        identity,
                        label: self.label,
                        checked: self.checked,
                        disabled: self.disabled,
                    },
                )
            }
        }

        #[doc = $doc]
        /// Actions: `toggle`, `set_value` (bool). Outcome: the new state.
        pub struct $control {
            identity: Identity,
            label: SharedString,
            checked: bool,
            disabled: bool,
        }

        impl $control {
            #[allow(clippy::new_ret_no_self)]
            pub fn new() -> $builder {
                $builder::default()
            }
            pub fn checked(&self) -> bool {
                self.checked
            }
            /// Host write: changes state without emitting an event.
            pub fn set_checked(&mut self, checked: bool, cx: &mut Context<Self>) {
                if self.checked != checked {
                    self.checked = checked;
                    cx.notify();
                }
            }
            /// Host write: one-way.
            pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
                if disabled != self.disabled {
                    self.disabled = disabled;
                    cx.notify();
                }
            }
            /// Host write: one-way.
            pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
                let label = label.into();
                if label != self.label {
                    self.label = label;
                    cx.notify();
                }
            }
        }

        impl EventEmitter<ToggleEvent> for $control {}

        impl Observe for $control {
            type Snapshot = ControlSnapshot;
            fn snapshot(&self, _: &App) -> ControlSnapshot {
                ControlSnapshot {
                    checked: Some(self.checked),
                    label: Some(self.label.as_ref().into()),
                    disabled: Some(self.disabled),
                    ..ControlSnapshot::default()
                }
            }
        }

        impl Drive for $control {
            type Action = ToggleAction;
            type Output = Outcome;
            fn decode(action: &Action) -> Result<ToggleAction, ActionError> {
                decode_toggle(action)
            }
            fn apply(
                &mut self,
                action: ToggleAction,
                _: &mut Window,
                cx: &mut Context<Self>,
            ) -> Result<Outcome, ActionError> {
                enabled(self.disabled)?;
                let next = match action {
                    ToggleAction::Toggle => !self.checked,
                    ToggleAction::Set(value) => value,
                };
                if next != self.checked {
                    self.checked = next;
                    cx.emit(ToggleEvent::Changed(next, self.identity.source(cx)));
                    cx.notify();
                }
                Ok(Outcome(Value::Bool(next)))
            }
        }

        impl Render for $control {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let this = cx.entity().downgrade();
                $kit::new(self.identity.element())
                    .label(self.label.clone())
                    .checked(self.checked)
                    .disabled(self.disabled)
                    .on_click(move |requested, window, cx| {
                        route(
                            &this,
                            action(names::SET_VALUE, Value::Bool(*requested)),
                            Source::Pointer,
                            window,
                            cx,
                        )
                        .ok();
                    })
            }
        }
    };
}

toggle_control!(
    CheckboxControl,
    CheckboxBuilder,
    "checkbox",
    Checkbox,
    "A checkbox."
);
toggle_control!(SwitchControl, SwitchBuilder, "switch", Switch, "A switch.");

// --- Slider ---------------------------------------------------------------------

/// Emitted when a [`SliderControl`] value changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderControlEvent {
    Changed(f32, Source),
}

/// Typed actions of a slider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderAction {
    Set(f32),
    Increment,
    Decrement,
}

/// Builder for [`SliderControl`].
pub struct SliderBuilder {
    map: Rc<dyn ValueMapping>,
    linear: Option<ValueMap>,
    value: Option<f64>,
    label: SharedString,
    disabled: bool,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl Default for SliderBuilder {
    fn default() -> Self {
        let linear = ValueMap::new(0.0, 100.0).step(1.0);
        Self {
            map: Rc::new(linear),
            linear: Some(linear),
            value: None,
            label: SharedString::default(),
            disabled: false,
            path: None,
            name: None,
        }
    }
}

impl SliderBuilder {
    /// A linear range (keeps the step). Use [`SliderBuilder::mapping`] for log, units and more.
    pub fn range(self, min: f32, max: f32) -> Self {
        self.linear_with(|map| {
            let next = ValueMap::new(f64::from(min), f64::from(max));
            match map.step_size() {
                Some(step) => next.step(step),
                None => next,
            }
        })
    }
    pub fn step(self, step: f32) -> Self {
        self.linear_with(|map| map.step(f64::from(step)))
    }
    fn linear_with(mut self, change: impl FnOnce(ValueMap) -> ValueMap) -> Self {
        let next = change(self.linear.unwrap_or_default());
        self.linear = Some(next);
        self.map = Rc::new(next);
        self
    }
    /// The value mapping shared with knobs and number fields: range, curve, step, default,
    /// units. Replaces `range` and `step`.
    pub fn mapping(mut self, map: impl ValueMapping) -> Self {
        self.map = Rc::new(map);
        self.linear = None;
        self
    }
    pub fn value(mut self, value: f32) -> Self {
        self.value = Some(f64::from(value));
        self
    }
    /// Accessible name of the slider.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Register on the window's action bus under this inspect path.
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    /// Element id. Without it, the path (if any) names the element; with both, this wins.
    pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<SliderControl> {
        let handle = window.window_handle();
        let map = self.map;
        build(
            "slider",
            Capabilities::SET_VALUE | Capabilities::INCREMENT | Capabilities::DECREMENT,
            self.path,
            self.name,
            window,
            cx,
            |identity, _, cx| {
                let value = map.constrain(self.value.unwrap_or_else(|| map.default_value()));
                // The kit slider runs in normalised travel, so every mapping (log Hz, exp
                // times) drags the same way it does on a knob.
                let travel_step = kit_step(&*map);
                let start = map.to_normal(value) as f32;
                let state = cx.new(|_| {
                    SliderState::new()
                        .min(0.0)
                        .max(1.0)
                        .step(travel_step)
                        .default_value(start)
                });
                let this: WeakEntity<SliderControl> = cx.entity().downgrade();
                let read = map.clone();
                let subscription = App::subscribe(cx, &state, move |state, event, cx| {
                    let SliderEvent::Change(SliderValue::Single(normal)) = *event else {
                        return;
                    };
                    let value = read.from_normal(f64::from(normal));
                    let routed = route_in(
                        handle,
                        &this,
                        action(names::SET_VALUE, Value::Num(value)),
                        Source::Pointer,
                        cx,
                    );
                    // A vetoed or disabled change snaps the kit state back.
                    if routed.is_err()
                        && let Some(control) = this.upgrade()
                    {
                        let current = control.read(cx).normal();
                        handle
                            .update(cx, |_, window, cx| {
                                state.update(cx, |s, cx| s.set_value(current, window, cx))
                            })
                            .ok();
                    }
                });
                SliderControl {
                    identity,
                    state,
                    value,
                    map,
                    label: self.label,
                    disabled: self.disabled,
                    pending: false,
                    _subscription: subscription,
                }
            },
        )
    }
}

/// The kit slider's step in travel: the mapping's value step on a linear range, else a
/// hundredth of the travel.
fn kit_step(map: &dyn ValueMapping) -> f32 {
    let span = map.max() - map.min();
    let one = map.step_from(map.min(), true, false) - map.min();
    if span > 0.0 && one > 0.0 && (map.to_normal(map.min() + one) - one / span).abs() < 1e-9 {
        (one / span) as f32
    } else {
        0.001
    }
}

/// A single-value slider over a [`ValueMapping`]. Actions: `set_value` (num), `increment`,
/// `decrement`. Outcome: the new value.
pub struct SliderControl {
    identity: Identity,
    state: Entity<SliderState>,
    value: f64,
    map: Rc<dyn ValueMapping>,
    label: SharedString,
    disabled: bool,
    /// A host write the kit state has not seen yet.
    pending: bool,
    _subscription: Subscription,
}

impl SliderControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> SliderBuilder {
        SliderBuilder::default()
    }
    pub fn value(&self) -> f32 {
        self.value as f32
    }
    /// The value at full precision.
    pub fn value_f64(&self) -> f64 {
        self.value
    }
    /// The value as the mapping formats it (`1.25 kHz`).
    pub fn formatted(&self) -> String {
        self.map.format(self.value)
    }
    pub fn mapping(&self) -> &dyn ValueMapping {
        &*self.map
    }
    fn normal(&self) -> f32 {
        self.map.to_normal(self.value) as f32
    }
    /// The kit state that renders the slider and receives pointer and key input.
    pub fn state(&self) -> &Entity<SliderState> {
        &self.state
    }
    /// Host write: changes the value without emitting an event. Needs no window;
    /// the kit state follows at the next render.
    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        let value = self.map.constrain(f64::from(value));
        if value != self.value {
            self.value = value;
            self.pending = true;
            cx.notify();
        }
    }
    /// Host write: one-way.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if disabled != self.disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }
    /// Host write: the accessible name. One-way.
    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        let label = label.into();
        if label != self.label {
            self.label = label;
            cx.notify();
        }
    }
    fn sync_state(&self, window: &mut Window, cx: &mut Context<Self>) {
        let normal = self.normal();
        if self.state.read(cx).value() != SliderValue::Single(normal) {
            self.state
                .update(cx, |state, cx| state.set_value(normal, window, cx));
        }
    }
}

impl EventEmitter<SliderControlEvent> for SliderControl {}

impl Observe for SliderControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: Some(Value::Num(self.value)),
            label: (!self.label.is_empty()).then(|| self.label.as_ref().into()),
            disabled: Some(self.disabled),
            ..ControlSnapshot::default()
        }
    }
}

impl Drive for SliderControl {
    type Action = SliderAction;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<SliderAction, ActionError> {
        match action.name.as_ref() {
            names::SET_VALUE => num_payload(action).map(|v| SliderAction::Set(v as f32)),
            names::INCREMENT => Ok(SliderAction::Increment),
            names::DECREMENT => Ok(SliderAction::Decrement),
            _ => Err(ActionError::Unsupported),
        }
    }
    fn apply(
        &mut self,
        action: SliderAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        enabled(self.disabled)?;
        let next = match action {
            SliderAction::Set(value) => self.map.constrain(f64::from(value)),
            SliderAction::Increment => self.map.step_from(self.value, true, false),
            SliderAction::Decrement => self.map.step_from(self.value, false, false),
        };
        if next != self.value {
            self.value = next;
            self.sync_state(window, cx);
            cx.emit(SliderControlEvent::Changed(
                next as f32,
                self.identity.source(cx),
            ));
        }
        Ok(Outcome(Value::Num(next)))
    }
}

impl Render for SliderControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.pending) {
            self.sync_state(window, cx);
        }
        let mut slider = Slider::new(&self.state).disabled(self.disabled);
        if !self.label.is_empty() {
            slider = slider.accessibility_label(self.label.clone());
        }
        div().id(self.identity.element()).w_full().child(slider)
    }
}

mod knob;
pub use knob::*;

// --- Text input -----------------------------------------------------------------

/// Emitted when a [`TextInputControl`] value changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputEvent {
    Changed(SharedString, Source),
}

/// Typed actions of a text input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputAction {
    Set(SharedString),
    Focus,
}

/// Builder for [`TextInputControl`].
#[derive(Default)]
pub struct TextInputBuilder {
    value: SharedString,
    placeholder: SharedString,
    disabled: bool,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl TextInputBuilder {
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Register on the window's action bus under this inspect path.
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    /// Element id. Without it, the path (if any) names the element; with both, this wins.
    pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<TextInputControl> {
        let handle = window.window_handle();
        build(
            "text_input",
            Capabilities::SET_VALUE | Capabilities::FOCUS,
            self.path,
            self.name,
            window,
            cx,
            |identity, window, cx| {
                let placeholder = self.placeholder;
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder.clone()));
                let value = self.value;
                input.update(cx, |state, cx| state.set_value(value.clone(), window, cx));
                let this: WeakEntity<TextInputControl> = cx.entity().downgrade();
                let subscription = App::subscribe(cx, &input, move |input, event, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let Some(control) = this.upgrade() else {
                        return;
                    };
                    let typed = input.read(cx).value();
                    let current = control.read(cx).value.clone();
                    if typed == current {
                        return;
                    }
                    let routed = route_in(
                        handle,
                        &this,
                        action(names::SET_VALUE, Value::Str(typed)),
                        Source::Keyboard,
                        cx,
                    );
                    if routed.is_err() {
                        handle
                            .update(cx, |_, window, cx| {
                                input.update(cx, |s, cx| s.set_value(current, window, cx))
                            })
                            .ok();
                    }
                });
                TextInputControl {
                    identity,
                    input,
                    value,
                    placeholder,
                    value_pending: false,
                    placeholder_pending: false,
                    disabled: self.disabled,
                    _subscription: subscription,
                }
            },
        )
    }
}

/// A single-line text input. Actions: `set_value` (str), `focus`.
/// Outcome: the new text.
pub struct TextInputControl {
    identity: Identity,
    input: Entity<InputState>,
    value: SharedString,
    placeholder: SharedString,
    /// Host writes the kit state has not seen yet.
    value_pending: bool,
    placeholder_pending: bool,
    disabled: bool,
    _subscription: Subscription,
}

impl TextInputControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TextInputBuilder {
        TextInputBuilder::default()
    }
    pub fn value(&self) -> &SharedString {
        &self.value
    }
    /// The kit state that renders the input and receives typing.
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }
    /// Host write: changes the text without emitting an event. Needs no window;
    /// the kit state follows at the next render.
    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        let value = value.into();
        if value != self.value {
            self.value = value;
            self.value_pending = true;
            cx.notify();
        }
    }
    /// Host write: one-way.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if disabled != self.disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }
    /// Host write: the placeholder, which is the input's visible label. One-way.
    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        if placeholder != self.placeholder {
            self.placeholder = placeholder;
            self.placeholder_pending = true;
            cx.notify();
        }
    }
    fn sync_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.value_pending = false;
        if self.input.read(cx).value() != self.value {
            let value = self.value.clone();
            self.input
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        if std::mem::take(&mut self.placeholder_pending) {
            let placeholder = self.placeholder.clone();
            self.input.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx)
            });
        }
    }
}

impl EventEmitter<TextInputEvent> for TextInputControl {}

impl Observe for TextInputControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: Some(Value::Str(self.value.clone())),
            label: (!self.placeholder.is_empty()).then(|| self.placeholder.as_ref().into()),
            disabled: Some(self.disabled),
            ..ControlSnapshot::default()
        }
    }
}

impl Drive for TextInputControl {
    type Action = TextInputAction;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<TextInputAction, ActionError> {
        match action.name.as_ref() {
            names::SET_VALUE => match &action.payload {
                Value::Str(text) => Ok(TextInputAction::Set(text.clone())),
                _ => Err(ActionError::Invalid("set_value expects a string")),
            },
            names::FOCUS => Ok(TextInputAction::Focus),
            _ => Err(ActionError::Unsupported),
        }
    }
    fn apply(
        &mut self,
        action: TextInputAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        enabled(self.disabled)?;
        match action {
            TextInputAction::Set(text) => {
                if text != self.value {
                    self.value = text.clone();
                    self.sync_input(window, cx);
                    cx.emit(TextInputEvent::Changed(text, self.identity.source(cx)));
                }
                Ok(Outcome(Value::Str(self.value.clone())))
            }
            TextInputAction::Focus => {
                self.input.update(cx, |state, cx| state.focus(window, cx));
                Ok(Outcome::EMPTY)
            }
        }
    }
}

impl Render for TextInputControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.value_pending || self.placeholder_pending {
            self.sync_input(window, cx);
        }
        div()
            .id(self.identity.element())
            .w_full()
            .child(Input::new(&self.input).disabled(self.disabled))
    }
}

// --- Select ---------------------------------------------------------------------

/// Emitted when a [`SelectControl`] or [`TabsControl`] selection changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionEvent {
    Selected(usize, Source),
}

/// Emitted with the typed value when a [`SelectControl`] selection changes,
/// after the index-based [`SelectionEvent`].
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedValue<T>(pub T, pub Source);

/// Bounds on a [`SelectControl`] value: a store enum or any comparable value.
pub trait SelectValue: Clone + PartialEq + 'static {}
impl<T: Clone + PartialEq + 'static> SelectValue for T {}

/// Builder for [`SelectControl`].
pub struct SelectBuilder<T: SelectValue = SharedString> {
    options: Vec<(T, SharedString)>,
    selected: Option<usize>,
    label: SharedString,
    disabled: bool,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl<T: SelectValue> Default for SelectBuilder<T> {
    fn default() -> Self {
        Self {
            options: Vec::new(),
            selected: None,
            label: SharedString::default(),
            disabled: false,
            path: None,
            name: None,
        }
    }
}

impl SelectBuilder<SharedString> {
    /// Options whose values are their labels.
    pub fn options(mut self, options: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.options = options
            .into_iter()
            .map(|label| {
                let label = label.into();
                (label.clone(), label)
            })
            .collect();
        self
    }
}

impl<T: SelectValue> SelectBuilder<T> {
    /// Typed options as `(value, label)` pairs.
    pub fn values(
        mut self,
        options: impl IntoIterator<Item = (T, impl Into<SharedString>)>,
    ) -> Self {
        self.options = options
            .into_iter()
            .map(|(value, label)| (value, label.into()))
            .collect();
        self
    }
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self
    }
    /// Select the first option equal to `value`.
    pub fn selected_value(mut self, value: &T) -> Self {
        self.selected = self.options.iter().position(|(v, _)| v == value);
        self
    }
    /// Accessible name of the dropdown.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Register on the window's action bus under this inspect path.
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    /// Element id. Without it, the path (if any) names the element; with both, this wins.
    pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<SelectControl<T>> {
        let handle = window.window_handle();
        build(
            "select",
            Capabilities::SELECT,
            self.path,
            self.name,
            window,
            cx,
            |identity, window, cx| {
                let (values, labels): (Vec<T>, Vec<SharedString>) =
                    self.options.into_iter().unzip();
                let selected = self.selected.filter(|&i| i < labels.len());
                let items = labels.clone();
                let state =
                    cx.new(|cx| SelectState::new(items, selected.map(IndexPath::new), window, cx));
                let this: WeakEntity<SelectControl<T>> = cx.entity().downgrade();
                let subscription = App::subscribe(cx, &state, move |state, event, cx| {
                    let SelectEvent::Confirm(Some(label)) = event else {
                        return;
                    };
                    let Some(control) = this.upgrade() else {
                        return;
                    };
                    let (index, current) = {
                        let control = control.read(cx);
                        (
                            control.labels.iter().position(|l| l == label),
                            control.selected,
                        )
                    };
                    let Some(index) = index else {
                        return;
                    };
                    if Some(index) == current {
                        return;
                    }
                    let routed = route_in(
                        handle,
                        &this,
                        action(names::SELECT, Value::Num(index as f64)),
                        Source::Pointer,
                        cx,
                    );
                    if routed.is_err() {
                        handle
                            .update(cx, |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.set_selected_index(current.map(IndexPath::new), window, cx)
                                })
                            })
                            .ok();
                    }
                });
                SelectControl {
                    identity,
                    state,
                    values: values.into(),
                    labels: labels.into(),
                    selected,
                    label: self.label,
                    disabled: self.disabled,
                    pending: false,
                    _subscription: subscription,
                }
            },
        )
    }
}

/// A dropdown select over typed values; `SelectControl` alone selects labels.
/// Actions: `select` (index). Outcome: the selected label.
pub struct SelectControl<T: SelectValue = SharedString> {
    identity: Identity,
    state: Entity<SelectState<Vec<SharedString>>>,
    values: Rc<[T]>,
    labels: Rc<[SharedString]>,
    selected: Option<usize>,
    label: SharedString,
    disabled: bool,
    /// A host write the kit state has not seen yet.
    pending: bool,
    _subscription: Subscription,
}

impl SelectControl<SharedString> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> SelectBuilder<SharedString> {
        SelectBuilder::default()
    }
}

impl<T: SelectValue> SelectControl<T> {
    /// A select over typed `(value, label)` options.
    pub fn typed(
        options: impl IntoIterator<Item = (T, impl Into<SharedString>)>,
    ) -> SelectBuilder<T> {
        SelectBuilder::default().values(options)
    }
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }
    pub fn selected_value(&self) -> Option<&T> {
        self.values.get(self.selected?)
    }
    /// The kit state that renders the dropdown and receives pointer input.
    pub fn state(&self) -> &Entity<SelectState<Vec<SharedString>>> {
        &self.state
    }
    /// Host write: changes the selection without emitting an event. Needs no
    /// window; the kit state follows at the next render.
    pub fn set_selected(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let index = index.filter(|&i| i < self.labels.len());
        if index != self.selected {
            self.selected = index;
            self.pending = true;
            cx.notify();
        }
    }
    /// Host write: selects the first option equal to `value`, or nothing if none
    /// matches. Returns whether one matched.
    pub fn set_selected_value(&mut self, value: &T, cx: &mut Context<Self>) -> bool {
        let index = self.values.iter().position(|v| v == value);
        self.set_selected(index, cx);
        index.is_some()
    }
    /// Host write: one-way.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if disabled != self.disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }
    /// Host write: the accessible name. One-way.
    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        let label = label.into();
        if label != self.label {
            self.label = label;
            cx.notify();
        }
    }
    fn sync_state(&self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.selected.map(IndexPath::new);
        if self.state.read(cx).selected_index(cx) != wanted {
            self.state
                .update(cx, |state, cx| state.set_selected_index(wanted, window, cx));
        }
    }
}

impl<T: SelectValue> Identified for SelectControl<T> {
    fn identity_mut(&mut self) -> &mut Identity {
        &mut self.identity
    }
    fn identity(&self) -> &Identity {
        &self.identity
    }
}

impl<T: SelectValue> SelectControl<T> {
    /// The bus handle, when built with a path.
    pub fn control_id(&self) -> Option<ControlId> {
        self.identity.id
    }
    /// The id of the rendered root element (the path when registered).
    pub fn element_id(&self) -> ElementId {
        self.identity.element()
    }
}

impl<T: SelectValue> EventEmitter<SelectionEvent> for SelectControl<T> {}
impl<T: SelectValue> EventEmitter<SelectedValue<T>> for SelectControl<T> {}

impl<T: SelectValue> Observe for SelectControl<T> {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: self
                .selected
                .and_then(|i| self.labels.get(i))
                .map(|l| Value::Str(l.clone())),
            selected: self.selected.map(|i| Rc::from([i])),
            label: (!self.label.is_empty()).then(|| self.label.as_ref().into()),
            disabled: Some(self.disabled),
            ..ControlSnapshot::default()
        }
    }
}

impl<T: SelectValue> Drive for SelectControl<T> {
    type Action = usize;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<usize, ActionError> {
        if action.name != names::SELECT {
            return Err(ActionError::Unsupported);
        }
        index_payload(action, usize::MAX)
    }
    fn apply(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        enabled(self.disabled)?;
        let label = self
            .labels
            .get(index)
            .cloned()
            .ok_or(ActionError::Invalid("index out of range"))?;
        if self.selected != Some(index) {
            self.selected = Some(index);
            self.sync_state(window, cx);
            let source = self.identity.source(cx);
            cx.emit(SelectionEvent::Selected(index, source));
            if let Some(value) = self.values.get(index) {
                cx.emit(SelectedValue(value.clone(), source));
            }
        }
        Ok(Outcome(Value::Str(label)))
    }
}

impl<T: SelectValue> Render for SelectControl<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.pending) {
            self.sync_state(window, cx);
        }
        let mut select = Select::new(&self.state).disabled(self.disabled);
        if !self.label.is_empty() {
            select = select.accessibility_label(self.label.clone());
        }
        div().id(self.identity.element()).child(select)
    }
}

// --- Tabs -----------------------------------------------------------------------

/// Builder for [`TabsControl`].
#[derive(Default)]
pub struct TabsBuilder {
    labels: Vec<SharedString>,
    selected: usize,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl TabsBuilder {
    pub fn tabs(mut self, labels: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.labels = labels.into_iter().map(Into::into).collect();
        self
    }
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
    /// Register on the window's action bus under this inspect path.
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    /// Element id. Without it, the path (if any) names the element; with both, this wins.
    pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<TabsControl> {
        build(
            "tabs",
            Capabilities::SELECT,
            self.path,
            self.name,
            window,
            cx,
            |identity, _, _| TabsControl {
                identity,
                selected: self.selected.min(self.labels.len().saturating_sub(1)),
                labels: self.labels.into(),
            },
        )
    }
}

/// A tab bar. Actions: `select` (index). Outcome: the selected index.
pub struct TabsControl {
    identity: Identity,
    labels: Rc<[SharedString]>,
    selected: usize,
}

impl TabsControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TabsBuilder {
        TabsBuilder::default()
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    /// Tab `index` renders as element id `index` within [`Self::element_id`].
    pub fn tab_count(&self) -> usize {
        self.labels.len()
    }
    /// Host write: changes the selection without emitting an event.
    pub fn set_selected(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.labels.len() && index != self.selected {
            self.selected = index;
            cx.notify();
        }
    }
}

impl EventEmitter<SelectionEvent> for TabsControl {}

impl Observe for TabsControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: self
                .labels
                .get(self.selected)
                .map(|l| Value::Str(l.clone())),
            selected: Some(Rc::from([self.selected])),
            ..ControlSnapshot::default()
        }
    }
}

impl Drive for TabsControl {
    type Action = usize;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<usize, ActionError> {
        if action.name != names::SELECT {
            return Err(ActionError::Unsupported);
        }
        index_payload(action, usize::MAX)
    }
    fn apply(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        if index >= self.labels.len() {
            return Err(ActionError::Invalid("index out of range"));
        }
        if index != self.selected {
            self.selected = index;
            cx.emit(SelectionEvent::Selected(index, self.identity.source(cx)));
            cx.notify();
        }
        Ok(Outcome(Value::Num(index as f64)))
    }
}

impl Render for TabsControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let identity = &self.identity;
        TabBar::new(identity.element())
            .selected_index(self.selected)
            .children(
                self.labels
                    .iter()
                    .map(|label| Tab::new().label(label.clone())),
            )
            .on_click(move |index, window, cx| {
                route(
                    &this,
                    action(names::SELECT, Value::Num(*index as f64)),
                    Source::Pointer,
                    window,
                    cx,
                )
                .ok();
            })
    }
}

// --- Menu -----------------------------------------------------------------------

/// Emitted when a [`MenuControl`] item is activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEvent {
    Activated(usize, Source),
}

/// Builder for [`MenuControl`].
#[derive(Default)]
pub struct MenuBuilder {
    items: Vec<(SharedString, bool)>,
    path: Option<SharedString>,
    name: Option<SharedString>,
}

impl MenuBuilder {
    pub fn item(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push((label.into(), false));
        self
    }
    pub fn disabled_item(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push((label.into(), true));
        self
    }
    /// Register on the window's action bus under this inspect path.
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    /// Element id. Without it, the path (if any) names the element; with both, this wins.
    pub fn element_id(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<MenuControl> {
        build(
            "menu",
            Capabilities::ACTIVATE,
            self.path,
            self.name,
            window,
            cx,
            |identity, _, _| MenuControl {
                identity,
                items: self.items.into(),
            },
        )
    }
}

/// A list of menu items. Actions: `activate` (index). Outcome: the item index.
pub struct MenuControl {
    identity: Identity,
    items: Rc<[(SharedString, bool)]>,
}

impl MenuControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> MenuBuilder {
        MenuBuilder::default()
    }
    /// The element id of item `index`.
    pub fn item_id(&self, index: usize) -> ElementId {
        self.identity.child(index)
    }
}

impl EventEmitter<MenuEvent> for MenuControl {}

impl Observe for MenuControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot::default()
    }
}

impl Drive for MenuControl {
    type Action = usize;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<usize, ActionError> {
        if action.name != names::ACTIVATE {
            return Err(ActionError::Unsupported);
        }
        index_payload(action, usize::MAX)
    }
    fn apply(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        let (_, disabled) = self
            .items
            .get(index)
            .ok_or(ActionError::Invalid("index out of range"))?;
        enabled(*disabled)?;
        cx.emit(MenuEvent::Activated(index, self.identity.source(cx)));
        Ok(Outcome(Value::Num(index as f64)))
    }
}

impl Render for MenuControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let identity = &self.identity;
        div()
            .id(identity.element())
            .flex()
            .flex_col()
            .children(self.items.iter().enumerate().map(|(i, (label, disabled))| {
                let this = this.clone();
                Button::new(identity.child(i))
                    .ghost()
                    .label(label.clone())
                    .disabled(*disabled)
                    .on_click(move |_, window, cx| {
                        route(
                            &this,
                            action(names::ACTIVATE, Value::Num(i as f64)),
                            Source::Pointer,
                            window,
                            cx,
                        )
                        .ok();
                    })
            }))
    }
}

identified!(
    ButtonControl,
    CheckboxControl,
    SwitchControl,
    SliderControl,
    TextInputControl,
    TabsControl,
    MenuControl
);

#[cfg(test)]
mod tests {
    use super::*;

    fn act(name: &'static str, payload: Value) -> Action {
        action(name, payload)
    }

    #[test]
    fn payloads_decode_and_reject() {
        assert_eq!(
            decode_toggle(&act(names::SET_VALUE, Value::Bool(true))),
            Ok(ToggleAction::Set(true))
        );
        assert!(decode_toggle(&act(names::SET_VALUE, Value::Null)).is_err());
        assert_eq!(
            decode_toggle(&act(names::OPEN, Value::Null)),
            Err(ActionError::Unsupported)
        );
        assert_eq!(
            index_payload(&act(names::SELECT, Value::Num(2.0)), 3),
            Ok(2)
        );
        for bad in [3.0, -1.0, 0.5, f64::NAN] {
            assert!(index_payload(&act(names::SELECT, Value::Num(bad)), 3).is_err());
        }
        assert!(num_payload(&act(names::SET_VALUE, Value::Num(f64::INFINITY))).is_err());
    }
}
