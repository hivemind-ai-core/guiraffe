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
    pub fn watch<L>(&self, lens: L, changed: impl Fn(&L::Target, &mut App) + 'static, cx: &mut App) -> Watch
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

struct WatchSlot<L: Lens> {
    lens: L,
    last: std::cell::RefCell<Option<L::Target>>,
    changed: Box<dyn Fn(&L::Target, &mut App)>,
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

/// One bound value: a lens and the value the control last saw.
pub struct Bind<L: Lens> {
    lens: L,
    value: Option<L::Target>,
}

impl<L: Lens> Bind<L>
where
    L::Target: Clone + PartialEq,
{
    fn new(lens: L, state: &L::Root) -> Self {
        Self {
            lens,
            value: lens.get(state).cloned(),
        }
    }

    /// The current value; `None` while the lens reads nothing (a removed item).
    pub fn get(&self) -> Option<&L::Target> {
        self.value.as_ref()
    }

    pub fn lens(&self) -> L {
        self.lens
    }

    fn changed(&self, state: &L::Root) -> bool {
        self.lens.get(state) != self.value.as_ref()
    }

    fn update(&mut self, state: &L::Root) {
        if let Some(value) = self.lens.get(state) {
            // `clone_from` reuses the old value's buffers where the type allows it.
            match &mut self.value {
                Some(old) if old != value => old.clone_from(value),
                Some(_) => {}
                None => self.value = Some(value.clone()),
            }
        } else {
            self.value = None;
        }
    }

    fn chain(&self) -> Vec<u64> {
        let mut chain = Vec::new();
        self.lens.for_each_key(&mut |key| chain.push(key));
        chain
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
        impl<S: 'static, $($name: Lens<Root = S>),+> Lenses<S> for ($($name,)+)
        where $($name::Target: Clone + PartialEq),+
        {
            type Bound = ($(Bind<$name>,)+);
            fn bind(self, state: &S) -> Self::Bound {
                ($(Bind::new(self.$index, state),)+)
            }
        }

        impl<S: 'static, $($name: Lens<Root = S>),+> Bindings<S> for ($(Bind<$name>,)+)
        where $($name::Target: Clone + PartialEq),+
        {
            #[inline]
            fn changed(&self, state: &S) -> bool {
                false $(|| self.$index.changed(state))+
            }
            fn update(&mut self, state: &S) {
                $(self.$index.update(state);)+
            }
            fn chains(&self) -> Vec<Vec<u64>> {
                vec![$(self.$index.chain()),+]
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
type BodyCell<S, B> = Rc<std::cell::RefCell<Option<Rc<Body<S, B>>>>>;

/// Element state for a bound site: never written after creation, so a parent that reads it
/// while rendering does not depend on the control's changes (gpui-fast tracks entity reads).
pub(crate) struct Holder<V, C> {
    pub(crate) view: gpui_kit::Entity<V>,
    pub(crate) cell: C,
    pub(crate) fresh: std::cell::Cell<bool>,
}

/// Notify `views` once drawing is done. On gpui-fast a notification while drawing, or a write,
/// would make the parent depend on the control's own later changes.
pub(crate) fn refresh_later(views: Vec<gpui_kit::EntityId>, cx: &mut App) {
    cx.defer(move |cx| {
        for view in views {
            cx.notify(view);
        }
    });
}

impl<V: 'static, C> Holder<V, C> {
    /// Whether the parent re-rendered an already mounted site, so its views must render
    /// again with the new body. Never on mount (they render anyway) or on upstream GPUI
    /// (they render with the parent).
    pub(crate) fn remounted(&self) -> bool {
        !self.fresh.replace(false) && gpui_kit::GPUI_FAST
    }
}

/// The retained view behind one bound control.
pub struct BoundView<S: Clone + PartialEq + 'static, B: Bindings<S>> {
    store: Store<S>,
    bindings: B,
    body: BodyCell<S, B>,
    pending: bool,
    renders: usize,
    _watch: Option<Watch>,
}

impl<S: Clone + PartialEq + 'static, B: Bindings<S>> BoundView<S, B> {
    fn new(store: Store<S>, bindings: B, body: BodyCell<S, B>, cx: &mut Context<Self>) -> Self {
        let slot: Rc<dyn Slot<S>> = Rc::new(BindSlot::<S, B> {
            view: cx.weak_entity(),
        });
        let watch = store.watch_chains(&bindings.chains(), slot, cx);
        Self {
            store,
            bindings,
            body,
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
        let body = self.body.borrow().clone();
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
            let cell: BodyCell<S, L::Bound> = Rc::default();
            let view = cx.new(|cx| BoundView::new(init, bindings, cell.clone(), cx));
            Holder {
                view,
                cell,
                fresh: std::cell::Cell::new(true),
            }
        });
        let holder = holder.read(cx);
        *holder.cell.borrow_mut() = Some(body);
        let view = holder.view.clone();
        if holder.remounted() {
            refresh_later(vec![view.entity_id()], cx);
        }
        AnyView::from(view)
    }
}
