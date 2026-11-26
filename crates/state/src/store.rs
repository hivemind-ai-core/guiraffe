//! The GPUI side: a [`Store`] entity that batches dispatches, runs effects, and keeps
//! memoized [`Selected`] values that notify *only* the views whose selection changed.
//!
//! Robustness rules:
//! - `processing`/`scheduled` live outside the entity and are reset by a drop guard, so a
//!   panic in a handler, selector or effect never leaves the store wedged.
//! - One cascade budget is shared by a whole flush, so loops that go through effect handlers
//!   and back are reported instead of hanging the UI.
//! - Timers hold a weak handle; a dropped store's pending events are discarded.
//! - Effect handlers receive `&Store`; capture [`WeakStore`] (never `Store`) inside them.

use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
    sync::Arc,
};

use gpui_kit::{App, AppContext, Context, Entity, Subscription, Task, WeakEntity};

use crate::engine::{
    Coeffects, DirtyKeys, DispatchLater, Effect, Engine, EngineError, Event, Interceptor, Queued,
    UndoEvent,
};
use crate::history::HistoryLimits;
use guiraffe_core::protocol::ActionBus;

type EffectHandler<S> = Rc<dyn Fn(Box<dyn std::any::Any>, &Store<S>, &mut App)>;
type ErrorHandler = Rc<dyn Fn(&EngineError, &mut App)>;

pub(crate) trait Slot<S> {
    /// Recompute against `state`, notifying the backing entity if the value changed.
    fn refresh(&self, state: &S, cx: &mut App);
    fn alive(&self) -> bool;
}

struct Slots<S> {
    entries: Rc<Vec<Weak<dyn Slot<S>>>>,
    live: usize,
}

type SlotGroup<S> = Rc<RefCell<Slots<S>>>;
/// A keyed group and its place in the key tree: a write that dirties a key's subtree
/// refreshes its children's groups too.
struct KeyNode<S> {
    slots: SlotGroup<S>,
    parent: Option<u64>,
    children: HashSet<u64>,
}

type GroupMap<S> = HashMap<u64, KeyNode<S>>;

/// Remove `key` and any ancestors left without selections or children.
fn prune<S>(groups: &mut GroupMap<S>, mut key: u64) {
    loop {
        let Some(node) = groups.get(&key) else { return };
        if node.slots.borrow().live > 0 || !node.children.is_empty() {
            return;
        }
        let parent = node.parent;
        groups.remove(&key);
        let Some(parent) = parent else { return };
        if let Some(node) = groups.get_mut(&parent) {
            node.children.remove(&key);
        }
        key = parent;
    }
}

impl<S> Default for Slots<S> {
    fn default() -> Self {
        Self {
            entries: Rc::default(),
            live: 0,
        }
    }
}

// The selection owns this slot. The Store holds weak entries in registration order.
struct RegisteredSlot<S, T> {
    slot: T,
    slots: Weak<RefCell<Slots<S>>>,
    keyed: Option<(u64, Weak<RefCell<GroupMap<S>>>)>,
}

impl<S, T: Slot<S>> Slot<S> for RegisteredSlot<S, T> {
    fn refresh(&self, state: &S, cx: &mut App) {
        self.slot.refresh(state, cx);
    }

    fn alive(&self) -> bool {
        self.slot.alive()
    }
}

impl<S, T> Drop for RegisteredSlot<S, T> {
    fn drop(&mut self) {
        if let Some(slots) = self.slots.upgrade() {
            let mut slots = slots.borrow_mut();
            slots.live -= 1;
            // Each sweep removes at least half the entries, so its cost can be
            // charged to the removed slots. Weak entries never own selectors.
            if slots.entries.len() >= 2 * slots.live {
                Rc::make_mut(&mut slots.entries).retain(|slot| slot.strong_count() != 0);
            }
            let empty = slots.live == 0;
            drop(slots);
            if empty
                && let Some((key, groups)) = &self.keyed
                && let Some(groups) = groups.upgrade()
            {
                prune(&mut groups.borrow_mut(), *key);
            }
        }
    }
}

struct Inner<S: Clone + PartialEq + 'static> {
    engine: Engine<S>,
    cofx: Rc<RefCell<Coeffects>>,
    effects: HashMap<TypeId, EffectHandler<S>>,
    on_error: Option<ErrorHandler>,
    slots: SlotGroup<S>,
    keyed: Rc<RefCell<GroupMap<S>>>,
    debounce: HashMap<&'static str, (u64, Queued<S>, Task<()>)>,
    next_timer: u64,
    stats: StoreStats,
    /// The `ActionBus::dispatch_batch` whose undo group the engine has open.
    batch: Option<u64>,
}

/// An event-driven state container. Cheap to clone (it is a handle).
pub struct Store<S: Clone + PartialEq + 'static> {
    inner: Entity<Inner<S>>,
    scheduled: Rc<Cell<bool>>,
    processing: Rc<Cell<bool>>,
    // The flushed state, readable without reading the entity, so that a parent mounting a
    // bound control does not come to depend on every Store write (gpui-fast tracks reads).
    snapshot: Rc<RefCell<Option<Arc<S>>>>,
    // The entity's registries, shared so registering a watch never reads the entity.
    slots: SlotGroup<S>,
    keyed: Rc<RefCell<GroupMap<S>>>,
}

impl<S: Clone + PartialEq + 'static> Clone for Store<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            scheduled: self.scheduled.clone(),
            processing: self.processing.clone(),
            snapshot: self.snapshot.clone(),
            slots: self.slots.clone(),
            keyed: self.keyed.clone(),
        }
    }
}

/// A weak handle for capturing in effect handlers, timers and long-lived closures.
pub struct WeakStore<S: Clone + PartialEq + 'static> {
    inner: WeakEntity<Inner<S>>,
    scheduled: Rc<Cell<bool>>,
    processing: Rc<Cell<bool>>,
    snapshot: Rc<RefCell<Option<Arc<S>>>>,
    // The entity's registries, shared so registering a watch never reads the entity.
    slots: SlotGroup<S>,
    keyed: Rc<RefCell<GroupMap<S>>>,
}

impl<S: Clone + PartialEq + 'static> Clone for WeakStore<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            scheduled: self.scheduled.clone(),
            processing: self.processing.clone(),
            snapshot: self.snapshot.clone(),
            slots: self.slots.clone(),
            keyed: self.keyed.clone(),
        }
    }
}

impl<S: Clone + PartialEq + 'static> WeakStore<S> {
    pub fn upgrade(&self) -> Option<Store<S>> {
        Some(Store {
            inner: self.inner.upgrade()?,
            scheduled: self.scheduled.clone(),
            processing: self.processing.clone(),
            snapshot: self.snapshot.clone(),
            slots: self.slots.clone(),
            keyed: self.keyed.clone(),
        })
    }
}

/// Diagnostics for devtools and tests.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StoreStats {
    pub events_processed: u64,
    pub flushes: u64,
    pub selections: usize,
    /// Type names of effects with no registered handler (most recent last, capped).
    pub unhandled_effects: Vec<&'static str>,
    pub last_error: Option<EngineError>,
}

/// Resets a flag when dropped, including during unwinding.
struct FlagGuard(Rc<Cell<bool>>);

impl Drop for FlagGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl<S: Clone + PartialEq + 'static> Store<S> {
    pub fn new(state: S, cx: &mut App) -> Self {
        let engine = Engine::new(state);
        let snapshot = Rc::new(RefCell::new(Some(engine.state().clone())));
        let slots: SlotGroup<S> = Rc::default();
        let keyed: Rc<RefCell<GroupMap<S>>> = Rc::default();
        let (shared_slots, shared_keyed) = (slots.clone(), keyed.clone());
        let inner = cx.new(|_| Inner {
            engine,
            cofx: Rc::new(RefCell::new(Coeffects::default())),
            effects: HashMap::new(),
            on_error: None,
            slots: shared_slots,
            keyed: shared_keyed,
            debounce: HashMap::new(),
            next_timer: 0,
            stats: StoreStats::default(),
            batch: None,
        });
        Self {
            inner,
            scheduled: Rc::new(Cell::new(false)),
            processing: Rc::new(Cell::new(false)),
            snapshot,
            slots,
            keyed,
        }
    }

    /// The flushed state without reading the Store entity (no render dependency).
    /// The flushed state, read without making the current view depend on the Store entity.
    /// Use it when mounting a bound control inside a parent's render; views that should
    /// re-render on change use selections or bindings instead.
    pub fn state_untracked(&self, cx: &App) -> Arc<S> {
        self.snapshot(cx)
    }

    /// Released while a flush runs, so in-place writes find the state unshared.
    pub(crate) fn snapshot(&self, cx: &App) -> Arc<S> {
        match &*self.snapshot.borrow() {
            Some(state) => state.clone(),
            None => self.inner.read(cx).engine.state().clone(),
        }
    }

    pub fn downgrade(&self) -> WeakStore<S> {
        WeakStore {
            inner: self.inner.downgrade(),
            scheduled: self.scheduled.clone(),
            processing: self.processing.clone(),
            snapshot: self.snapshot.clone(),
            slots: self.slots.clone(),
            keyed: self.keyed.clone(),
        }
    }

    /// Current state snapshot (cheap `Arc` clone). Views should prefer [`Store::select`].
    /// Between a dispatch and its flush this is the last flushed state.
    pub fn state(&self, cx: &App) -> Arc<S> {
        self.inner.read(cx).engine.state().clone()
    }

    /// Whole-state undo within `limits` (see [`Engine::enable_undo`] for the caveat).
    pub fn enable_undo(&self, limits: impl Into<HistoryLimits>, cx: &mut App) {
        let limits = limits.into();
        self.inner.update(cx, |i, _| i.engine.enable_undo(limits));
    }

    /// Whole-state undo with a per-step weight estimate (see [`Engine::enable_undo_weighed`]).
    pub fn enable_undo_weighed(
        &self,
        limits: impl Into<HistoryLimits>,
        weigh: impl Fn(&S) -> u64 + 'static,
        cx: &mut App,
    ) {
        let limits = limits.into();
        self.inner
            .update(cx, |i, _| i.engine.enable_undo_weighed(limits, weigh));
    }

    /// Undo scoped to part of the state (see [`Engine::enable_undo_lens`]).
    pub fn enable_undo_lens<U: Clone + 'static>(
        &self,
        limits: impl Into<HistoryLimits>,
        get: impl Fn(&S) -> U + 'static,
        set: impl Fn(&mut S, U) + 'static,
        cx: &mut App,
    ) {
        let limits = limits.into();
        self.inner
            .update(cx, |i, _| i.engine.enable_undo_lens(limits, get, set));
    }

    /// Scoped undo with a per-step weight estimate (see [`Engine::enable_undo_lens_weighed`]).
    pub fn enable_undo_lens_weighed<U: Clone + 'static>(
        &self,
        limits: impl Into<HistoryLimits>,
        get: impl Fn(&S) -> U + 'static,
        set: impl Fn(&mut S, U) + 'static,
        weigh: impl Fn(&U) -> u64 + 'static,
        cx: &mut App,
    ) {
        let limits = limits.into();
        self.inner.update(cx, |i, _| {
            i.engine.enable_undo_lens_weighed(limits, get, set, weigh)
        });
    }

    /// Change the history limits, keeping the steps that still fit.
    pub fn set_undo_limits(&self, limits: impl Into<HistoryLimits>, cx: &mut App) {
        let limits = limits.into();
        self.inner
            .update(cx, |i, _| i.engine.set_undo_limits(limits));
    }

    pub fn undo_limits(&self, cx: &App) -> HistoryLimits {
        self.inner.read(cx).engine.undo_limits()
    }

    /// Start an undo group (see [`Engine::begin_undo_group`]). Events dispatched before the
    /// matching [`Store::end_undo_group`] are flushed first, so they stay in this group.
    pub fn begin_undo_group(&self, cx: &mut App) {
        self.flush(cx);
        self.inner.update(cx, |i, _| i.engine.begin_undo_group());
    }

    /// End the undo group, flushing its pending events into it first.
    pub fn end_undo_group(&self, cx: &mut App) {
        self.flush(cx);
        self.inner.update(cx, |i, _| i.engine.end_undo_group());
    }

    pub fn can_undo(&self, cx: &App) -> bool {
        self.inner.read(cx).engine.can_undo()
    }

    pub fn can_redo(&self, cx: &App) -> bool {
        self.inner.read(cx).engine.can_redo()
    }

    pub fn add_interceptor(&self, i: impl Interceptor<S>, cx: &mut App) {
        self.inner
            .update(cx, |inner, _| inner.engine.add_interceptor(i));
    }

    pub fn set_cascade_limit(&self, limit: usize, cx: &mut App) {
        self.inner
            .update(cx, |i, _| i.engine.cascade_limit = limit.max(1));
    }

    /// Provide a coeffect value readable by handlers via `fx.cofx::<T>()`.
    pub fn provide<T: 'static>(&self, value: T, cx: &mut App) {
        self.inner
            .update(cx, |i, _| i.cofx.borrow_mut().provide(value));
    }

    /// Register the executor for effects of type `E` (replaces an existing one).
    /// Use the `&Store` argument (or a [`WeakStore`]) rather than capturing a `Store`,
    /// which would keep the store alive forever.
    pub fn on_effect<E: 'static>(
        &self,
        cx: &mut App,
        f: impl Fn(E, &Store<S>, &mut App) + 'static,
    ) {
        let handler: EffectHandler<S> = Rc::new(move |boxed, store, cx| {
            if let Ok(e) = boxed.downcast::<E>() {
                f(*e, store, cx);
            }
        });
        self.inner.update(cx, |i, _| {
            i.effects.insert(TypeId::of::<E>(), handler);
        });
    }

    /// Called when a flush reports an error (e.g. a dispatch loop).
    pub fn on_error(&self, cx: &mut App, f: impl Fn(&EngineError, &mut App) + 'static) {
        self.inner.update(cx, |i, _| i.on_error = Some(Rc::new(f)));
    }

    /// Queue an event. All events dispatched in the same tick are processed together
    /// (one refresh of selections, so views re-render at most once).
    pub fn dispatch(&self, event: impl Event<S>, cx: &mut App) {
        self.sync_batch(cx);
        self.inner.update(cx, |i, _| i.engine.dispatch(event));
        self.schedule(cx);
    }

    /// Process a typed event immediately when idle, without boxing it.
    ///
    /// When the store already has a scheduled batch or is processing effects,
    /// the event enters that batch in FIFO order.
    pub fn dispatch_now(&self, event: impl Event<S>, cx: &mut App) {
        self.sync_batch(cx);
        if self.scheduled.get() || self.processing.get() || self.inner.read(cx).engine.has_pending()
        {
            self.dispatch(event, cx);
        } else {
            self.flush_with_first(Some(event), cx);
        }
    }

    /// Follow `ActionBus::dispatch_batch`: writes during a batch are one undo step. On the
    /// first write in a batch, earlier events are processed outside the group and the group
    /// opens; the next write or flush after the batch processes the batch's events, then
    /// closes it. Outside batches this is a global lookup and a compare.
    fn sync_batch(&self, cx: &mut App) {
        if self.processing.get() {
            return; // joins the running flush, and whatever group it is in
        }
        let open = ActionBus::open_batch(cx);
        let joined = self.inner.read(cx).batch;
        if joined == open {
            return;
        }
        if joined.is_some() {
            self.flush(cx); // processes the finished batch's events and closes its group
        }
        if let Some(serial) = open {
            self.flush(cx);
            self.inner.update(cx, |i, _| {
                i.engine.begin_undo_group();
                i.batch = Some(serial);
            });
        }
    }

    /// Close this store's batch group once its batch has finished (after processing).
    fn close_finished_batch(&self, cx: &mut App) {
        let Some(joined) = self.inner.read(cx).batch else {
            return;
        };
        if ActionBus::open_batch(cx) != Some(joined) {
            self.inner.update(cx, |i, _| {
                i.engine.end_undo_group();
                i.batch = None;
            });
        }
    }

    fn schedule(&self, cx: &mut App) {
        if self.scheduled.get() || self.processing.get() {
            return; // the pending or running flush will pick it up
        }
        self.scheduled.set(true);
        let weak = self.downgrade();
        cx.defer(move |cx| {
            if let Some(store) = weak.upgrade() {
                store.flush(cx);
            }
        });
    }

    /// Process everything queued now. Reentrant calls (from effect handlers or renders during
    /// the flush) are folded into the running flush.
    pub fn flush(&self, cx: &mut App) {
        self.flush_with_first::<UndoEvent>(None, cx);
    }

    fn flush_with_first<E: Event<S>>(&self, mut first: Option<E>, cx: &mut App) {
        if self.processing.get() {
            return;
        }
        self.processing.set(true);
        let _processing = FlagGuard(self.processing.clone());
        self.scheduled.set(false);
        // Share the state with no snapshot while handlers write it in place.
        self.snapshot.borrow_mut().take();
        let mut budget = self.inner.read(cx).engine.cascade_limit;
        let mut error = None;
        loop {
            let mut dirty = DirtyKeys::default();
            let (outcome, state) = self.inner.update(cx, |i, cx| {
                let cofx = i.cofx.clone();
                let mut cofx = cofx.borrow_mut();
                cofx.now = Some(cx.background_executor().now());
                let out = i
                    .engine
                    .process_with_first(first.take(), &cofx, &mut budget, &mut dirty);
                i.stats.events_processed += out.processed as u64;
                (out, i.engine.state().clone())
            });
            if outcome.changed {
                let inner = self.inner.read(cx);
                let generic = inner.slots.clone();
                let keyed = inner.keyed.clone();
                Self::refresh_group(&generic, &state, cx);
                if dirty.all {
                    // Callbacks may add or remove subscriptions during a refresh.
                    let groups: Vec<_> = keyed
                        .borrow()
                        .values()
                        .map(|node| node.slots.clone())
                        .collect();
                    for group in groups {
                        Self::refresh_group(&group, &state, cx);
                    }
                } else {
                    for (key, subtree) in dirty.iter() {
                        Self::refresh_key(&keyed, key, subtree, &state, cx);
                    }
                }
            }
            if outcome.error.is_some() {
                error = outcome.error;
                break;
            }
            for effect in outcome.effects {
                self.run_effect(effect, cx);
            }
            if !self.inner.read(cx).engine.has_pending() {
                break;
            }
        }
        self.close_finished_batch(cx);
        *self.snapshot.borrow_mut() = Some(self.inner.read(cx).engine.state().clone());
        let handler = self.inner.update(cx, |i, _| {
            i.stats.flushes += 1;
            i.stats.last_error = error.clone();
            i.on_error.clone()
        });
        if let (Some(e), Some(h)) = (error, handler) {
            h(&e, cx);
        }
    }

    fn refresh_key(
        keyed: &Rc<RefCell<GroupMap<S>>>,
        key: u64,
        subtree: bool,
        state: &S,
        cx: &mut App,
    ) {
        let (group, children) = {
            let groups = keyed.borrow();
            let Some(node) = groups.get(&key) else { return };
            // Only subtree writes with registered descendants collect (and allocate).
            let children = (subtree && !node.children.is_empty())
                .then(|| node.children.iter().copied().collect::<Vec<_>>());
            (node.slots.clone(), children)
        };
        Self::refresh_group(&group, state, cx);
        for child in children.into_iter().flatten() {
            Self::refresh_key(keyed, child, true, state, cx);
        }
    }

    fn refresh_group(group: &SlotGroup<S>, state: &S, cx: &mut App) {
        let slots = group.borrow().entries.clone();
        for slot in slots.iter().filter_map(Weak::upgrade) {
            slot.refresh(state, cx);
        }
    }

    fn run_effect(&self, effect: Effect, cx: &mut App) {
        let Effect { name, value } = effect;
        let value = match value.downcast::<DispatchLater<S>>() {
            Ok(later) => return self.schedule_later(*later, cx),
            Err(other) => other,
        };
        let ty = (*value).type_id();
        let handler = self.inner.read(cx).effects.get(&ty).cloned();
        match handler {
            Some(h) => h(value, self, cx),
            None => self.inner.update(cx, |i, _| {
                let list = &mut i.stats.unhandled_effects;
                list.push(name);
                if list.len() > 32 {
                    list.remove(0);
                }
            }),
        }
    }

    fn schedule_later(&self, later: DispatchLater<S>, cx: &mut App) {
        let delay = later.delay;
        let key = later.key;
        let queued = later.into_queued();
        let weak = self.downgrade();
        let timer = cx.background_executor().timer(delay);
        if let Some(key) = key {
            let ticket = self.inner.update(cx, |inner, _| {
                inner.next_timer += 1;
                inner.next_timer
            });
            let task = cx.spawn(async move |cx| {
                timer.await;
                cx.update(|cx| {
                    let Some(store) = weak.upgrade() else { return };
                    let dispatched = store.inner.update(cx, |inner, _| {
                        if inner
                            .debounce
                            .get(key)
                            .is_none_or(|entry| entry.0 != ticket)
                        {
                            return false;
                        }
                        if let Some((_, action, task)) = inner.debounce.remove(key) {
                            task.detach();
                            inner.engine.dispatch_queued(action);
                        }
                        true
                    });
                    if dispatched {
                        store.schedule(cx);
                    }
                });
            });
            self.inner.update(cx, |inner, _| {
                inner.debounce.insert(key, (ticket, queued, task));
            });
        } else {
            cx.spawn(async move |cx| {
                timer.await;
                cx.update(|cx| {
                    if let Some(store) = weak.upgrade() {
                        store
                            .inner
                            .update(cx, |inner, _| inner.engine.dispatch_queued(queued));
                        store.schedule(cx);
                    }
                });
            })
            .detach();
        }
    }

    pub fn stats(&self, cx: &App) -> StoreStats {
        let i = self.inner.read(cx);
        StoreStats {
            selections: i
                .slots
                .borrow()
                .entries
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|s| s.alive())
                .count()
                + i.keyed
                    .borrow()
                    .values()
                    .map(|node| node.slots.borrow().live)
                    .sum::<usize>(),
            ..i.stats.clone()
        }
    }

    /// Select a value for view `V`; the view is notified only when the value changes.
    ///
    /// Call in the view's constructor and keep the [`Selected`]; read it in `render`.
    pub fn select<V: 'static, T: Clone + PartialEq + 'static>(
        &self,
        cx: &mut Context<V>,
        f: impl Fn(&S) -> T + 'static,
    ) -> Selected<T> {
        self.select_in(&[], cx, f)
    }

    /// Provisional selection refreshed by writes to `key` and by unkeyed writes.
    /// Every dependency of `f` must be written under this key or without a key.
    pub fn select_key<V: 'static, T: Clone + PartialEq + 'static>(
        &self,
        key: u64,
        cx: &mut Context<V>,
        f: impl Fn(&S) -> T + 'static,
    ) -> Selected<T> {
        self.select_in(&[key], cx, f)
    }

    /// Select the value at `lens`, refreshed only by writes on its path (and unkeyed writes).
    /// `None` while the lens reads nothing, such as a removed item.
    pub fn select_lens<V: 'static, L>(
        &self,
        lens: L,
        cx: &mut Context<V>,
    ) -> Selected<Option<L::Target>>
    where
        L: crate::Lens<Root = S>,
        L::Target: Clone + PartialEq,
    {
        self.select_at(lens, cx, move |state| lens.get(state).cloned())
    }

    /// Select `f(state)`, refreshed by writes on `lens`'s path and unkeyed writes. `f` may
    /// depend only on state at or below `lens`.
    pub fn select_at<V: 'static, L, T>(
        &self,
        lens: L,
        cx: &mut Context<V>,
        f: impl Fn(&S) -> T + 'static,
    ) -> Selected<T>
    where
        L: crate::Lens<Root = S>,
        T: Clone + PartialEq + 'static,
    {
        let mut chain = Vec::new();
        lens.for_each_key(&mut |key| chain.push(key));
        self.select_in(&chain, cx, f)
    }

    fn select_in<V: 'static, T: Clone + PartialEq + 'static>(
        &self,
        chain: &[u64],
        cx: &mut Context<V>,
        f: impl Fn(&S) -> T + 'static,
    ) -> Selected<T> {
        let value = f(&self.state(cx));
        let entity = cx.new(|_| SelectionCell { value });
        let slot = self.register_chain(
            chain,
            SelectSlot {
                entity: entity.downgrade(),
                select: Box::new(f),
            },
            cx,
        );
        let observation = cx.observe(&entity, |_, _, cx| cx.notify());
        Selected {
            entity,
            _observation: observation,
            _slot: slot,
        }
    }

    /// Two-stage selection (re-frame "layer 3"): `input` is cheap and runs after every state
    /// change; `compute` (possibly expensive) runs only when the input changed.
    pub fn select_memo<
        V: 'static,
        I: Clone + PartialEq + 'static,
        T: Clone + PartialEq + 'static,
    >(
        &self,
        cx: &mut Context<V>,
        input: impl Fn(&S) -> I + 'static,
        compute: impl Fn(&I) -> T + 'static,
    ) -> Selected<T> {
        let first_input = input(&self.state(cx));
        let value = compute(&first_input);
        let entity = cx.new(|_| SelectionCell { value });
        let slot = self.register(
            MemoSlot {
                entity: entity.downgrade(),
                last_input: RefCell::new(first_input),
                input: Box::new(input),
                compute: Box::new(compute),
            },
            cx,
        );
        let observation = cx.observe(&entity, |_, _, cx| cx.notify());
        Selected {
            entity,
            _observation: observation,
            _slot: slot,
        }
    }

    fn register(&self, slot: impl Slot<S> + 'static, cx: &mut App) -> Rc<dyn Any> {
        self.register_chain(&[], slot, cx)
    }

    /// Register under the last key of `chain`, linking each key to its parent in the key
    /// tree. An empty chain registers an unkeyed selection, refreshed by every change.
    pub(crate) fn register_chain(
        &self,
        chain: &[u64],
        slot: impl Slot<S> + 'static,
        cx: &mut App,
    ) -> Rc<dyn Any> {
        let _ = &cx;
        let inner = self;
        let key = chain.last().copied();
        let slots = match key {
            Some(key) => {
                let mut groups = inner.keyed.borrow_mut();
                let mut parent = None;
                for &k in chain {
                    let node = groups.entry(k).or_insert_with(|| KeyNode {
                        slots: Rc::default(),
                        parent,
                        children: HashSet::new(),
                    });
                    if node.parent.is_none() {
                        node.parent = parent;
                    }
                    if let Some(p) = parent
                        && let Some(parent) = groups.get_mut(&p)
                    {
                        parent.children.insert(k);
                    }
                    parent = Some(k);
                }
                groups
                    .get(&key)
                    .map(|node| node.slots.clone())
                    .unwrap_or_default()
            }
            None => inner.slots.clone(),
        };
        let registered = Rc::new(RegisteredSlot {
            slot,
            slots: Rc::downgrade(&slots),
            keyed: key.map(|key| (key, Rc::downgrade(&inner.keyed))),
        });
        let erased: Rc<dyn Slot<S>> = registered.clone();
        let mut slots = slots.borrow_mut();
        Rc::make_mut(&mut slots.entries).push(Rc::downgrade(&erased));
        slots.live += 1;
        registered
    }
}

struct SelectionCell<T> {
    value: T,
}

fn set_cell<T: PartialEq + 'static>(entity: &Entity<SelectionCell<T>>, value: T, cx: &mut App) {
    if entity.read(cx).value != value {
        entity.update(cx, |cell, cx| {
            cell.value = value;
            cx.notify();
        });
    }
}

struct SelectSlot<S, T: 'static> {
    entity: WeakEntity<SelectionCell<T>>,
    select: Box<dyn Fn(&S) -> T>,
}

impl<S, T: PartialEq + 'static> Slot<S> for SelectSlot<S, T> {
    fn refresh(&self, state: &S, cx: &mut App) {
        if let Some(entity) = self.entity.upgrade() {
            set_cell(&entity, (self.select)(state), cx);
        }
    }
    fn alive(&self) -> bool {
        self.entity.upgrade().is_some()
    }
}

struct MemoSlot<S, I, T: 'static> {
    entity: WeakEntity<SelectionCell<T>>,
    last_input: RefCell<I>,
    input: Box<dyn Fn(&S) -> I>,
    compute: Box<dyn Fn(&I) -> T>,
}

impl<S, I: PartialEq, T: PartialEq + 'static> Slot<S> for MemoSlot<S, I, T> {
    fn refresh(&self, state: &S, cx: &mut App) {
        let Some(entity) = self.entity.upgrade() else {
            return;
        };
        let input = (self.input)(state);
        if *self.last_input.borrow() == input {
            return;
        }
        let value = (self.compute)(&input);
        *self.last_input.borrow_mut() = input;
        set_cell(&entity, value, cx);
    }
    fn alive(&self) -> bool {
        self.entity.upgrade().is_some()
    }
}

/// A memoized selection held by a view. Dropping it unsubscribes.
pub struct Selected<T: 'static> {
    entity: Entity<SelectionCell<T>>,
    _observation: Subscription,
    _slot: Rc<dyn Any>,
}

impl<T: 'static> Selected<T> {
    /// The value as of the last flush.
    pub fn get<'a>(&self, cx: &'a App) -> &'a T {
        &self.entity.read(cx).value
    }
}

#[cfg(test)]
mod registration_tests {
    use super::*;
    use gpui_kit::TestAppContext;

    struct CountedSlot(Rc<Cell<usize>>);

    impl Slot<()> for CountedSlot {
        fn refresh(&self, _: &(), _: &mut App) {}

        fn alive(&self) -> bool {
            self.0.set(self.0.get() + 1);
            true
        }
    }

    #[gpui_kit::test]
    fn registering_live_selections_has_linear_liveness_work(cx: &mut TestAppContext) {
        let checks = |cx: &mut App, count| {
            let store = Store::new((), cx);
            let checks = Rc::new(Cell::new(0));
            let mut live: Vec<_> = (0..count)
                .map(|_| store.register(CountedSlot(checks.clone()), cx))
                .collect();
            let registration_checks = checks.get();
            assert_eq!(store.stats(cx).selections, count);
            live.truncate(3);
            assert_eq!(store.stats(cx).selections, 3);
            assert!(store.inner.read(cx).slots.borrow().entries.len() < 6);
            live.clear();
            assert!(store.inner.read(cx).slots.borrow().entries.is_empty());
            registration_checks
        };
        let small = cx.update(|cx| checks(cx, 1_000));
        let large = cx.update(|cx| checks(cx, 10_000));
        assert!(large <= 2 * 10_000, "1k: {small}, 10k: {large}");
    }

    #[gpui_kit::test]
    fn registration_and_single_drops_do_not_sweep(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let store = Store::new((), cx);
            let checks = Rc::new(Cell::new(0));
            let mut live: Vec<_> = (0..10)
                .map(|_| store.register(CountedSlot(checks.clone()), cx))
                .collect();

            // Any per-operation sweep, however it tests liveness, would remove this dead entry.
            live.pop();
            let entries = store.inner.read(cx).slots.borrow().entries.len();
            assert_eq!(entries, 10, "dropping one selection swept the registry");
            live.push(store.register(CountedSlot(checks.clone()), cx));
            let entries = store.inner.read(cx).slots.borrow().entries.len();
            assert_eq!(entries, 11, "registering swept the registry");
        });
    }
}
