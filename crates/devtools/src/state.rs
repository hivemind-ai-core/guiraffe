use crate::{StateDiff, diff};
use gpui_kit::App;
use guiraffe_state::{Event, EventRecord, Fx, Interceptor, Store};
use serde_json::Value;
use std::{cell::RefCell, collections::VecDeque, fmt, rc::Rc, sync::Arc};

/// One event and its immutable, structurally shared snapshots.
pub struct HistoryEntry<S> {
    pub event: EventRecord,
    pub before: Arc<S>,
    pub after: Arc<S>,
}

/// A bounded history. Serialization/diffing is deferred until a row is displayed.
pub struct History<S> {
    pub(crate) entries: VecDeque<HistoryEntry<S>>,
    capacity: usize,
}
impl<S> History<S> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub fn entries(&self) -> &VecDeque<HistoryEntry<S>> {
        &self.entries
    }
    pub fn snapshot(&self, seq: u64, before: bool) -> Option<Arc<S>> {
        self.entries.iter().find(|e| e.event.seq == seq).map(|e| {
            if before {
                e.before.clone()
            } else {
                e.after.clone()
            }
        })
    }
}

/// Install a clone as a Store interceptor; retain the handle to inspect it.
pub struct Recorder<S>(pub Rc<RefCell<History<S>>>);
impl<S> Clone for Recorder<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<S: 'static> Interceptor<S> for Recorder<S> {
    fn after(&mut self, event: &EventRecord, before: &Arc<S>, after: &Arc<S>) {
        let mut history = self.0.borrow_mut();
        if history.capacity == 0 {
            return;
        }
        if history.entries.len() == history.capacity {
            history.entries.pop_front();
        }
        history.entries.push_back(HistoryEntry {
            event: event.clone(),
            before: before.clone(),
            after: after.clone(),
        });
    }
}

struct Restore<S>(Arc<S>);
impl<S> fmt::Debug for Restore<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DevtoolsJump")
    }
}
impl<S: Clone + PartialEq + 'static> Event<S> for Restore<S> {
    fn handle(self, fx: &mut Fx<'_, S>) {
        fx.set((*self.0).clone());
    }
    fn undoable(&self) -> bool {
        false
    }
}

/// Event row for the type-erased panel.
#[derive(Clone)]
pub struct EventRow {
    pub seq: u64,
    pub description: String,
    pub changed: bool,
    pub changes: Vec<StateDiff>,
}

/// Adapter for heterogeneous stores in one panel.
pub trait InspectStore {
    fn name(&self) -> &str;
    fn rows(&self) -> Vec<EventRow>;
    /// Queue a normal, non-undoable restore event; effects are never replayed.
    fn jump(&self, seq: u64, before: bool, cx: &mut App) -> bool;
}

type Describe<S> = Rc<dyn Fn(&S) -> Value>;
struct StoreAdapter<S: Clone + PartialEq + 'static> {
    name: String,
    store: guiraffe_state::WeakStore<S>,
    history: Recorder<S>,
    describe: Describe<S>,
}
impl<S: Clone + PartialEq + 'static> InspectStore for StoreAdapter<S> {
    fn name(&self) -> &str {
        &self.name
    }
    fn rows(&self) -> Vec<EventRow> {
        self.history
            .0
            .borrow()
            .entries
            .iter()
            .rev()
            .take(8)
            .map(|e| EventRow {
                seq: e.event.seq,
                description: e.event.description.clone(),
                changed: e.event.changed,
                changes: diff(&(self.describe)(&e.before), &(self.describe)(&e.after)),
            })
            .collect()
    }
    fn jump(&self, seq: u64, before: bool, cx: &mut App) -> bool {
        let Some(snapshot) = self.history.0.borrow().snapshot(seq, before) else {
            return false;
        };
        let Some(store) = self.store.upgrade() else {
            return false;
        };
        store.dispatch(Restore(snapshot), cx);
        true
    }
}

/// Connect a Store using its public interceptor API. `describe` selects safe diagnostic data
/// (avoids a Serialize requirement and lets applications redact secrets).
/// Snapshot retention is Arc-only; restoring uses `S::clone` because Store's public Fx API
/// accepts an owned S. Use Arc/persistent fields in S as recommended by guiraffe-state.
pub fn inspect_store<S: Clone + PartialEq + 'static>(
    name: impl Into<String>,
    store: &Store<S>,
    capacity: usize,
    describe: impl Fn(&S) -> Value + 'static,
    cx: &mut App,
) -> (Rc<dyn InspectStore>, Recorder<S>) {
    let recorder = Recorder(Rc::new(RefCell::new(History::new(capacity))));
    store.add_interceptor(recorder.clone(), cx);
    let adapter = StoreAdapter {
        name: name.into(),
        store: store.downgrade(),
        history: recorder.clone(),
        describe: Rc::new(describe),
    };
    (Rc::new(adapter), recorder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_state::{Coeffects, Engine};
    #[derive(Debug)]
    struct Add;
    impl Event<u32> for Add {
        fn handle(self, fx: &mut Fx<'_, u32>) {
            fx.update(|n| *n += 1);
        }
    }
    #[test]
    fn snapshots_survive_eviction_and_restore_branches_history() {
        let recorder = Recorder(Rc::new(RefCell::new(History::new(2))));
        let mut engine = Engine::new(0u32);
        engine.add_interceptor(recorder.clone());
        engine.dispatch(Add);
        engine.process(&Coeffects::default());
        let saved = recorder
            .0
            .borrow()
            .snapshot(1, false)
            .unwrap_or_else(|| panic!("missing snapshot"));
        assert!(Arc::ptr_eq(&saved, engine.state()));
        for _ in 0..2 {
            engine.dispatch(Add);
            engine.process(&Coeffects::default());
        }
        assert!(recorder.0.borrow().snapshot(1, false).is_none());
        engine.dispatch(Restore(saved));
        engine.process(&Coeffects::default());
        assert_eq!(**engine.state(), 1);
        assert_eq!(recorder.0.borrow().entries().len(), 2);
        engine.dispatch(Add);
        engine.process(&Coeffects::default());
        assert_eq!(**engine.state(), 2);
    }
}
