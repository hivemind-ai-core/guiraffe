//! The pure, GPUI-free event engine (re-frame's "dominoes" 1-3).
//!
//! 1. Events are queued ([`Engine::dispatch`]) and processed run-to-completion, FIFO.
//! 2. A handler reads the state and *coeffects* (injected inputs such as the time) and
//!    describes its outcome through [`Fx`]: a new state and *effects as data*.
//! 3. The caller executes the returned effects.
//!
//! Handlers are therefore pure functions of `(state, event, coeffects)`, testable without GPUI.

use std::{
    any::{Any, TypeId},
    collections::{HashMap, VecDeque},
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
}

trait ErasedEvent<S> {
    fn handle(self: Box<Self>, fx: &mut Fx<'_, S>);
    fn undoable(&self) -> bool;
    fn describe(&self) -> String;
    fn event_type(&self) -> TypeId;
}

impl<S, E: Event<S>> ErasedEvent<S> for E {
    fn event_type(&self) -> TypeId {
        TypeId::of::<E>()
    }
    fn handle(self: Box<Self>, fx: &mut Fx<'_, S>) {
        Event::handle(*self, fx)
    }
    fn undoable(&self) -> bool {
        Event::undoable(self)
    }
    fn describe(&self) -> String {
        format!("{self:?}")
    }
}

/// An effect is plain data; the host runs the handler registered for its type.
pub type Effect = Box<dyn Any>;

/// Effect: dispatch an event after a delay.
pub struct DispatchLater<S> {
    pub delay: Duration,
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

/// The handler's window onto the world.
pub struct Fx<'a, S> {
    state: &'a mut Arc<S>,
    changed: bool,
    effects: &'a mut Vec<Effect>,
    queue: &'a mut VecDeque<Queued<S>>,
    cofx: &'a Coeffects,
}

impl<S: Clone + PartialEq + 'static> Fx<'_, S> {
    /// Current state.
    pub fn state(&self) -> &S {
        self.state
    }

    /// Mutate the state (clone-on-write: shared snapshots, e.g. undo history, are untouched).
    pub fn update<R>(&mut self, f: impl FnOnce(&mut S) -> R) -> R {
        self.changed = true;
        f(Arc::make_mut(self.state))
    }

    /// Replace the whole state.
    pub fn set(&mut self, state: S) {
        *self.state = Arc::new(state);
        self.changed = true;
    }

    /// Queue another event; it runs after the current one (and after earlier queued events).
    pub fn dispatch(&mut self, event: impl Event<S>) {
        self.queue.push_back(Queued(Box::new(event)));
    }

    /// Ask the host to dispatch `event` after `delay`.
    pub fn dispatch_later(&mut self, delay: Duration, event: impl Event<S>) {
        self.effects.push(Box::new(DispatchLater {
            delay,
            event: Box::new(event) as Box<dyn ErasedEventSend<S>>,
        }));
    }

    /// Emit an effect as data, executed by the handler registered for `E`.
    pub fn effect<E: 'static>(&mut self, effect: E) {
        self.effects.push(Box::new(effect));
    }

    pub fn now(&self) -> Option<Instant> {
        self.cofx.now
    }

    pub fn cofx<T: 'static>(&self) -> Option<&T> {
        self.cofx.get()
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

struct Undo<S> {
    limit: usize,
    past: VecDeque<Arc<S>>,
    future: Vec<Arc<S>>,
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

    /// Keep up to `limit` undo steps. Use [`UndoEvent`]/[`RedoEvent`] (or [`Engine::undo`]).
    pub fn enable_undo(&mut self, limit: usize) {
        self.undo = Some(Undo {
            limit: limit.max(1),
            past: VecDeque::new(),
            future: Vec::new(),
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
        let Some(u) = self.undo.as_mut() else {
            return false;
        };
        let Some(prev) = u.past.pop_back() else {
            return false;
        };
        u.future.push(std::mem::replace(&mut self.state, prev));
        true
    }

    /// Re-apply an undone state. Returns whether anything changed.
    pub fn redo(&mut self) -> bool {
        let Some(u) = self.undo.as_mut() else {
            return false;
        };
        let Some(next) = u.future.pop() else {
            return false;
        };
        u.past.push_back(std::mem::replace(&mut self.state, next));
        true
    }

    /// Process queued events until the queue is empty (or the cascade limit trips, which
    /// drops the remaining queue and reports an error instead of hanging the UI).
    pub fn process(&mut self, cofx: &Coeffects) -> Outcome {
        let mut effects = Vec::new();
        let mut processed = 0;
        let mut any_change = false;
        let mut error = None;
        while let Some(Queued(event)) = self.queue.pop_front() {
            if processed >= self.cascade_limit {
                self.queue.clear();
                error = Some(EngineError::CascadeLimit {
                    limit: self.cascade_limit,
                });
                break;
            }
            processed += 1;
            self.seq += 1;
            let description = event.describe();
            let before = self.state.clone();
            let ty = event.event_type();
            let changed = if ty == TypeId::of::<UndoEvent>() {
                self.undo()
            } else if ty == TypeId::of::<RedoEvent>() {
                self.redo()
            } else {
                let undoable = event.undoable();
                let mut fx = Fx {
                    state: &mut self.state,
                    changed: false,
                    effects: &mut effects,
                    queue: &mut self.queue,
                    cofx,
                };
                event.handle(&mut fx);
                let touched = fx.changed;
                // `update` may have been called without a real change: only a different value
                // counts, so no-op updates create no undo step and notify nobody.
                let changed =
                    touched && !Arc::ptr_eq(&before, &self.state) && *before != *self.state;
                if touched && !changed {
                    // Keep the original allocation so pointer-equality fast paths stay valid.
                    self.state = before.clone();
                }
                if changed
                    && undoable
                    && let Some(u) = self.undo.as_mut()
                {
                    u.past.push_back(before.clone());
                    while u.past.len() > u.limit {
                        u.past.pop_front();
                    }
                    u.future.clear();
                }
                changed
            };
            any_change |= changed;
            let record = EventRecord {
                seq: self.seq,
                description,
                changed,
            };
            for i in &mut self.interceptors {
                i.after(&record, &before, &self.state);
            }
        }
        Outcome {
            changed: any_change,
            processed,
            effects,
            error,
        }
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
        assert_eq!(out.effects[0].downcast_ref::<Save>(), Some(&Save(7)));
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
            .and_then(|b| b.downcast::<DispatchLater<S>>().ok())
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
