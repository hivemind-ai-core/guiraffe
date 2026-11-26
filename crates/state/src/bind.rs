//! Store binding for controls (provisional, 0.3).
//!
//! A bound control is a small view that holds the last value of each lens it reads. A store
//! write that dirties one of those lenses compares the lens against that value and, only if it
//! changed, updates the view and notifies it once per frame. Nothing else re-renders; the
//! control's parent does not. A bound edit is a [`Set`] event dispatched into the store, so
//! undo, interceptors and devtools see it, tagged with its [`Source`].
//!
//! `ui!` generates these calls; [`bound`] is the builder-API entry point.

use std::{any::Any, fmt, panic::Location, rc::Rc};

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, ElementId, IntoElement, Render, RenderOnce,
    WeakEntity, Window,
};
use guiraffe_core::protocol::{ActionBus, Source};

use crate::{Event, Fx, Store, lens::Lens, store::Slot};

/// Dispatch `value` into `lens`: a typed, undoable edit with its source.
pub struct Set<L: Lens> {
    pub lens: L,
    pub value: L::Target,
    pub source: Source,
    pub undoable: bool,
}

impl<L: Lens> Set<L> {
    /// A host write (`Source::Host`), recorded for undo unless the lens says otherwise.
    pub fn host(lens: L, value: L::Target) -> Self {
        Self {
            lens,
            value,
            source: Source::Host,
            undoable: lens.undoable(),
        }
    }
}

impl<L: Lens> fmt::Debug for Set<L>
where
    L::Target: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Set({} = {:?})", self.lens.name(), self.value)
    }
}

impl<L: Lens> Event<L::Root> for Set<L>
where
    L::Root: Clone + PartialEq,
    L::Target: Clone + PartialEq + fmt::Debug,
{
    fn handle(self, fx: &mut Fx<'_, L::Root>) {
        fx.set_at(self.lens, self.value);
    }
    fn undoable(&self) -> bool {
        self.undoable
    }
    fn source(&self) -> Source {
        self.source
    }
}

/// An event tagged with the source of the gesture or bus action that caused it.
pub struct Sourced<E> {
    pub event: E,
    pub source: Source,
}

impl<E: fmt::Debug> fmt::Debug for Sourced<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.event.fmt(f)
    }
}

impl<S, E: Event<S>> Event<S> for Sourced<E> {
    fn handle(self, fx: &mut Fx<'_, S>) {
        self.event.handle(fx);
    }
    fn undoable(&self) -> bool {
        self.event.undoable()
    }
    fn compare_state(&self) -> bool {
        self.event.compare_state()
    }
    fn source(&self) -> Source {
        self.source
    }
}

impl<S: Clone + PartialEq + 'static> Store<S> {
    /// Dispatch an event tagged with `source`, as a control's typed event reports it.
    pub fn dispatch_from(&self, event: impl Event<S>, source: Source, cx: &mut App) {
        self.dispatch_now(Sourced { event, source }, cx);
    }

    /// Dispatch an event a control raised (`ui!`'s `on_x => Event`), tagged with the bus
    /// source while a control applies a bus action and [`Source::Pointer`] otherwise.
    pub fn dispatch_from_control(&self, event: impl Event<S>, cx: &mut App) {
        let source = ActionBus::current_source(cx).unwrap_or(Source::Pointer);
        self.dispatch_now(Sourced { event, source }, cx);
    }

    /// A host write through a lens ([`Source::Host`]). A bound control shows the new value and
    /// never reports it back as a user edit.
    pub fn set<L>(&self, lens: L, value: L::Target, cx: &mut App)
    where
        L: Lens<Root = S>,
        L::Target: Clone + PartialEq + fmt::Debug,
    {
        self.set_from(lens, value, Source::Host, cx);
    }

    /// A write through a lens with an explicit source.
    pub fn set_from<L>(&self, lens: L, value: L::Target, source: Source, cx: &mut App)
    where
        L: Lens<Root = S>,
        L::Target: Clone + PartialEq + fmt::Debug,
    {
        self.dispatch_now(
            Set {
                lens,
                value,
                source,
                undoable: lens.undoable(),
            },
            cx,
        );
    }

    /// The edit a bound control makes: tagged with the bus source while a control applies a
    /// bus action, and [`Source::Pointer`] for a direct gesture.
    pub fn set_from_control<L>(&self, lens: L, value: L::Target, cx: &mut App)
    where
        L: Lens<Root = S>,
        L::Target: Clone + PartialEq + fmt::Debug,
    {
        let source = ActionBus::current_source(cx).unwrap_or(Source::Pointer);
        self.set_from(lens, value, source, cx);
    }

    /// Call `changed` with the new value whenever a flush changes what `lens` reads. Only
    /// writes on the lens's path (and unkeyed writes) compare; an unchanged value calls
    /// nothing and allocates nothing. Dropping the returned handle unsubscribes.
    ///
    /// This is how a bound control entity receives host writes: `changed` calls its
    /// non-emitting setter, so the write never comes back as an edit.
    pub fn watch<L>(
        &self,
        lens: L,
        changed: impl Fn(&L::Target, &mut App) + 'static,
        cx: &mut App,
    ) -> Watch
    where
        L: Lens<Root = S>,
        L::Target: Clone + PartialEq,
    {
        let last = lens.get(&self.snapshot(cx)).cloned();
        let slot: Rc<dyn Slot<S>> = Rc::new(WatchSlot {
            lens,
            last: std::cell::RefCell::new(last),
            changed: Box::new(changed),
        });
        let mut chain = Vec::new();
        lens.for_each_key(&mut |key| chain.push(key));
        self.watch_chains(&[chain], slot, cx)
    }

    /// Run `slot` after every flush that dirties one of `chains` (each a lens's
    /// [`Lens::for_each_key`] chain). Dropping the returned handle unregisters.
    pub(crate) fn watch_chains(
        &self,
        chains: &[Vec<u64>],
        slot: Rc<dyn Slot<S>>,
        cx: &mut App,
    ) -> Watch {
        let registrations = if chains.iter().any(Vec::is_empty) || chains.is_empty() {
            vec![self.register_chain(&[], slot, cx)]
        } else {
            chains
                .iter()
                .map(|chain| self.register_chain(chain, slot.clone(), cx))
                .collect()
        };
        Watch(registrations)
    }
}

impl<S, T: Slot<S> + ?Sized> Slot<S> for Rc<T> {
    fn refresh(&self, state: &S, cx: &mut App) {
        (**self).refresh(state, cx);
    }
    fn alive(&self) -> bool {
        (**self).alive()
    }
}

type Changed<T> = Box<dyn Fn(&T, &mut App)>;

struct WatchSlot<L: Lens> {
    lens: L,
    last: std::cell::RefCell<Option<L::Target>>,
    changed: Changed<L::Target>,
}

impl<L: Lens> Slot<L::Root> for WatchSlot<L>
where
    L::Target: Clone + PartialEq,
{
    fn refresh(&self, state: &L::Root, cx: &mut App) {
        let Some(value) = self.lens.get(state) else {
            return;
        };
        {
            let mut last = self.last.borrow_mut();
            match &mut *last {
                Some(old) if old == value => return,
                Some(old) => old.clone_from(value),
                None => *last = Some(value.clone()),
            }
        }
        (self.changed)(value, cx);
    }
    fn alive(&self) -> bool {
        true
    }
}

/// A live store registration. Dropping it unsubscribes.
pub struct Watch(#[allow(dead_code)] Vec<Rc<dyn Any>>);

/// What a binding reads: a lens, or a value derived from one ([`derived`]).
pub trait BindSource: 'static {
    type Root: 'static;
    type Value: Clone + PartialEq + 'static;
    /// Whether the value differs from `cached`.
    fn changed(&self, root: &Self::Root, cached: Option<&Self::Value>) -> bool;
    /// Refresh `cached`, reusing its buffers where the type allows.
    fn store(&self, root: &Self::Root, cached: &mut Option<Self::Value>);
    /// The key chains the value depends on: one per lens it reads.
    fn chains(&self, out: &mut Vec<Vec<u64>>);
}

fn chain_of<L: Lens>(lens: &L) -> Vec<u64> {
    let mut chain = Vec::new();
    lens.for_each_key(&mut |key| chain.push(key));
    chain
}

impl<L: Lens> BindSource for L
where
    L::Target: Clone + PartialEq,
{
    type Root = L::Root;
    type Value = L::Target;
    #[inline]
    fn changed(&self, root: &L::Root, cached: Option<&L::Target>) -> bool {
        // By reference: no clone unless it changed.
        self.get(root) != cached
    }
    fn store(&self, root: &L::Root, cached: &mut Option<L::Target>) {
        match (cached.as_mut(), self.get(root)) {
            (Some(old), Some(value)) if old != value => old.clone_from(value),
            (Some(_), Some(_)) => {}
            (_, value) => *cached = value.cloned(),
        }
    }
    fn chains(&self, out: &mut Vec<Vec<u64>>) {
        out.push(chain_of(self));
    }
}

/// A value computed from what `lens` reads, as `ui!`'s `<= derived(lens, |v| …)` binds it.
/// It is recomputed only when a write marks the lens's path, and a control is notified only
/// when the result changed, coalesced like any binding. `f` must not depend on anything
/// else that changes.
pub struct Derived<L, F> {
    lens: L,
    f: F,
}

/// Bind a value derived from a lens: `derived(Review::selected, move |s| *s == key)`.
pub fn derived<L, F, T>(lens: L, f: F) -> Derived<L, F>
where
    L: Lens,
    F: Fn(&L::Target) -> T + 'static,
    T: Clone + PartialEq + 'static,
{
    Derived { lens, f }
}

impl<L, F, T> BindSource for Derived<L, F>
where
    L: Lens,
    F: Fn(&L::Target) -> T + 'static,
    T: Clone + PartialEq + 'static,
{
    type Root = L::Root;
    type Value = T;
    #[inline]
    fn changed(&self, root: &L::Root, cached: Option<&T>) -> bool {
        match (self.lens.get(root), cached) {
            (Some(input), Some(cached)) => (self.f)(input) != *cached,
            (None, None) => false,
            _ => true,
        }
    }
    fn store(&self, root: &L::Root, cached: &mut Option<T>) {
        *cached = self.lens.get(root).map(&self.f);
    }
    fn chains(&self, out: &mut Vec<Vec<u64>>) {
        out.push(chain_of(&self.lens));
    }
}

/// A value computed from several lenses of one store, as `ui!`'s
/// `<= derived((a, b), |a, b| …)` binds it. Built by [`derived2`], [`derived3`] and
/// [`derived4`]. It is recomputed only when a write marks one of the inputs' paths, and a
/// control is notified only when the result changed. Reading an input never clones it.
pub struct DerivedN<I, F> {
    inputs: I,
    f: F,
}

macro_rules! derived_n {
    ($name:ident; $($input:ident $index:tt $value:ident),+) => {
        /// Bind a value computed from several lenses (see [`DerivedN`]).
        pub fn $name<S: 'static, $($input,)+ F, T>(inputs: ($($input,)+), f: F) -> DerivedN<($($input,)+), F>
        where
            $($input: Lens<Root = S>,)+
            F: Fn($(&$input::Target),+) -> T + 'static,
            T: Clone + PartialEq + 'static,
        {
            DerivedN { inputs, f }
        }

        impl<S: 'static, $($input,)+ F, T> BindSource for DerivedN<($($input,)+), F>
        where
            $($input: Lens<Root = S>,)+
            F: Fn($(&$input::Target),+) -> T + 'static,
            T: Clone + PartialEq + 'static,
        {
            type Root = S;
            type Value = T;
            #[inline]
            fn changed(&self, root: &S, cached: Option<&T>) -> bool {
                let ($(Some($value),)+) = ($(self.inputs.$index.get(root),)+) else {
                    return cached.is_some();
                };
                cached.is_none_or(|cached| (self.f)($($value),+) != *cached)
            }
            fn store(&self, root: &S, cached: &mut Option<T>) {
                *cached = match ($(self.inputs.$index.get(root),)+) {
                    ($(Some($value),)+) => Some((self.f)($($value),+)),
                    _ => None,
                };
            }
            fn chains(&self, out: &mut Vec<Vec<u64>>) {
                $(out.push(chain_of(&self.inputs.$index));)+
            }
        }
    };
}

derived_n!(derived2; A 0 a, B 1 b);
derived_n!(derived3; A 0 a, B 1 b, C 2 c);
derived_n!(derived4; A 0 a, B 1 b, C 2 c, D 3 d);

/// One bound value: its source and the value the control last saw.
pub struct Bind<B: BindSource> {
    source: B,
    value: Option<B::Value>,
}

impl<B: BindSource> Bind<B> {
    fn new(source: B, state: &B::Root) -> Self {
        let mut value = None;
        source.store(state, &mut value);
        Self { source, value }
    }

    /// The current value; `None` while the source reads nothing (a removed item).
    pub fn get(&self) -> Option<&B::Value> {
        self.value.as_ref()
    }

    fn changed(&self, state: &B::Root) -> bool {
        self.source.changed(state, self.value.as_ref())
    }

    fn update(&mut self, state: &B::Root) {
        self.source.store(state, &mut self.value);
    }

    fn chains(&self, out: &mut Vec<Vec<u64>>) {
        self.source.chains(out);
    }
}

impl<L: Lens + BindSource> Bind<L> {
    /// The lens, for a two-way binding's write.
    pub fn lens(&self) -> L {
        self.source
    }
}

/// A tuple of lenses bound by one control.
pub trait Lenses<S>: 'static {
    type Bound: Bindings<S>;
    fn bind(self, state: &S) -> Self::Bound;
}

/// The bound values of one control.
pub trait Bindings<S>: 'static {
    fn changed(&self, state: &S) -> bool;
    fn update(&mut self, state: &S);
    fn chains(&self) -> Vec<Vec<u64>>;
}

macro_rules! tuples {
    ($($name:ident $index:tt),+) => {
        impl<S: 'static, $($name: BindSource<Root = S>),+> Lenses<S> for ($($name,)+)
        {
            type Bound = ($(Bind<$name>,)+);
            fn bind(self, state: &S) -> Self::Bound {
                ($(Bind::new(self.$index, state),)+)
            }
        }

        impl<S: 'static, $($name: BindSource<Root = S>),+> Bindings<S> for ($(Bind<$name>,)+)
        {
            #[inline]
            fn changed(&self, state: &S) -> bool {
                false $(|| self.$index.changed(state))+
            }
            fn update(&mut self, state: &S) {
                $(self.$index.update(state);)+
            }
            fn chains(&self) -> Vec<Vec<u64>> {
                let mut chains = Vec::new();
                $(self.$index.chains(&mut chains);)+
                chains
            }
        }
    };
}

tuples!(A 0);
tuples!(A 0, B 1);
tuples!(A 0, B 1, C 2);
tuples!(A 0, B 1, C 2, D 3);
tuples!(A 0, B 1, C 2, D 3, E 4);
tuples!(A 0, B 1, C 2, D 3, E 4, G 5);
tuples!(A 0, B 1, C 2, D 3, E 4, G 5, H 6);
tuples!(A 0, B 1, C 2, D 3, E 4, G 5, H 6, I 7);

type Body<S, B> = dyn Fn(&B, &Store<S>, &mut Window, &mut App) -> AnyElement;
type BodyProps<S, B> = Option<Rc<Body<S, B>>>;

/// The keyed element state of a bound site, and the props its parent hands the site's view.
///
/// The parent writes `props` as it renders; the view reads them as it renders. On gpui-fast a
/// write while drawing builds the readers again in the same frame, and the parent depends only
/// on this holder, never on the view the Store notifies, so a store edit never rebuilds it.
pub(crate) struct Holder<V, P> {
    pub(crate) view: gpui_kit::Entity<V>,
    pub(crate) props: P,
}

/// The retained view behind one bound control.
pub struct BoundView<S: Clone + PartialEq + 'static, B: Bindings<S>> {
    store: Store<S>,
    bindings: B,
    holder: WeakEntity<Holder<Self, BodyProps<S, B>>>,
    pending: bool,
    renders: usize,
    _watch: Option<Watch>,
}

impl<S: Clone + PartialEq + 'static, B: Bindings<S>> BoundView<S, B> {
    fn new(
        store: Store<S>,
        bindings: B,
        holder: WeakEntity<Holder<Self, BodyProps<S, B>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let slot: Rc<dyn Slot<S>> = Rc::new(BindSlot::<S, B> {
            view: cx.weak_entity(),
        });
        let watch = store.watch_chains(&bindings.chains(), slot, cx);
        Self {
            store,
            bindings,
            holder,
            pending: false,
            renders: 0,
            _watch: Some(watch),
        }
    }

    /// The bound values as of the last flush.
    pub fn bindings(&self) -> &B {
        &self.bindings
    }

    /// How often this control rendered (tests and devtools).
    pub fn renders(&self) -> usize {
        self.renders
    }
}

impl<S: Clone + PartialEq + 'static, B: Bindings<S>> Render for BoundView<S, B> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.pending = false;
        self.renders += 1;
        // Reading the holder makes a parent's re-render (which writes it) rebuild this view in
        // the same frame.
        let body = self
            .holder
            .upgrade()
            .and_then(|holder| holder.read(cx).props.clone());
        match body {
            Some(body) => body(&self.bindings, &self.store, window, cx),
            None => gpui_kit::Empty.into_any_element(),
        }
    }
}

struct BindSlot<S: Clone + PartialEq + 'static, B: Bindings<S>> {
    view: WeakEntity<BoundView<S, B>>,
}

impl<S: Clone + PartialEq + 'static, B: Bindings<S>> Slot<S> for BindSlot<S, B> {
    fn refresh(&self, state: &S, cx: &mut App) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        // Compare first: an update is a write on gpui-fast.
        if !view.read(cx).bindings.changed(state) {
            return;
        }
        view.update(cx, |view, cx| {
            view.bindings.update(state);
            // Coalesce: one notification per control per frame.
            if !view.pending {
                view.pending = true;
                cx.notify();
            }
        });
    }
    fn alive(&self) -> bool {
        self.view.upgrade().is_some()
    }
}

/// A control bound to store lenses, as an element. Built by `ui!`; see [`bound`].
pub struct Bound<S: Clone + PartialEq + 'static, L: Lenses<S>> {
    id: ElementId,
    store: Store<S>,
    lenses: L,
    body: Rc<Body<S, L::Bound>>,
}

/// Bind a control to store lenses with the builder API.
///
/// `body` builds the control from the bound values each time one of them changes. It receives
/// the tuple of [`Bind`]s in lens order and the store for two-way edits. Each call site is one
/// control; inside a loop, give each instance a distinct key (`ui!`'s `for … key`).
///
/// ```ignore
/// bound(&settings, (Settings::font_size,), |(size,), store, _, _| {
///     let Some(size) = size.get() else { return Empty.into_any_element() };
///     Slider::new("font-size").value(*size)
///         .on_change({ let s = store.downgrade(); move |v, _, cx| { /* s.set_from_control(..) */ } })
///         .into_any_element()
/// })
/// ```
#[track_caller]
pub fn bound<S, L>(
    store: &Store<S>,
    lenses: L,
    body: impl Fn(&L::Bound, &Store<S>, &mut Window, &mut App) -> AnyElement + 'static,
) -> Bound<S, L>
where
    S: Clone + PartialEq + 'static,
    L: Lenses<S>,
{
    bound_at(
        ElementId::CodeLocation(*Location::caller()),
        store,
        lenses,
        body,
    )
}

/// [`bound`] with an explicit element id.
pub fn bound_at<S, L>(
    id: impl Into<ElementId>,
    store: &Store<S>,
    lenses: L,
    body: impl Fn(&L::Bound, &Store<S>, &mut Window, &mut App) -> AnyElement + 'static,
) -> Bound<S, L>
where
    S: Clone + PartialEq + 'static,
    L: Lenses<S>,
{
    Bound {
        id: id.into(),
        store: store.clone(),
        lenses,
        body: Rc::new(body),
    }
}

impl<S: Clone + PartialEq + 'static, L: Lenses<S>> IntoElement for Bound<S, L> {
    type Element = gpui_kit::ViewElement<Self>;
    fn into_element(self) -> Self::Element {
        gpui_kit::ViewElement::new(self)
    }
}

impl<S: Clone + PartialEq + 'static, L: Lenses<S>> RenderOnce for Bound<S, L> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            store,
            lenses,
            body,
        } = self;
        let init = store.clone();
        let holder = window.use_keyed_state(id, cx, move |_, cx: &mut Context<Holder<_, _>>| {
            let bindings = lenses.bind(&init.snapshot(cx));
            let weak = cx.weak_entity();
            let view = cx.new(|cx| BoundView::new(init, bindings, weak, cx));
            Holder { view, props: None }
        });
        // The parent rendered: hand the control its new body, in this frame. No notification.
        let view = holder.update(cx, |holder, _| {
            holder.props = Some(body);
            holder.view.clone()
        });
        AnyView::from(view)
    }
}
