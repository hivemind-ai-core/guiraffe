use std::{cell::RefCell, panic::Location};

use gpui_kit::{
    AnyElement, AnyView, App, Bounds, Context, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Render, RenderOnce, StyleRefinement,
    ViewElement, Window,
};

/// A layout-transparent scope for the global IDs of the row's children.
struct Keyed {
    key: ElementId,
    child: AnyElement,
    location: Option<&'static Location<'static>>,
}

pub fn keyed<E: IntoElement>(key: impl Into<ElementId>, child: E) -> impl IntoElement {
    let child = child.into_element();
    Keyed {
        key: key.into(),
        location: child.source_location(),
        child: child.into_any(),
    }
}

impl IntoElement for Keyed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Keyed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        self.location
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Move the child into its normal GPUI drawable so it retains its own
        // ID, accessibility node and rendering behavior under this scope.
        let layout = window.with_element_namespace(self.key.clone(), |window| {
            self.child.request_layout(window, cx)
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_namespace(self.key.clone(), |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_namespace(self.key.clone(), |window| self.child.paint(window, cx));
    }
}

struct CacheView<F> {
    body: RefCell<Option<F>>,
}

impl<F, E> Render for CacheView<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.body.get_mut().as_ref().map_or_else(
            || gpui_kit::Empty.into_any_element(),
            |body| body().into_any_element(),
        )
    }
}

struct Cached<F> {
    id: ElementId,
    style: StyleRefinement,
    body: F,
}

#[track_caller]
pub fn cached<F, E>(style: StyleRefinement, body: F) -> impl IntoElement
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    Cached {
        id: ElementId::CodeLocation(*Location::caller()),
        style,
        body,
    }
}

impl<F, E> IntoElement for Cached<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    type Element = ViewElement<Self>;

    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }
}

impl<F, E> RenderOnce for Cached<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let entity = window.use_keyed_state(self.id, cx, |_, _| CacheView {
            body: RefCell::new(None),
        });
        // Supply the latest closure without marking the cache entity dirty.
        *entity.read(cx).body.borrow_mut() = Some(self.body);
        AnyView::from(entity).cached(self.style)
    }
}

// ---- Store binding support (provisional, 0.3) -------------------------------------------

pub use gpui_kit::SharedString;
pub use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, Capabilities, ControlSnapshot, Drive, Observe, names,
};
pub use guiraffe_core::widget_value::Value;
pub use guiraffe_state::{Lens, ListLens, bound_at, bound_list};

use guiraffe_state::{Identity, Lensed, Store};

/// The lens path at the root of a store's state.
pub fn root<S: Clone + PartialEq + Lensed>(_: &Store<S>) -> S::Path<Identity<S>> {
    S::wrap(Identity::new())
}

/// Text for a bound read: one conversion when the value changes, none per frame.
pub fn text<T: BoundText + ?Sized>(value: &T) -> SharedString {
    value.bound_text()
}

/// Values a bound `{<= lens}` can show.
pub trait BoundText {
    fn bound_text(&self) -> SharedString;
}

impl BoundText for SharedString {
    fn bound_text(&self) -> SharedString {
        self.clone()
    }
}

impl BoundText for str {
    fn bound_text(&self) -> SharedString {
        SharedString::from(self.to_owned())
    }
}

impl BoundText for String {
    fn bound_text(&self) -> SharedString {
        SharedString::from(self.clone())
    }
}

macro_rules! display_text {
    ($($t:ty),*) => {$(
        impl BoundText for $t {
            fn bound_text(&self) -> SharedString {
                SharedString::from(self.to_string())
            }
        }
    )*};
}
display_text!(
    bool, char, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

/// A stateful component's entity, as `#[component(stateful)]` generates it.
pub trait Stateful: Sized + 'static {
    type Fields: Clone;
    fn fields(&self) -> &Self::Fields;
    /// Apply new field values: write owned state, or report controlled fields to the caller.
    fn commit(&mut self, next: Self::Fields, window: &mut Window, cx: &mut Context<Self>);
}

/// A weak handle to a stateful component, captured by its event handlers.
pub struct StateHandle<T>(gpui_kit::WeakEntity<T>);

impl<T> Clone for StateHandle<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Stateful> StateHandle<T> {
    pub fn new(entity: gpui_kit::WeakEntity<T>) -> Self {
        Self(entity)
    }

    /// Change the state in place, as `on_click => count += 1` does.
    pub fn mutate(&self, window: &mut Window, cx: &mut App, change: impl FnOnce(&mut T::Fields)) {
        let Some(entity) = self.0.upgrade() else {
            return;
        };
        entity.update(cx, |state, cx| {
            let mut next = state.fields().clone();
            change(&mut next);
            state.commit(next, window, cx);
        });
    }
}

/// Field types a stateful component can expose over the action bus.
pub trait StateValue: Sized {
    fn to_value(&self) -> Value;
    fn from_value(value: &Value) -> Option<Self>;
}

impl StateValue for bool {
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

macro_rules! numeric_state {
    ($($t:ty),*) => {$(
        impl StateValue for $t {
            fn to_value(&self) -> Value {
                Value::Num(*self as f64)
            }
            #[allow(clippy::float_cmp)]
            fn from_value(value: &Value) -> Option<Self> {
                match value {
                    // Reject values the type cannot hold exactly.
                    Value::Num(n) if (*n as $t) as f64 == *n => Some(*n as $t),
                    _ => None,
                }
            }
        }
    )*};
}
numeric_state!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl StateValue for SharedString {
    fn to_value(&self) -> Value {
        Value::Str(self.clone())
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl StateValue for String {
    fn to_value(&self) -> Value {
        Value::Str(self.clone().into())
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Str(s) => Some(s.to_string()),
            _ => None,
        }
    }
}

/// A record snapshot of several state fields.
pub fn record(entries: impl IntoIterator<Item = (SharedString, Value)>) -> Value {
    Value::Record(std::sync::Arc::new(entries.into_iter().collect()))
}

/// One field of a record payload.
pub fn field<'a>(value: &'a Value, name: &'static str) -> Option<&'a Value> {
    match value {
        Value::Record(fields) => fields.get(&SharedString::new_static(name)),
        _ => None,
    }
}

// ---- Bound control entities (`guiraffe_components::controls`) ---------------------------

use guiraffe_components::controls::{
    ButtonBuilder, ButtonControl, ButtonEvent, CheckboxBuilder, CheckboxControl, KnobBuilder,
    KnobControl, NumberFieldBuilder, NumberFieldControl, ParamEvent, SelectBuilder, SelectControl,
    SelectValue, SelectedValue, SliderBuilder, SliderControl, SliderControlEvent, SwitchBuilder,
    SwitchControl, TextInputBuilder, TextInputControl, TextInputEvent, ToggleEvent,
};
use guiraffe_core::protocol::Source;

pub use guiraffe_state::{derived, derived2, derived3, derived4};

/// A protocol control entity `ui!` can mount: how to start and finish its builder, and
/// which focus handle keyboard focus goes to.
pub trait ControlEntity: Render + Sized + 'static {
    type Builder;
    fn builder() -> Self::Builder;
    fn build(builder: Self::Builder, window: &mut Window, cx: &mut App) -> gpui_kit::Entity<Self>;
    /// The handle that takes keyboard focus, for controls that take it.
    fn focus(&self, _cx: &App) -> Option<gpui_kit::FocusHandle> {
        None
    }
}

/// A control entity with a value `<=>` binds: set its initial value on the builder, read an
/// edit (and its source) from its typed event, apply a host write through its non-emitting
/// setter.
pub trait ControlBinding: ControlEntity + gpui_kit::EventEmitter<Self::Event> {
    type Value: Clone + PartialEq + 'static;
    type Event: 'static;
    fn initial(builder: Self::Builder, value: &Self::Value) -> Self::Builder;
    fn edited(event: &Self::Event) -> Option<(Self::Value, Source)>;
    fn host_set(&mut self, value: &Self::Value, cx: &mut Context<Self>);
}

/// A control entity `on_click => Event` listens to: its activation and the source.
pub trait ControlActivation: ControlEntity + gpui_kit::EventEmitter<Self::Activation> {
    type Activation: 'static;
    fn activated(event: &Self::Activation) -> Option<Source>;
}

macro_rules! control_entity {
    ($control:ty, $builder:ty) => {
        impl ControlEntity for $control {
            type Builder = $builder;
            fn builder() -> $builder {
                <$control>::new()
            }
            fn build(
                builder: $builder,
                window: &mut Window,
                cx: &mut App,
            ) -> gpui_kit::Entity<Self> {
                builder.build(window, cx)
            }
        }
    };
}
control_entity!(ButtonControl, ButtonBuilder);
control_entity!(CheckboxControl, CheckboxBuilder);
control_entity!(SwitchControl, SwitchBuilder);
control_entity!(SliderControl, SliderBuilder);

impl ControlEntity for TextInputControl {
    type Builder = TextInputBuilder;
    fn builder() -> TextInputBuilder {
        TextInputControl::new()
    }
    fn build(
        builder: TextInputBuilder,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::Entity<Self> {
        builder.build(window, cx)
    }
    fn focus(&self, cx: &App) -> Option<gpui_kit::FocusHandle> {
        Some(gpui_kit::Focusable::focus_handle(self.input().read(cx), cx))
    }
}

impl<T: SelectValue> ControlEntity for SelectControl<T> {
    type Builder = SelectBuilder<T>;
    fn builder() -> SelectBuilder<T> {
        SelectBuilder::default()
    }
    fn build(
        builder: SelectBuilder<T>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::Entity<Self> {
        builder.build(window, cx)
    }
}

macro_rules! param_entity {
    ($control:ty, $builder:ty) => {
        impl ControlEntity for $control {
            type Builder = $builder;
            fn builder() -> $builder {
                <$control>::new()
            }
            fn build(
                builder: $builder,
                window: &mut Window,
                cx: &mut App,
            ) -> gpui_kit::Entity<Self> {
                builder.build(window, cx)
            }
            fn focus(&self, cx: &App) -> Option<gpui_kit::FocusHandle> {
                Some(gpui_kit::Focusable::focus_handle(self, cx))
            }
        }

        impl ControlBinding for $control {
            type Value = f64;
            type Event = ParamEvent;
            fn initial(builder: $builder, value: &f64) -> $builder {
                builder.value(*value)
            }
            fn edited(event: &ParamEvent) -> Option<(f64, Source)> {
                let ParamEvent::Changed(value, source) = event;
                Some((*value, *source))
            }
            fn host_set(&mut self, value: &f64, cx: &mut Context<Self>) {
                self.set_value(*value, cx);
            }
        }
    };
}
param_entity!(KnobControl, KnobBuilder);
param_entity!(NumberFieldControl, NumberFieldBuilder);

impl ControlActivation for ButtonControl {
    type Activation = ButtonEvent;
    fn activated(event: &ButtonEvent) -> Option<Source> {
        let ButtonEvent::Activated(source) = event;
        Some(*source)
    }
}

macro_rules! toggle_binding {
    ($control:ty) => {
        impl ControlBinding for $control {
            type Value = bool;
            type Event = ToggleEvent;
            fn initial(builder: Self::Builder, value: &bool) -> Self::Builder {
                builder.checked(*value)
            }
            fn edited(event: &ToggleEvent) -> Option<(bool, Source)> {
                let ToggleEvent::Changed(checked, source) = event;
                Some((*checked, *source))
            }
            fn host_set(&mut self, value: &bool, cx: &mut Context<Self>) {
                self.set_checked(*value, cx);
            }
        }
    };
}
toggle_binding!(CheckboxControl);
toggle_binding!(SwitchControl);

impl ControlBinding for SliderControl {
    type Value = f32;
    type Event = SliderControlEvent;
    fn initial(builder: SliderBuilder, value: &f32) -> SliderBuilder {
        builder.value(*value)
    }
    fn edited(event: &SliderControlEvent) -> Option<(f32, Source)> {
        let SliderControlEvent::Changed(value, source) = event;
        Some((*value, *source))
    }
    fn host_set(&mut self, value: &f32, cx: &mut Context<Self>) {
        self.set_value(*value, cx);
    }
}

impl ControlBinding for TextInputControl {
    type Value = SharedString;
    type Event = TextInputEvent;
    fn initial(builder: TextInputBuilder, value: &SharedString) -> TextInputBuilder {
        builder.value(value.clone())
    }
    fn edited(event: &TextInputEvent) -> Option<(SharedString, Source)> {
        let TextInputEvent::Changed(value, source) = event;
        Some((value.clone(), *source))
    }
    fn host_set(&mut self, value: &SharedString, cx: &mut Context<Self>) {
        self.set_value(value.clone(), cx);
    }
}

/// A select binds the option's value: `SelectControl { value <=> theme  .values([...]) }`.
impl<T: SelectValue> ControlBinding for SelectControl<T> {
    type Value = T;
    type Event = SelectedValue<T>;
    fn initial(builder: SelectBuilder<T>, value: &T) -> SelectBuilder<T> {
        builder.selected_value(value)
    }
    fn edited(event: &SelectedValue<T>) -> Option<(T, Source)> {
        Some((event.0.clone(), event.1))
    }
    fn host_set(&mut self, value: &T, cx: &mut Context<Self>) {
        self.set_selected_value(value, cx);
    }
}

/// A handle to a control entity mounted by `ui!`, for focus, tests and direct calls:
/// `TextInputControl { value <=> reason  handle: self.reason.clone() }`. Create it once
/// (in the view's constructor); it fills when the control mounts.
pub struct ControlHandle<C>(std::rc::Rc<std::cell::RefCell<Option<gpui_kit::WeakEntity<C>>>>);

impl<C> Clone for ControlHandle<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C> Default for ControlHandle<C> {
    fn default() -> Self {
        Self(std::rc::Rc::default())
    }
}

impl<C: ControlEntity> ControlHandle<C> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The control, once mounted.
    pub fn entity(&self) -> Option<gpui_kit::Entity<C>> {
        self.0
            .borrow()
            .as_ref()
            .and_then(gpui_kit::WeakEntity::upgrade)
    }

    /// The handle that takes keyboard focus, once mounted, for controls that take it.
    pub fn focus_handle(&self, cx: &App) -> Option<gpui_kit::FocusHandle> {
        self.entity()?.read(cx).focus(cx)
    }

    fn set(&self, entity: &gpui_kit::Entity<C>) {
        *self.0.borrow_mut() = Some(entity.downgrade());
    }
}

type Setup<C, S> = Box<dyn FnOnce(&gpui_kit::Entity<C>, &Store<S>, &mut App) -> SetupParts<C>>;
type Seed<C, S> =
    Box<dyn FnOnce(<C as ControlEntity>::Builder, &S) -> <C as ControlEntity>::Builder>;
type Configure<C> = Box<dyn FnOnce(<C as ControlEntity>::Builder) -> <C as ControlEntity>::Builder>;

/// What a site keeps alive, and how its first render seeds the builder.
#[derive(Default)]
struct SetupParts<C> {
    _watch: Option<guiraffe_state::Watch>,
    _subscription: Option<gpui_kit::Subscription>,
    _control: std::marker::PhantomData<C>,
}

struct ControlState<C> {
    control: gpui_kit::Entity<C>,
    _parts: Vec<SetupParts<C>>,
}

/// A protocol control entity mounted by `ui!`, with its binding, `on_click` and handle.
/// Built once per site; host writes go through the control's non-emitting setter, edits come
/// back from its typed event as `Set` events, and activations as events, tagged with their
/// source.
pub struct ControlSite<C: ControlEntity, S: Clone + PartialEq + 'static> {
    id: ElementId,
    store: Store<S>,
    configure: Configure<C>,
    initial: Vec<Seed<C, S>>,
    setup: Vec<Setup<C, S>>,
    handle: Option<ControlHandle<C>>,
}

pub fn control_site<C, S>(
    id: impl Into<ElementId>,
    store: &Store<S>,
    configure: impl FnOnce(C::Builder) -> C::Builder + 'static,
) -> ControlSite<C, S>
where
    C: ControlEntity,
    S: Clone + PartialEq + 'static,
{
    ControlSite {
        id: id.into(),
        store: store.clone(),
        configure: Box::new(configure),
        initial: Vec::new(),
        setup: Vec::new(),
        handle: None,
    }
}

impl<C: ControlEntity, S: Clone + PartialEq + 'static> ControlSite<C, S> {
    /// Bind the control's value to `lens`; two-way also writes its edits back.
    pub fn bind<L>(mut self, lens: L, two_way: bool) -> Self
    where
        C: ControlBinding,
        L: Lens<Root = S, Target = C::Value>,
        C::Value: std::fmt::Debug,
    {
        self.initial
            .push(Box::new(move |builder, state| match lens.get(state) {
                Some(value) => C::initial(builder, value),
                None => builder,
            }));
        self.setup.push(Box::new(move |control, store, cx| {
            let weak = control.downgrade();
            let watch = store.watch(
                lens,
                move |value, cx| {
                    if let Some(control) = weak.upgrade() {
                        control.update(cx, |control, cx| control.host_set(value, cx));
                    }
                },
                cx,
            );
            let subscription = two_way.then(|| {
                let store = store.downgrade();
                cx.subscribe(control, move |_, event: &C::Event, cx| {
                    if let (Some((value, source)), Some(store)) =
                        (C::edited(event), store.upgrade())
                    {
                        store.set_from(lens, value, source, cx);
                    }
                })
            });
            SetupParts {
                _watch: Some(watch),
                _subscription: subscription,
                _control: std::marker::PhantomData,
            }
        }));
        self
    }

    /// Run `f` with the store and the source each time the control is activated.
    pub fn on_activate(mut self, f: impl Fn(&Store<S>, Source, &mut App) + 'static) -> Self
    where
        C: ControlActivation,
    {
        self.setup.push(Box::new(move |control, store, cx| {
            let store = store.downgrade();
            let subscription = cx.subscribe(control, move |_, event: &C::Activation, cx| {
                if let (Some(source), Some(store)) = (C::activated(event), store.upgrade()) {
                    f(&store, source, cx);
                }
            });
            SetupParts {
                _watch: None,
                _subscription: Some(subscription),
                _control: std::marker::PhantomData,
            }
        }));
        self
    }

    /// Fill `handle` with the control once it mounts.
    pub fn handle(mut self, handle: ControlHandle<C>) -> Self {
        self.handle = Some(handle);
        self
    }
}

impl<C: ControlEntity, S: Clone + PartialEq + 'static> IntoElement for ControlSite<C, S> {
    type Element = ViewElement<Self>;
    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }
}

impl<C: ControlEntity, S: Clone + PartialEq + 'static> RenderOnce for ControlSite<C, S> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            store,
            configure,
            initial,
            setup,
            handle,
        } = self;
        let site = window.use_keyed_state(id, cx, move |window, cx| {
            let state = store.state_untracked(cx);
            let builder = initial
                .into_iter()
                .fold(configure(C::builder()), |builder, seed| {
                    seed(builder, &state)
                });
            let control = C::build(builder, window, cx);
            let parts = setup
                .into_iter()
                .map(|setup| setup(&control, &store, cx))
                .collect();
            ControlState {
                control,
                _parts: parts,
            }
        });
        let control = site.read(cx).control.clone();
        if let Some(handle) = handle {
            handle.set(&control);
        }
        AnyView::from(control)
    }
}
