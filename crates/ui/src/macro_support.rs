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
    CheckboxBuilder, CheckboxControl, SelectBuilder, SelectControl, SelectValue, SelectedValue,
    SliderBuilder, SliderControl, SliderControlEvent, SwitchBuilder, SwitchControl,
    TextInputBuilder, TextInputControl, TextInputEvent, ToggleEvent,
};
use guiraffe_core::protocol::Source;

/// The seam between `ui!` bindings and a protocol control entity: how to build it with an
/// initial value, read an edit (and its source) from its typed event, and apply a host write
/// through its non-emitting setter.
pub trait ControlBinding: Render + gpui_kit::EventEmitter<Self::Event> + Sized + 'static {
    type Value: Clone + PartialEq + 'static;
    type Event: 'static;
    type Builder;
    fn builder() -> Self::Builder;
    fn build(
        builder: Self::Builder,
        value: Option<&Self::Value>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::Entity<Self>;
    fn edited(event: &Self::Event) -> Option<(Self::Value, Source)>;
    fn host_set(&mut self, value: &Self::Value, cx: &mut Context<Self>);
}

macro_rules! toggle_binding {
    ($control:ty, $builder:ty) => {
        impl ControlBinding for $control {
            type Value = bool;
            type Event = ToggleEvent;
            type Builder = $builder;
            fn builder() -> $builder {
                <$control>::new()
            }
            fn build(
                builder: $builder,
                value: Option<&bool>,
                window: &mut Window,
                cx: &mut App,
            ) -> gpui_kit::Entity<Self> {
                builder
                    .checked(value.copied().unwrap_or_default())
                    .build(window, cx)
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
toggle_binding!(CheckboxControl, CheckboxBuilder);
toggle_binding!(SwitchControl, SwitchBuilder);

impl ControlBinding for SliderControl {
    type Value = f32;
    type Event = SliderControlEvent;
    type Builder = SliderBuilder;
    fn builder() -> SliderBuilder {
        SliderControl::new()
    }
    fn build(
        builder: SliderBuilder,
        value: Option<&f32>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::Entity<Self> {
        match value {
            Some(value) => builder.value(*value),
            None => builder,
        }
        .build(window, cx)
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
    type Builder = TextInputBuilder;
    fn builder() -> TextInputBuilder {
        TextInputControl::new()
    }
    fn build(
        builder: TextInputBuilder,
        value: Option<&SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::Entity<Self> {
        match value {
            Some(value) => builder.value(value.clone()),
            None => builder,
        }
        .build(window, cx)
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
    type Builder = SelectBuilder<T>;
    fn builder() -> SelectBuilder<T> {
        SelectBuilder::default()
    }
    fn build(
        builder: SelectBuilder<T>,
        value: Option<&T>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::Entity<Self> {
        match value {
            Some(value) => builder.selected_value(value),
            None => builder,
        }
        .build(window, cx)
    }
    fn edited(event: &SelectedValue<T>) -> Option<(T, Source)> {
        Some((event.0.clone(), event.1))
    }
    fn host_set(&mut self, value: &T, cx: &mut Context<Self>) {
        self.set_selected_value(value, cx);
    }
}

struct ControlSite<C> {
    control: gpui_kit::Entity<C>,
    _watch: guiraffe_state::Watch,
    _edits: Option<gpui_kit::Subscription>,
}

/// A protocol control entity bound to a store lens (`SliderControl { value <=> volume }`).
/// Built once per site; host writes go through the control's non-emitting setter and edits
/// come back from its typed event as `Set` events tagged with their source.
pub struct BoundControl<C: ControlBinding, S: Clone + PartialEq + 'static, L> {
    id: ElementId,
    store: Store<S>,
    lens: L,
    two_way: bool,
    configure: Box<dyn FnOnce(C::Builder) -> C::Builder>,
}

pub fn bound_control<C, S, L>(
    id: impl Into<ElementId>,
    store: &Store<S>,
    lens: L,
    two_way: bool,
    configure: impl FnOnce(C::Builder) -> C::Builder + 'static,
) -> BoundControl<C, S, L>
where
    C: ControlBinding,
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = C::Value>,
    C::Value: std::fmt::Debug,
{
    BoundControl {
        id: id.into(),
        store: store.clone(),
        lens,
        two_way,
        configure: Box::new(configure),
    }
}

impl<C, S, L> IntoElement for BoundControl<C, S, L>
where
    C: ControlBinding,
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = C::Value>,
    C::Value: std::fmt::Debug,
{
    type Element = ViewElement<Self>;
    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }
}

impl<C, S, L> RenderOnce for BoundControl<C, S, L>
where
    C: ControlBinding,
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = C::Value>,
    C::Value: std::fmt::Debug,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            store,
            lens,
            two_way,
            configure,
        } = self;
        let site = window.use_keyed_state(id, cx, move |window, cx| {
            let initial = lens.get(&store.state_untracked(cx)).cloned();
            let control = C::build(configure(C::builder()), initial.as_ref(), window, cx);
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
            let edits = two_way.then(|| {
                let store = store.downgrade();
                cx.subscribe(&control, move |_, _, event: &C::Event, cx| {
                    if let (Some((value, source)), Some(store)) =
                        (C::edited(event), store.upgrade())
                    {
                        store.set_from(lens, value, source, cx);
                    }
                })
            });
            ControlSite {
                control,
                _watch: watch,
                _edits: edits,
            }
        });
        AnyView::from(site.read(cx).control.clone())
    }
}
