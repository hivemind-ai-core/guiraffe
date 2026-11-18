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

/// Called after every recorded event (e.g. to refresh an open panel).
pub type RecordHook = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Install a clone as a Store interceptor; retain the handle to inspect it.
pub struct Recorder<S>(pub Rc<RefCell<History<S>>>, pub RecordHook);
impl<S> Clone for Recorder<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
impl<S> Recorder<S> {
    pub fn new(capacity: usize) -> Self {
        Self(Rc::new(RefCell::new(History::new(capacity))), Rc::default())
    }
    /// Seq of the newest recorded event (0 when empty).
    pub fn latest_seq(&self) -> u64 {
        self.0.borrow().entries.back().map_or(0, |e| e.event.seq)
    }
}
impl<S: 'static> Interceptor<S> for Recorder<S> {
    fn after(&mut self, event: &EventRecord, before: &Arc<S>, after: &Arc<S>) {
        {
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
        // Clone the hook out so it may freely touch the recorder.
        let hook = self.1.borrow().clone();
        if let Some(hook) = hook {
            hook();
        }
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

/// The first row of the last page: an "Older" offset never runs past a store's history.
pub fn clamp_offset(offset: usize, len: usize, page: usize) -> usize {
    if len == 0 || page == 0 {
        return 0;
    }
    offset.min((len - 1) / page * page)
}

/// Adapter for heterogeneous stores in one panel.
pub trait InspectStore {
    fn name(&self) -> &str;
    /// Recorded events currently retained.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Newest-first rows of one page; `offset` is clamped to this store's last page.
    /// Diffs are cached per (latest event, offset, limit), so an unchanged page costs
    /// no serialization or diffing on re-render.
    fn rows(&self, offset: usize, limit: usize) -> Rc<Vec<EventRow>>;
    /// Install (or clear) a callback run after each recorded event.
    fn set_on_record(&self, _hook: Option<Rc<dyn Fn()>>) {}
    /// Queue a normal, non-undoable restore event; effects are never replayed.
    fn jump(&self, seq: u64, before: bool, cx: &mut App) -> bool;
}

type Describe<S> = Rc<dyn Fn(&S) -> Value>;
type RowKey = (u64, usize, usize, usize);
struct StoreAdapter<S: Clone + PartialEq + 'static> {
    name: String,
    store: guiraffe_state::WeakStore<S>,
    history: Recorder<S>,
    describe: Describe<S>,
    cache: RefCell<Option<(RowKey, Rc<Vec<EventRow>>)>>,
}
impl<S: Clone + PartialEq + 'static> InspectStore for StoreAdapter<S> {
    fn name(&self) -> &str {
        &self.name
    }
    fn len(&self) -> usize {
        self.history.0.borrow().entries.len()
    }
    fn rows(&self, offset: usize, limit: usize) -> Rc<Vec<EventRow>> {
        let len = self.len();
        let offset = clamp_offset(offset, len, limit);
        let key = (self.history.latest_seq(), len, offset, limit);
        if let Some((cached, rows)) = &*self.cache.borrow()
            && *cached == key
        {
            return rows.clone();
        }
        let rows: Rc<Vec<EventRow>> = Rc::new(
            self.history
                .0
                .borrow()
                .entries
                .iter()
                .rev()
                .skip(offset)
                .take(limit)
                .map(|e| EventRow {
                    seq: e.event.seq,
                    description: e.event.description.clone(),
                    changed: e.event.changed,
                    changes: diff(&(self.describe)(&e.before), &(self.describe)(&e.after)),
                })
                .collect(),
        );
        *self.cache.borrow_mut() = Some((key, rows.clone()));
        rows
    }
    fn set_on_record(&self, hook: Option<Rc<dyn Fn()>>) {
        *self.history.1.borrow_mut() = hook;
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
    let recorder = Recorder::new(capacity);
    store.add_interceptor(recorder.clone(), cx);
    let adapter = StoreAdapter {
        name: name.into(),
        store: store.downgrade(),
        history: recorder.clone(),
        describe: Rc::new(describe),
        cache: RefCell::new(None),
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
        let recorder = Recorder::new(2);
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
    #[test]
    fn offsets_clamp_to_each_stores_last_page() {
        assert_eq!(clamp_offset(0, 0, 8), 0);
        assert_eq!(
            clamp_offset(16, 5, 8),
            0,
            "short store stays on its only page"
        );
        assert_eq!(clamp_offset(16, 20, 8), 16);
        assert_eq!(clamp_offset(24, 20, 8), 16);
        assert_eq!(clamp_offset(8, 8, 8), 0);
    }
    #[gpui_kit::test]
    fn rows_are_cached_until_a_new_event_and_hook_fires(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            let store = Store::new(0u32, cx);
            let describes = Rc::new(std::cell::Cell::new(0));
            let count = describes.clone();
            let (inspect, _) = inspect_store(
                "counter",
                &store,
                32,
                move |n| {
                    count.set(count.get() + 1);
                    serde_json::json!({ "n": n })
                },
                cx,
            );
            let recorded = Rc::new(std::cell::Cell::new(0));
            let hits = recorded.clone();
            inspect.set_on_record(Some(Rc::new(move || hits.set(hits.get() + 1))));
            store.dispatch(Add, cx);
            store.flush(cx);
            assert_eq!(recorded.get(), 1, "recorder runs the hook per event");
            let first = inspect.rows(0, 8);
            let after_first = describes.get();
            assert_eq!(after_first, 2, "one before/after pair described");
            let again = inspect.rows(0, 8);
            assert!(Rc::ptr_eq(&first, &again), "unchanged page is cached");
            assert_eq!(describes.get(), after_first, "no re-diff on re-render");
            // An out-of-range offset clamps to the same (only) page: still cached.
            assert!(Rc::ptr_eq(&first, &inspect.rows(40, 8)));
            store.dispatch(Add, cx);
            store.flush(cx);
            let fresh = inspect.rows(0, 8);
            assert_eq!(fresh.len(), 2);
            assert!(describes.get() > after_first, "new event recomputes");
        });
    }
}
