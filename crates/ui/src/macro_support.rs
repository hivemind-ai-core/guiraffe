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
