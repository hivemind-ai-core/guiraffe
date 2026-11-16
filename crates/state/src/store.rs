//! The GPUI side: a [`Store`] entity that batches dispatches, runs effects, and keeps
//! memoized [`Selected`] values that notify *only* the views whose selection changed.

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

use gpui_kit::{App, AppContext, Context, Entity, Subscription, WeakEntity};

use crate::engine::{Coeffects, DispatchLater, Engine, EngineError, Event, Interceptor};

type EffectHandler<S> = Rc<dyn Fn(Box<dyn Any>, &Store<S>, &mut App)>;

trait Slot<S> {
    /// Recompute against `state`; notify the backing entity if the value changed.
    /// Returns false once the selection's entity is gone (slot can be dropped).
    fn refresh(&self, state: &S, cx: &mut App) -> bool;
}

struct Inner<S: Clone + PartialEq + 'static> {
    engine: Engine<S>,
    cofx: Rc<RefCell<Coeffects>>,
    effects: HashMap<TypeId, EffectHandler<S>>,
    slots: Vec<Rc<dyn Slot<S>>>,
    scheduled: bool,
    processing: bool,
    last_error: Option<EngineError>,
    unhandled_effects: usize,
    events_processed: u64,
}

/// An event-driven state container. Cheap to clone (it is a handle).
pub struct Store<S: Clone + PartialEq + 'static> {
    inner: Entity<Inner<S>>,
}

impl<S: Clone + PartialEq + 'static> Clone for Store<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Diagnostics for devtools and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStats {
    pub events_processed: u64,
    pub selections: usize,
    pub unhandled_effects: usize,
    pub last_error: Option<EngineError>,
}

impl<S: Clone + PartialEq + 'static> Store<S> {
    pub fn new(state: S, cx: &mut App) -> Self {
        let inner = cx.new(|_| Inner {
            engine: Engine::new(state),
            cofx: Rc::new(RefCell::new(Coeffects::default())),
            effects: HashMap::new(),
            slots: Vec::new(),
            scheduled: false,
            processing: false,
            last_error: None,
            unhandled_effects: 0,
            events_processed: 0,
        });
        Self { inner }
    }

    /// Current state snapshot (cheap `Arc` clone). Views should prefer [`Store::select`].
    pub fn state(&self, cx: &App) -> Arc<S> {
        self.inner.read(cx).engine.state().clone()
    }

    pub fn enable_undo(&self, limit: usize, cx: &mut App) {
        self.inner.update(cx, |i, _| i.engine.enable_undo(limit));
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

    /// Provide a coeffect value readable by handlers via `fx.cofx::<T>()`.
    pub fn provide<T: 'static>(&self, value: T, cx: &mut App) {
        self.inner
            .update(cx, |i, _| i.cofx.borrow_mut().provide(value));
    }

    /// Register the executor for effects of type `E`.
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

    /// Queue an event. All events dispatched in the same tick are processed together
    /// (one refresh of selections, so views re-render at most once).
    pub fn dispatch(&self, event: impl Event<S>, cx: &mut App) {
        let schedule = self.inner.update(cx, |i, _| {
            i.engine.dispatch(event);
            let s = !i.scheduled && !i.processing;
            i.scheduled = true;
            s
        });
        if schedule {
            let store = self.clone();
            cx.defer(move |cx| store.flush(cx));
        }
    }

    /// Process everything queued now (tests, or when an immediate result is needed).
    /// Reentrant calls (from an effect handler) are folded into the running flush.
    pub fn flush(&self, cx: &mut App) {
        if self.inner.read(cx).processing {
            return;
        }
        loop {
            let (outcome, state) = self.inner.update(cx, |i, cx| {
                i.scheduled = false;
                i.processing = true;
                let cofx = i.cofx.clone();
                let mut cofx = cofx.borrow_mut();
                cofx.now = Some(cx.background_executor().now());
                let out = i.engine.process(&cofx);
                i.events_processed += out.processed as u64;
                if let Some(e) = &out.error {
                    i.last_error = Some(e.clone());
                }
                (out, i.engine.state().clone())
            });
            if outcome.changed {
                let slots: Vec<Rc<dyn Slot<S>>> = self.inner.read(cx).slots.clone();
                let mut dead = Vec::new();
                for (idx, slot) in slots.iter().enumerate() {
                    if !slot.refresh(&state, cx) {
                        dead.push(idx);
                    }
                }
                if !dead.is_empty() {
                    self.inner.update(cx, |i, _| {
                        let mut idx = 0;
                        i.slots.retain(|_| {
                            let keep = !dead.contains(&idx);
                            idx += 1;
                            keep
                        });
                    });
                }
            }
            for effect in outcome.effects {
                self.run_effect(effect, cx);
            }
            let again = self.inner.update(cx, |i, _| {
                i.processing = false;
                i.engine.has_pending()
            });
            if !again {
                break;
            }
        }
    }

    fn run_effect(&self, effect: Box<dyn Any>, cx: &mut App) {
        let effect = match effect.downcast::<DispatchLater<S>>() {
            Ok(later) => {
                let later = *later;
                let delay = later.delay;
                let store = self.clone();
                let queued = later.into_queued();
                let timer = cx.background_executor().timer(delay);
                cx.spawn(async move |cx| {
                    timer.await;
                    cx.update(|cx| {
                        store
                            .inner
                            .update(cx, |i, _| i.engine.dispatch_queued(queued));
                        store.flush(cx);
                    });
                })
                .detach();
                return;
            }
            Err(other) => other,
        };
        let ty = (*effect).type_id();
        let handler = self.inner.read(cx).effects.get(&ty).cloned();
        match handler {
            Some(h) => h(effect, self, cx),
            None => self.inner.update(cx, |i, _| i.unhandled_effects += 1),
        }
    }

    pub fn stats(&self, cx: &App) -> StoreStats {
        let i = self.inner.read(cx);
        StoreStats {
            events_processed: i.events_processed,
            selections: i.slots.len(),
            unhandled_effects: i.unhandled_effects,
            last_error: i.last_error.clone(),
        }
    }

    /// Select a value for the view `V`. The view is notified only when the selected
    /// value changes (`PartialEq`), never for unrelated state changes.
    pub fn select<V: 'static, T: Clone + PartialEq + 'static>(
        &self,
        cx: &mut Context<V>,
        f: impl Fn(&S) -> T + 'static,
    ) -> Selected<T> {
        self.select_memo(cx, f, |input: &T| input.clone())
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
        let state = self.state(cx);
        let first_input = input(&state);
        let value = compute(&first_input);
        let entity = cx.new(|_| SelectionCell { value });
        let slot: Rc<dyn Slot<S>> = Rc::new(MemoSlot {
            entity: entity.downgrade(),
            last_input: RefCell::new(first_input),
            input: Box::new(input),
            compute: Box::new(compute),
        });
        self.inner.update(cx, |i, _| i.slots.push(slot));
        let observation = cx.observe(&entity, |_, _, cx| cx.notify());
        Selected {
            entity,
            _observation: observation,
        }
    }
}

struct SelectionCell<T> {
    value: T,
}

struct MemoSlot<S, I, T: 'static> {
    entity: WeakEntity<SelectionCell<T>>,
    last_input: RefCell<I>,
    input: Box<dyn Fn(&S) -> I>,
    compute: Box<dyn Fn(&I) -> T>,
}

impl<S, I: PartialEq, T: PartialEq + 'static> Slot<S> for MemoSlot<S, I, T> {
    fn refresh(&self, state: &S, cx: &mut App) -> bool {
        let Some(entity) = self.entity.upgrade() else {
            return false;
        };
        let input = (self.input)(state);
        if *self.last_input.borrow() == input {
            return true;
        }
        let value = (self.compute)(&input);
        *self.last_input.borrow_mut() = input;
        entity.update(cx, |cell, cx| {
            if cell.value != value {
                cell.value = value;
                cx.notify();
            }
        });
        true
    }
}

/// A memoized selection held by a view. Dropping it unsubscribes.
pub struct Selected<T: 'static> {
    entity: Entity<SelectionCell<T>>,
    _observation: Subscription,
}

impl<T: 'static> Selected<T> {
    pub fn get<'a>(&self, cx: &'a App) -> &'a T {
        &self.entity.read(cx).value
    }
}
