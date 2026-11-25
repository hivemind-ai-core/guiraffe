//! The pure, GPUI-free event engine (re-frame's "dominoes" 1-3).
//!
//! 1. Events are queued ([`Engine::dispatch`]) and processed run-to-completion, FIFO.
//! 2. A handler reads the state and *coeffects* (injected inputs such as the time) and
//!    describes its outcome through [`Fx`]: a new state and *effects as data*.
//! 3. The caller executes the returned effects.
//!
//! Handlers are therefore pure functions of `(state, event, coeffects)`, testable without GPUI.

use guiraffe_core::protocol::Source;

use crate::keyed::KeyedList;
use crate::lens::{Lens, write_keys};

use std::{
    any::{Any, TypeId},
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

/// An event for state `S`. Implement on a plain struct or enum per feature module.
///
/// ```
/// # use guiraffe_state::{Event, Fx};
/// #[derive(Clone, Default, PartialEq)]
/// struct Counter { n: i32 }
/// #[derive(Debug)]
/// struct Add(i32);
/// impl Event<Counter> for Add {
///     fn handle(self, fx: &mut Fx<'_, Counter>) {
///         fx.update(|s| s.n += self.0);
///     }
/// }
/// ```
pub trait Event<S>: fmt::Debug + 'static {
    fn handle(self, fx: &mut Fx<'_, S>);

    /// Whether a state change from this event is recorded for undo (default: yes).
    fn undoable(&self) -> bool {
        true
    }

    /// Whether to confirm a change by comparing old and new state (`PartialEq`). Return
    /// `false` for hot events on large states where every `update` is a real change; the
    /// engine then trusts that `update`/`set` was called.
    fn compare_state(&self) -> bool {
        true
    }

    /// Where the event came from. Bound controls tag their edits with the gesture or bus
    /// source; app code defaults to [`Source::Host`]. Interceptors, undo and recording use it
    /// to tell a user edit from a host write.
    fn source(&self) -> Source {
        Source::Host
    }
}

trait ErasedEvent<S> {
    fn source(&self) -> Source;
    fn handle(self: Box<Self>, fx: &mut Fx<'_, S>);
    fn undoable(&self) -> bool;
    fn compare_state(&self) -> bool;
    fn describe(&self) -> String;
    fn event_type(&self) -> TypeId;
}

impl<S, E: Event<S>> ErasedEvent<S> for E {
    fn event_type(&self) -> TypeId {
        TypeId::of::<E>()
    }
    fn source(&self) -> Source {
        Event::source(self)
    }
    fn handle(self: Box<Self>, fx: &mut Fx<'_, S>) {
        Event::handle(*self, fx)
    }
    fn undoable(&self) -> bool {
        Event::undoable(self)
    }
    fn compare_state(&self) -> bool {
        Event::compare_state(self)
    }
    fn describe(&self) -> String {
        format!("{self:?}")
    }
}

/// An effect is plain data; the host runs the handler registered for its type.
pub struct Effect {
    /// `std::any::type_name` of the effect, for diagnostics.
    pub name: &'static str,
    pub value: Box<dyn Any>,
}

/// Effect: dispatch an event after a delay.
pub struct DispatchLater<S> {
    pub delay: Duration,
    /// Debounce key: a newer `DispatchLater` with the same key cancels the pending one.
    pub key: Option<&'static str>,
    pub(crate) event: Box<dyn ErasedEventSend<S>>,
}

#[doc(hidden)]
pub trait ErasedEventSend<S>: 'static {
    #[doc(hidden)]
    fn into_queued(self: Box<Self>) -> Queued<S>;
}

impl<S: 'static, E: Event<S>> ErasedEventSend<S> for E {
    fn into_queued(self: Box<Self>) -> Queued<S> {
        Queued(self)
    }
}

#[doc(hidden)]
pub struct Queued<S>(Box<dyn ErasedEvent<S>>);

/// Injected inputs available to handlers (re-frame "coeffects").
#[derive(Default)]
pub struct Coeffects {
    pub now: Option<Instant>,
    values: HashMap<TypeId, Box<dyn Any>>,
}

impl Coeffects {
    pub fn with_now(now: Instant) -> Self {
        Self {
            now: Some(now),
            values: HashMap::new(),
        }
    }
    pub fn provide<T: 'static>(&mut self, value: T) {
        self.values.insert(TypeId::of::<T>(), Box::new(value));
    }
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.values
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }
}

/// Keys name independent projections within one store. A write without a key
/// conservatively refreshes every registered projection.
///
/// Each entry is a key and whether its whole subtree changed (`true`) or only the
/// projections registered exactly at it (`false`). Up to eight keys are stored inline, so a
/// write through a lens of depth eight or less never allocates.
#[derive(Default)]
pub(crate) struct DirtyKeys {
    pub(crate) all: bool,
    inline: [(u64, bool); 8],
    len: usize,
    rest: BTreeMap<u64, bool>,
}

impl DirtyKeys {
    /// Mark `key` and its whole subtree.
    pub(crate) fn insert(&mut self, key: u64) {
        self.mark(key, true);
    }

    pub(crate) fn mark(&mut self, key: u64, subtree: bool) {
        if let Some(entry) = self.inline[..self.len].iter_mut().find(|(k, _)| *k == key) {
            entry.1 |= subtree;
            return;
        }
        if self.len < self.inline.len() {
            self.inline[self.len] = (key, subtree);
            self.len += 1;
        } else {
            *self.rest.entry(key).or_default() |= subtree;
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (u64, bool)> + '_ {
        self.inline[..self.len]
            .iter()
            .copied()
            .chain(self.rest.iter().map(|(k, s)| (*k, *s)))
    }
}

/// The handler's window onto the world.
pub struct Fx<'a, S> {
    state: &'a mut Arc<S>,
    candidate: Option<S>,
    dirty: &'a mut DirtyKeys,
    direct: bool,
    changed_in_place: bool,
    effects: &'a mut Vec<Effect>,
    queue: &'a mut VecDeque<Queued<S>>,
    cofx: &'a Coeffects,
    source: Source,
}

impl<S: Clone + PartialEq + 'static> Fx<'_, S> {
    /// Current state.
    pub fn state(&self) -> &S {
        self.candidate.as_ref().unwrap_or(self.state)
    }

    /// Mutate a working copy of the state, leaving shared snapshots untouched.
    pub fn update<R>(&mut self, f: impl FnOnce(&mut S) -> R) -> R {
        self.dirty.all = true;
        f(self.candidate.get_or_insert_with(|| (**self.state).clone()))
    }

    /// Replace the whole state.
    pub fn set(&mut self, state: S) {
        self.dirty.all = true;
        self.candidate = Some(state);
    }

    /// Write a projected field without cloning the state when the value is equal.
    /// The reader and writer must address the same field.
    pub fn set_field<T: PartialEq>(
        &mut self,
        value: T,
        read: impl FnOnce(&S) -> &T,
        write: impl FnOnce(&mut S, T),
    ) {
        if *read(self.state()) != value {
            self.update(|state| write(state, value));
        }
    }

    /// Provisional keyed field write. Readers registered for this key must depend
    /// only on fields that publish under it. Ordinary selections observe all writes.
    /// The projections must select the same field. Structural edits should use
    /// `update` so that keyed readers are refreshed after reordering.
    pub fn set_keyed_field<T: PartialEq>(
        &mut self,
        key: u64,
        value: T,
        read: impl FnOnce(&S) -> &T,
        field: impl FnOnce(&mut S) -> &mut T,
    ) {
        if *read(self.state()) == value {
            return;
        }
        self.dirty.insert(key);
        if self.direct && self.candidate.is_none() {
            *field(Arc::make_mut(self.state)) = value;
            self.changed_in_place = true;
        } else {
            *field(self.candidate.get_or_insert_with(|| (**self.state).clone())) = value;
        }
    }

    /// Write `value` through `lens` if it differs. The write marks only the lens's path, so
    /// only watchers at or below it, and those registered exactly at its prefixes, re-check.
    /// Returns whether the state changed. A lens that reads `None` (a removed item) is a no-op.
    pub fn set_at<L: Lens<Root = S>>(&mut self, lens: L, value: L::Target) -> bool
    where
        L::Target: PartialEq,
    {
        let Some(value) = lens.check(value) else {
            return false;
        };
        if lens
            .get(self.state())
            .is_none_or(|current| *current == value)
        {
            return false;
        }
        write_keys(&lens, |key, subtree| self.dirty.mark(key, subtree));
        self.write_in_place(|state| {
            if let Some(slot) = lens.get_mut(state) {
                *slot = value;
            }
        });
        true
    }

    /// Insert an item into a keyed list. Notifies the list's watchers and the item's own,
    /// not the other items'. An existing key is replaced in place.
    pub fn list_insert<L, K, T>(&mut self, list: L, index: usize, key: K, item: T)
    where
        L: Lens<Root = S, Target = KeyedList<K, T>>,
        K: Copy + std::hash::Hash + Eq + 'static,
        T: Clone + 'static,
    {
        self.structural(list, key, |list| list.insert(index, key, item));
    }

    /// Remove an item from a keyed list. Returns whether it existed.
    pub fn list_remove<L, K, T>(&mut self, list: L, key: K) -> bool
    where
        L: Lens<Root = S, Target = KeyedList<K, T>>,
        K: Copy + std::hash::Hash + Eq + 'static,
        T: Clone + 'static,
    {
        if !list.get(self.state()).is_some_and(|l| l.contains(&key)) {
            return false;
        }
        self.structural(list, key, |list| {
            list.remove(&key);
        });
        true
    }

    /// Move an item within a keyed list. Returns whether it moved.
    pub fn list_move<L, K, T>(&mut self, list: L, key: K, index: usize) -> bool
    where
        L: Lens<Root = S, Target = KeyedList<K, T>>,
        K: Copy + std::hash::Hash + Eq + 'static,
        T: Clone + 'static,
    {
        let Some(from) = list.get(self.state()).and_then(|l| l.index_of(&key)) else {
            return false;
        };
        let len = list.get(self.state()).map_or(0, KeyedList::len);
        if from == index.min(len.saturating_sub(1)) {
            return false;
        }
        self.structural(list, key, |list| {
            list.move_to(&key, index);
        });
        true
    }

    fn structural<L, K, T>(&mut self, list: L, key: K, edit: impl FnOnce(&mut KeyedList<K, T>))
    where
        L: Lens<Root = S, Target = KeyedList<K, T>>,
        K: Copy + std::hash::Hash + Eq + 'static,
        T: Clone + 'static,
    {
        // The list and its prefixes exactly (list views, whole-list readers), and the item's
        // subtree. Other items' watchers are untouched.
        list.for_each_key(&mut |k| self.dirty.mark(k, false));
        let item = crate::lens::ListLens::by_key(list, key);
        self.dirty.mark(item.key(), true);
        self.write_in_place(|state| {
            if let Some(list) = list.get_mut(state) {
                edit(list);
            }
        });
    }

    fn write_in_place(&mut self, write: impl FnOnce(&mut S)) {
        if self.direct && self.candidate.is_none() {
            write(Arc::make_mut(self.state));
            self.changed_in_place = true;
        } else {
            write(self.candidate.get_or_insert_with(|| (**self.state).clone()));
        }
    }

    /// The source of the event being handled.
    pub fn source(&self) -> Source {
        self.source
    }

    /// Queue another event; it runs after the current one (and after earlier queued events).
    pub fn dispatch(&mut self, event: impl Event<S>) {
        self.queue.push_back(Queued(Box::new(event)));
    }

    /// Ask the host to dispatch `event` after `delay`.
    pub fn dispatch_later(&mut self, delay: Duration, event: impl Event<S>) {
        self.push_effect(DispatchLater {
            delay,
            key: None,
            event: Box::new(event) as Box<dyn ErasedEventSend<S>>,
        });
    }

    /// Dispatch `event` after `delay`, cancelling any pending debounced event with the same
    /// `key` (search-as-you-type, autosave).
    pub fn dispatch_debounced(&mut self, key: &'static str, delay: Duration, event: impl Event<S>) {
        self.push_effect(DispatchLater {
            delay,
            key: Some(key),
            event: Box::new(event) as Box<dyn ErasedEventSend<S>>,
        });
    }

    /// Emit an effect as data, executed by the handler registered for `E`.
    pub fn effect<E: 'static>(&mut self, effect: E) {
        self.push_effect(effect);
    }

    fn push_effect<E: 'static>(&mut self, effect: E) {
        self.effects.push(Effect {
            name: std::any::type_name::<E>(),
            value: Box::new(effect),
        });
    }

    pub fn now(&self) -> Option<Instant> {
        self.cofx.now
    }

    pub fn cofx<T: 'static>(&self) -> Option<&T> {
        self.cofx.get()
    }
}

impl<S> Drop for Fx<'_, S> {
    fn drop(&mut self) {
        // Publish on unwind as well, matching mutation of the engine's working
        // state before a handler or its comparison panicked.
        if let Some(candidate) = self.candidate.take() {
            *self.state = Arc::new(candidate);
        }
    }
}

/// Observes every processed event (logging, validation, analytics, devtools).
pub trait Interceptor<S>: 'static {
    /// Called after an event's handler ran. `before`/`after` are cheap `Arc` snapshots.
    fn after(&mut self, record: &EventRecord, before: &Arc<S>, after: &Arc<S>);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub seq: u64,
    pub description: String,
    pub changed: bool,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// More than `limit` events cascaded from one batch (likely a dispatch loop).
    CascadeLimit { limit: usize },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::CascadeLimit { limit } => write!(
                f,
                "more than {limit} events cascaded in one batch; likely a dispatch loop"
            ),
        }
    }
}

impl std::error::Error for EngineError {}

/// Result of processing the queue.
pub struct Outcome {
    pub changed: bool,
    pub processed: usize,
    pub effects: Vec<Effect>,
    pub error: Option<EngineError>,
}

type Capture<S> = Box<dyn Fn(&Arc<S>) -> Box<dyn Any>>;
type Restore<S> = Box<dyn Fn(&mut Arc<S>, Box<dyn Any>)>;

/// Undo history over a *scope* of the state (the whole state, or a lens onto part of it).
struct Undo<S> {
    limit: usize,
    past: VecDeque<Box<dyn Any>>,
    future: Vec<Box<dyn Any>>,
    capture: Capture<S>,
    restore: Restore<S>,
}

pub struct Engine<S> {
    state: Arc<S>,
    queue: VecDeque<Queued<S>>,
    interceptors: Vec<Box<dyn Interceptor<S>>>,
    undo: Option<Undo<S>>,
    seq: u64,
    pub cascade_limit: usize,
}

/// Built-in event: undo the last undoable change (no-op without history).
#[derive(Debug, Clone, Copy)]
pub struct UndoEvent;
/// Built-in event: redo.
#[derive(Debug, Clone, Copy)]
pub struct RedoEvent;

impl<S: Clone + PartialEq + 'static> Engine<S> {
    pub fn new(state: S) -> Self {
        Self {
            state: Arc::new(state),
            queue: VecDeque::new(),
            interceptors: Vec::new(),
            undo: None,
            seq: 0,
            cascade_limit: 10_000,
        }
    }

    pub fn state(&self) -> &Arc<S> {
        &self.state
    }

    pub fn dispatch(&mut self, event: impl Event<S>) {
        self.queue.push_back(Queued(Box::new(event)));
    }

    #[doc(hidden)]
    pub fn dispatch_queued(&mut self, q: Queued<S>) {
        self.queue.push_back(q);
    }

    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn add_interceptor(&mut self, i: impl Interceptor<S>) {
        self.interceptors.push(Box::new(i));
    }

    /// Undo/redo over the **whole** state, keeping up to `limit` steps (0 disables undo).
    /// Calling again replaces (and clears) any existing history.
    ///
    /// Caveat: undo restores the entire state, so it also reverts changes made by
    /// non-undoable events since that step (e.g. data that was loaded). Prefer
    /// [`Engine::enable_undo_lens`] to scope undo to the document part of the state.
    pub fn enable_undo(&mut self, limit: usize) {
        self.set_undo(
            limit,
            Box::new(|s: &Arc<S>| Box::new(s.clone()) as Box<dyn Any>),
            Box::new(|st: &mut Arc<S>, snap: Box<dyn Any>| {
                if let Ok(prev) = snap.downcast::<Arc<S>>() {
                    *st = *prev;
                }
            }),
        );
    }

    /// Undo/redo scoped to the part of the state reached by `get`/`set` (re-frame's
    /// undo `:path`). State outside the lens is never touched by undo or redo.
    pub fn enable_undo_lens<U: Clone + 'static>(
        &mut self,
        limit: usize,
        get: impl Fn(&S) -> U + 'static,
        set: impl Fn(&mut S, U) + 'static,
    ) {
        self.set_undo(
            limit,
            Box::new(move |s: &Arc<S>| Box::new(get(s)) as Box<dyn Any>),
            Box::new(move |st: &mut Arc<S>, snap: Box<dyn Any>| {
                if let Ok(u) = snap.downcast::<U>() {
                    set(Arc::make_mut(st), *u);
                }
            }),
        );
    }

    fn set_undo(&mut self, limit: usize, capture: Capture<S>, restore: Restore<S>) {
        self.undo = (limit > 0).then(|| Undo {
            limit,
            past: VecDeque::new(),
            future: Vec::new(),
            capture,
            restore,
        });
    }

    pub fn can_undo(&self) -> bool {
        self.undo.as_ref().is_some_and(|u| !u.past.is_empty())
    }
    pub fn can_redo(&self) -> bool {
        self.undo.as_ref().is_some_and(|u| !u.future.is_empty())
    }

    /// Restore the previous undoable state. Returns whether anything changed.
    pub fn undo(&mut self) -> bool {
        self.step(true)
    }

    /// Re-apply an undone state. Returns whether anything changed.
    pub fn redo(&mut self) -> bool {
        self.step(false)
    }

    fn step(&mut self, back: bool) -> bool {
        let Some(u) = self.undo.as_mut() else {
            return false;
        };
        let target = if back {
            u.past.pop_back()
        } else {
            u.future.pop()
        };
        let Some(target) = target else { return false };
        let current = (u.capture)(&self.state);
        if back {
            u.future.push(current);
        } else {
            u.past.push_back(current);
        }
        let before = self.state.clone();
        (u.restore)(&mut self.state, target);
        !Arc::ptr_eq(&before, &self.state) && *before != *self.state
    }

    /// Process queued events until the queue is empty, with a fresh cascade budget.
    pub fn process(&mut self, cofx: &Coeffects) -> Outcome {
        let mut budget = self.cascade_limit;
        self.process_with_budget(cofx, &mut budget)
    }

    /// Process queued events, spending from `budget` (shared across calls, so loops that go
    /// through effects and back are caught too). When the budget runs out the remaining queue
    /// **and this batch's effects** are dropped and an error is reported, instead of hanging.
    pub fn process_with_budget(&mut self, cofx: &Coeffects, budget: &mut usize) -> Outcome {
        self.process_with_first::<UndoEvent>(None, cofx, budget, &mut DirtyKeys::default())
    }

    pub(crate) fn process_with_first<E: Event<S>>(
        &mut self,
        mut first: Option<E>,
        cofx: &Coeffects,
        budget: &mut usize,
        dirty: &mut DirtyKeys,
    ) -> Outcome {
        let mut effects = Vec::new();
        let mut processed = 0;
        let mut any_change = false;
        let mut error = None;
        while first.is_some() || !self.queue.is_empty() {
            if *budget == 0 {
                self.queue.clear();
                effects.clear();
                error = Some(EngineError::CascadeLimit {
                    limit: self.cascade_limit,
                });
                break;
            }
            *budget -= 1;
            processed += 1;
            let changed = if let Some(event) = first.take() {
                let description = (!self.interceptors.is_empty()).then(|| format!("{event:?}"));
                self.process_event(
                    TypeId::of::<E>(),
                    Event::source(&event),
                    event.undoable(),
                    event.compare_state(),
                    description,
                    dirty,
                    &mut effects,
                    cofx,
                    |fx| event.handle(fx),
                )
            } else if let Some(Queued(event)) = self.queue.pop_front() {
                let description = (!self.interceptors.is_empty()).then(|| event.describe());
                self.process_event(
                    event.event_type(),
                    event.source(),
                    event.undoable(),
                    event.compare_state(),
                    description,
                    dirty,
                    &mut effects,
                    cofx,
                    |fx| event.handle(fx),
                )
            } else {
                break;
            };
            any_change |= changed;
        }
        Outcome {
            changed: any_change,
            processed,
            effects,
            error,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_event(
        &mut self,
        ty: TypeId,
        source: Source,
        undoable: bool,
        compare: bool,
        description: Option<String>,
        dirty: &mut DirtyKeys,
        effects: &mut Vec<Effect>,
        cofx: &Coeffects,
        handle: impl FnOnce(&mut Fx<'_, S>),
    ) -> bool {
        self.seq += 1;
        // Keep an old snapshot only when undo or an interceptor observes it;
        // holding one would make a keyed write copy the whole state. Shared
        // snapshots are still protected by `Arc::make_mut`.
        let before = ((undoable && self.undo.is_some()) || description.is_some())
            .then(|| self.state.clone());
        let changed = if ty == TypeId::of::<UndoEvent>() {
            dirty.all = true;
            self.undo()
        } else if ty == TypeId::of::<RedoEvent>() {
            dirty.all = true;
            self.redo()
        } else {
            let mut fx = Fx {
                state: &mut self.state,
                candidate: None,
                direct: before.is_none(),
                changed_in_place: false,
                dirty,
                effects,
                queue: &mut self.queue,
                cofx,
                source,
            };
            handle(&mut fx);
            // Without an in-place write the state is still the old one, so a
            // candidate is compared with it.
            let changed = fx.changed_in_place
                || fx
                    .candidate
                    .as_ref()
                    .is_some_and(|candidate| !compare || **fx.state != *candidate);
            if !changed {
                // An equal write keeps the original snapshot and needs no new Arc.
                fx.candidate = None;
            }
            drop(fx);
            if changed
                && undoable
                && let (Some(u), Some(before)) = (self.undo.as_mut(), before.as_ref())
            {
                u.past.push_back((u.capture)(before));
                while u.past.len() > u.limit {
                    u.past.pop_front();
                }
                u.future.clear();
            }
            changed
        };
        if let (Some(description), Some(before)) = (description, before) {
            let record = EventRecord {
                seq: self.seq,
                description,
                changed,
                source,
            };
            for i in &mut self.interceptors {
                i.after(&record, &before, &self.state);
            }
        }
        changed
    }
}

impl<S: Clone + PartialEq + 'static> Event<S> for UndoEvent {
    fn handle(self, _: &mut Fx<'_, S>) {}
    fn undoable(&self) -> bool {
        false
    }
}

impl<S: Clone + PartialEq + 'static> Event<S> for RedoEvent {
    fn handle(self, _: &mut Fx<'_, S>) {}
    fn undoable(&self) -> bool {
        false
    }
}

impl<S: 'static> DispatchLater<S> {
    #[doc(hidden)]
    pub fn into_queued(self) -> Queued<S> {
        self.event.into_queued()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq)]
    struct S {
        n: i32,
        log: Vec<&'static str>,
    }

    #[derive(Debug)]
    struct Add(i32);
    impl Event<S> for Add {
        fn handle(self, fx: &mut Fx<'_, S>) {
            fx.update(|s| s.n += self.0);
        }
    }

    #[derive(Debug)]
    struct Chain;
    impl Event<S> for Chain {
        fn handle(self, fx: &mut Fx<'_, S>) {
            fx.update(|s| s.log.push("chain"));
            fx.dispatch(Note("a"));
            fx.dispatch(Note("b"));
        }
    }

    #[derive(Debug)]
    struct Note(&'static str);
    impl Event<S> for Note {
        fn handle(self, fx: &mut Fx<'_, S>) {
            fx.update(|s| s.log.push(self.0));
        }
    }

    #[derive(Debug)]
    struct Noop;
    impl Event<S> for Noop {
        fn handle(self, fx: &mut Fx<'_, S>) {
            fx.update(|s| s.n += 0);
        }
    }

    #[derive(Debug)]
    struct Loop;
    impl Event<S> for Loop {
        fn handle(self, fx: &mut Fx<'_, S>) {
            fx.dispatch(Loop);
        }
    }

    #[derive(Debug, PartialEq)]
    struct Save(i32);
    #[derive(Debug)]
    struct Persist;
    impl Event<S> for Persist {
        fn handle(self, fx: &mut Fx<'_, S>) {
            let n = fx.state().n;
            fx.effect(Save(n));
            if let Some(tag) = fx.cofx::<&'static str>() {
                let tag = *tag;
                fx.update(|s| s.log.push(tag));
            }
        }
    }

    #[derive(Debug)]
    struct Silent(i32);
    impl Event<S> for Silent {
        fn handle(self, fx: &mut Fx<'_, S>) {
            fx.update(|s| s.n = self.0);
        }
        fn undoable(&self) -> bool {
            false
        }
    }

    fn run(e: &mut Engine<S>) -> Outcome {
        e.process(&Coeffects::default())
    }

    #[test]
    fn handlers_update_state_fifo_with_nested_dispatch() {
        let mut e = Engine::new(S::default());
        e.dispatch(Chain);
        e.dispatch(Note("c"));
        let out = run(&mut e);
        assert!(out.changed);
        assert_eq!(out.processed, 4);
        assert_eq!(
            e.state().log,
            vec!["chain", "c", "a", "b"],
            "queued events run after earlier ones"
        );
    }

    #[test]
    fn noop_update_is_not_a_change_and_keeps_allocation() {
        let mut e = Engine::new(S::default());
        let before = e.state().clone();
        e.dispatch(Noop);
        let out = run(&mut e);
        assert!(!out.changed);
        assert!(Arc::ptr_eq(&before, e.state()));
    }

    #[test]
    fn effects_are_data_and_coeffects_are_injected() {
        let mut e = Engine::new(S {
            n: 7,
            ..Default::default()
        });
        e.dispatch(Persist);
        let mut cofx = Coeffects::default();
        cofx.provide::<&'static str>("injected");
        let out = e.process(&cofx);
        assert_eq!(out.effects.len(), 1);
        assert_eq!(out.effects[0].value.downcast_ref::<Save>(), Some(&Save(7)));
        assert!(out.effects[0].name.ends_with("Save"));
        assert_eq!(e.state().log, vec!["injected"]);
    }

    #[test]
    fn cascade_limit_stops_dispatch_loops() {
        let mut e = Engine::new(S::default());
        e.cascade_limit = 100;
        e.dispatch(Loop);
        let out = run(&mut e);
        assert_eq!(out.error, Some(EngineError::CascadeLimit { limit: 100 }));
        assert!(!e.has_pending(), "queue dropped, not left to loop again");
    }

    #[test]
    fn undo_redo_and_non_undoable_events() {
        let mut e = Engine::new(S::default());
        e.enable_undo(2);
        for n in [1, 2, 3] {
            e.dispatch(Add(n));
        }
        run(&mut e);
        assert_eq!(e.state().n, 6);
        e.dispatch(UndoEvent);
        run(&mut e);
        assert_eq!(e.state().n, 3);
        e.dispatch(UndoEvent);
        e.dispatch(UndoEvent); // limit 2: third undo is a no-op
        run(&mut e);
        assert_eq!(e.state().n, 1);
        e.dispatch(RedoEvent);
        run(&mut e);
        assert_eq!(e.state().n, 3);
        e.dispatch(Silent(50));
        run(&mut e);
        assert!(e.can_redo(), "non-undoable change keeps redo history");
        e.dispatch(Add(1));
        run(&mut e);
        assert!(!e.can_redo(), "a new undoable change clears redo");
        let mut plain = Engine::new(S::default());
        plain.dispatch(UndoEvent);
        assert!(!run(&mut plain).changed, "undo without history is a no-op");
    }

    #[derive(Clone, Debug, Default, PartialEq)]
    struct Doc {
        title: String,
        loaded: Vec<i32>,
        big: Arc<Vec<u8>>,
    }

    #[derive(Debug)]
    struct Rename(&'static str);
    impl Event<Doc> for Rename {
        fn handle(self, fx: &mut Fx<'_, Doc>) {
            fx.update(|d| d.title = self.0.into());
        }
    }

    #[derive(Debug)]
    struct Loaded(i32);
    impl Event<Doc> for Loaded {
        fn handle(self, fx: &mut Fx<'_, Doc>) {
            fx.update(|d| d.loaded.push(self.0));
        }
        fn undoable(&self) -> bool {
            false
        }
    }

    #[test]
    fn lens_undo_never_reverts_state_outside_the_lens() {
        let mut e = Engine::new(Doc::default());
        e.enable_undo_lens(10, |d: &Doc| d.title.clone(), |d: &mut Doc, t| d.title = t);
        e.dispatch(Rename("a"));
        e.dispatch(Loaded(1));
        e.dispatch(UndoEvent);
        e.process(&Coeffects::default());
        assert_eq!(e.state().title, "");
        assert_eq!(e.state().loaded, vec![1], "loaded data survives undo");
        e.dispatch(Loaded(2));
        e.dispatch(RedoEvent);
        e.process(&Coeffects::default());
        assert_eq!(e.state().title, "a");
        assert_eq!(
            e.state().loaded,
            vec![1, 2],
            "redo doesn't resurrect stale data"
        );
    }

    #[test]
    fn whole_state_undo_caveat_is_real_and_documented() {
        let mut e = Engine::new(Doc::default());
        e.enable_undo(10);
        e.dispatch(Rename("a"));
        e.dispatch(Loaded(1));
        e.dispatch(UndoEvent);
        e.process(&Coeffects::default());
        assert!(
            e.state().loaded.is_empty(),
            "whole-state undo reverts everything (see enable_undo docs)"
        );
    }

    #[test]
    fn undo_limit_zero_disables_and_reenabling_clears() {
        let mut e = Engine::new(S::default());
        e.enable_undo(0);
        e.dispatch(Add(1));
        run(&mut e);
        assert!(!e.can_undo());
        e.enable_undo(5);
        e.dispatch(Add(1));
        run(&mut e);
        assert!(e.can_undo());
        e.enable_undo(5);
        assert!(!e.can_undo(), "re-enabling replaces history");
    }

    #[test]
    fn structurally_shared_fields_are_not_deep_cloned() {
        let big = Arc::new(vec![0u8; 1_000_000]);
        let mut e = Engine::new(Doc {
            big: big.clone(),
            ..Default::default()
        });
        e.enable_undo(10);
        e.dispatch(Rename("x"));
        run_doc(&mut e);
        assert!(
            Arc::ptr_eq(&e.state().big, &big),
            "Arc field shared, not copied, by update + undo snapshot"
        );
    }

    fn run_doc(e: &mut Engine<Doc>) -> Outcome {
        e.process(&Coeffects::default())
    }

    #[test]
    fn compare_state_opt_out_trusts_update() {
        #[derive(Debug)]
        struct Hot;
        impl Event<S> for Hot {
            fn handle(self, fx: &mut Fx<'_, S>) {
                fx.update(|s| s.n += 0);
            }
            fn compare_state(&self) -> bool {
                false
            }
        }
        let mut e = Engine::new(S::default());
        e.dispatch(Hot);
        assert!(
            run(&mut e).changed,
            "no comparison: update is trusted as a change"
        );
    }

    #[test]
    fn shared_budget_catches_loops_across_process_calls_and_drops_effects() {
        #[derive(Debug)]
        struct Ping;
        impl Event<S> for Ping {
            fn handle(self, fx: &mut Fx<'_, S>) {
                fx.effect(Save(0));
            }
        }
        let mut e = Engine::new(S::default());
        e.cascade_limit = 10;
        let mut budget = 10;
        let mut errors = 0;
        for _ in 0..50 {
            // Simulates an effect handler that dispatches Ping again.
            e.dispatch(Ping);
            let out = e.process_with_budget(&Coeffects::default(), &mut budget);
            if out.error.is_some() {
                errors += 1;
                assert!(
                    out.effects.is_empty(),
                    "effects of the aborted batch are dropped"
                );
                break;
            }
        }
        assert_eq!(errors, 1);
    }

    struct Recorder(std::rc::Rc<std::cell::RefCell<Vec<EventRecord>>>);
    impl Interceptor<S> for Recorder {
        fn after(&mut self, record: &EventRecord, before: &Arc<S>, after: &Arc<S>) {
            assert_eq!(record.changed, before != after);
            self.0.borrow_mut().push(record.clone());
        }
    }

    #[test]
    fn interceptors_see_every_event_with_snapshots() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut e = Engine::new(S::default());
        e.add_interceptor(Recorder(log.clone()));
        e.dispatch(Add(2));
        e.dispatch(Noop);
        run(&mut e);
        let log = log.borrow();
        assert_eq!(log.len(), 2);
        assert_eq!(
            (log[0].seq, log[0].changed, log[0].description.as_str()),
            (1, true, "Add(2)")
        );
        assert!(!log[1].changed);
    }

    #[test]
    fn dispatch_later_is_an_effect() {
        #[derive(Debug)]
        struct Later;
        impl Event<S> for Later {
            fn handle(self, fx: &mut Fx<'_, S>) {
                fx.dispatch_later(Duration::from_millis(5), Add(1));
            }
        }
        let mut e = Engine::new(S::default());
        e.dispatch(Later);
        let out = run(&mut e);
        let later = out
            .effects
            .into_iter()
            .next()
            .and_then(|b| b.value.downcast::<DispatchLater<S>>().ok())
            .map(|l| *l);
        let Some(later) = later else {
            panic!("expected DispatchLater")
        };
        assert_eq!(later.delay, Duration::from_millis(5));
        e.dispatch_queued(later.into_queued());
        run(&mut e);
        assert_eq!(e.state().n, 1);
    }
}
