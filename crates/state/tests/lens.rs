//! Lens writes and keyed lists: a write refreshes only the selections on its path, a
//! structural edit only the list and the item, every write carries its source, and
//! `store!` validation applies to every write path.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gpui_kit::{Context, IntoElement, Render, TestAppContext, Window, div};
use guiraffe_state::{
    Event, EventRecord, Fx, Interceptor, KeyedList, Lens, ListLens, Selected, Source, Store,
    UndoEvent, store,
};

#[derive(Clone, Debug, PartialEq, guiraffe_state::Lens)]
pub struct Clip {
    pub label: u32,
    pub start: u32,
}

store! {
    pub Project {
        title: u32 = 0,
        tempo: f32 = 120.0 where 20.0..=300.0,
        even: u32 = 0 where |v: &u32| v % 2 == 0,
        scroll: u32 = 0 #[no_undo],
        clips: KeyedList<u64, Clip> = KeyedList::new(),
    }
    events {
        Rename(title: u32) => |s| s.title = title,
        Reset => |s, fx| { *s = Project::default(); fx.effect(()) },
    }
}

#[derive(Debug)]
enum Edit {
    Insert(u64, usize),
    Remove(u64),
    Move(u64, usize),
}

impl Event<Project> for Edit {
    fn handle(self, fx: &mut Fx<'_, Project>) {
        match self {
            Edit::Insert(key, at) => {
                fx.list_insert(Project::clips, at, key, Clip { label: 0, start: 0 })
            }
            Edit::Remove(key) => {
                fx.list_remove(Project::clips, key);
            }
            Edit::Move(key, to) => {
                fx.list_move(Project::clips, key, to);
            }
        }
    }
}

#[derive(Default, Clone)]
struct Counts {
    labels: Rc<Cell<usize>>,
    list: Rc<Cell<usize>>,
    title: Rc<Cell<usize>>,
}

struct Probe {
    labels: Vec<Selected<Option<u32>>>,
    list: Selected<usize>,
    _title: Selected<u32>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

const WATCHED: [u64; 4] = [0, 1, 2, 3];

fn fixture(cx: &mut TestAppContext, n: u64) -> (Store<Project>, Counts, gpui_kit::Entity<Probe>) {
    let store = cx.update(|cx| {
        Store::new(
            Project {
                clips: (0..n).map(|k| (k, Clip { label: 0, start: 0 })).collect(),
                ..Project::default()
            },
            cx,
        )
    });
    let counts = Counts::default();
    let (probe, _) = cx.add_window_view(|_, cx| {
        let labels = WATCHED
            .iter()
            .map(|&k| {
                let lens = Project::clips.by_key(k).then(Clip::label);
                let visits = counts.labels.clone();
                store.select_at(lens, cx, move |state| {
                    visits.set(visits.get() + 1);
                    lens.get(state).copied()
                })
            })
            .collect();
        let visits = counts.list.clone();
        let list = store.select_at(Project::clips, cx, move |state| {
            visits.set(visits.get() + 1);
            state.clips.len()
        });
        let visits = counts.title.clone();
        let title = store.select_at(Project::title, cx, move |state| {
            visits.set(visits.get() + 1);
            state.title
        });
        Probe {
            labels,
            list,
            _title: title,
        }
    });
    (store, counts, probe)
}

fn reset(counts: &Counts) {
    counts.labels.set(0);
    counts.list.set(0);
    counts.title.set(0);
}

#[gpui_kit::test]
fn an_item_field_write_refreshes_that_item_and_its_prefixes_only(cx: &mut TestAppContext) {
    for n in [1_000, 10_000] {
        let (store, counts, probe) = fixture(cx, n);
        reset(&counts);
        let label = Project::clips.by_key(1).then(Clip::label);
        cx.update(|cx| store.set(label, 7, cx));
        // The edited label's selection, plus the list selection registered exactly at the
        // list's key (a prefix of the path). Not the other labels, not the title.
        assert_eq!(counts.labels.get(), 1, "{n} rows");
        assert_eq!(counts.list.get(), 1, "{n} rows");
        assert_eq!(counts.title.get(), 0);
        cx.update(|cx| {
            assert_eq!(probe.read(cx).labels[1].get(cx), &Some(7));
            assert_eq!(probe.read(cx).labels[0].get(cx), &Some(0));
        });
        // An equal write is no change at all.
        reset(&counts);
        cx.update(|cx| store.set(label, 7, cx));
        assert_eq!(counts.labels.get() + counts.list.get(), 0);
    }
}

#[gpui_kit::test]
fn structural_edits_notify_the_list_and_the_item_only(cx: &mut TestAppContext) {
    for n in [1_000, 10_000] {
        let (store, counts, probe) = fixture(cx, n);
        for (edit, label_refreshes) in [
            (Edit::Insert(n + 5, 0), 0),
            (Edit::Move(2, n as usize / 2), 1),
            (Edit::Remove(3), 1),
            (Edit::Insert(3, 0), 1),
        ] {
            reset(&counts);
            cx.update(|cx| store.dispatch_now(edit, cx));
            assert_eq!(counts.list.get(), 1, "{n} rows");
            assert_eq!(counts.labels.get(), label_refreshes, "{n} rows");
            assert_eq!(counts.title.get(), 0);
        }
        cx.update(|cx| {
            let probe = probe.read(cx);
            assert_eq!(*probe.list.get(cx), n as usize + 1);
            // The binding to clip 2 still reads clip 2 after it moved.
            assert_eq!(probe.labels[2].get(cx), &Some(0));
            assert_eq!(store.state(cx).clips.index_of(&2), Some(n as usize / 2));
        });
    }
}

#[gpui_kit::test]
fn an_unkeyed_write_refreshes_everything(cx: &mut TestAppContext) {
    let (store, counts, _probe) = fixture(cx, 100);
    reset(&counts);
    cx.update(|cx| store.dispatch_now(Rename(3), cx));
    assert_eq!(counts.labels.get(), WATCHED.len());
    assert_eq!(counts.list.get(), 1);
    assert_eq!(counts.title.get(), 1);
}

#[derive(Default)]
struct Log(Rc<RefCell<Vec<(String, Source, bool)>>>);

impl Interceptor<Project> for Log {
    fn after(&mut self, record: &EventRecord, _: &Arc<Project>, _: &Arc<Project>) {
        self.0
            .borrow_mut()
            .push((record.description.clone(), record.source, record.changed));
    }
}

#[gpui_kit::test]
fn writes_carry_their_source_and_respect_validation_and_undo(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let store = cx.update(|cx| {
        let store = Project::store(cx);
        store.add_interceptor(Log(log.clone()), cx);
        store.enable_undo(16, cx);
        store
    });
    cx.update(|cx| {
        store.set(Project::tempo, 500.0, cx);
        assert_eq!(store.state(cx).tempo, 300.0, "clamped by `where`");
        store.set_from(Project::even, 3, Source::Mcp, cx);
        assert_eq!(store.state(cx).even, 0, "rejected by the predicate");
        store.dispatch_now(SetEven(4), cx);
        assert_eq!(store.state(cx).even, 4);
        store.set(Project::scroll, 40, cx);
        store.set_from_control(Project::title, 9, cx);
        store.dispatch_now(UndoEvent, cx);
        assert_eq!(store.state(cx).title, 0, "undo restores the bound field");
        assert_eq!(store.state(cx).scroll, 40, "#[no_undo] writes skip history");
    });
    let log = log.borrow();
    assert_eq!(log[0], ("Set(tempo = 500.0)".into(), Source::Host, true));
    assert_eq!(log[1], ("Set(even = 3)".into(), Source::Mcp, false));
    assert_eq!(log[4].1, Source::Pointer, "a direct control edit");
}
