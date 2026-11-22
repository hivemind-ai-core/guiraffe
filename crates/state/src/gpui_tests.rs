use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext, Window, div,
};

use crate::*;

#[derive(Clone, Debug, Default, PartialEq)]
struct App {
    a: i32,
    b: i32,
    items: Vec<i32>,
}

#[derive(Debug)]
struct IncA;
impl Event<App> for IncA {
    fn handle(self, fx: &mut Fx<'_, App>) {
        fx.update(|s| s.a += 1);
    }
}

#[derive(Debug)]
struct IncB;
impl Event<App> for IncB {
    fn handle(self, fx: &mut Fx<'_, App>) {
        fx.update(|s| s.b += 1);
    }
}

#[derive(Debug)]
struct Push(i32);
impl Event<App> for Push {
    fn handle(self, fx: &mut Fx<'_, App>) {
        fx.update(|s| s.items.push(self.0));
    }
}

/// A view showing one selected field, counting its renders.
struct Field {
    value: Selected<i32>,
    renders: Rc<Cell<usize>>,
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().child(self.value.get(cx).to_string())
    }
}

struct Page {
    a: Entity<Field>,
    b: Entity<Field>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.a.clone()).child(self.b.clone())
    }
}

struct Fixture {
    store: Store<App>,
    a: Entity<Field>,
    ra: Rc<Cell<usize>>,
    rb: Rc<Cell<usize>>,
}

fn fixture(cx: &mut TestAppContext) -> (Fixture, &mut gpui_kit::VisualTestContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    let (ra, rb) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (s, ra2, rb2) = (store.clone(), ra.clone(), rb.clone());
    let mut a_out = None;
    let (_page, vcx) = cx.add_window_view(|_, cx| {
        let s2 = s.clone();
        let a = cx.new(|cx| Field {
            value: s.select(cx, |st: &App| st.a),
            renders: ra2,
        });
        let b = cx.new(|cx| Field {
            value: s2.select(cx, |st: &App| st.b),
            renders: rb2,
        });
        a_out = Some(a.clone());
        Page { a, b }
    });
    let a = a_out.unwrap_or_else(|| unreachable!());
    vcx.run_until_parked();
    (Fixture { store, a, ra, rb }, vcx)
}

#[gpui_kit::test]
fn only_views_whose_selection_changed_rerender(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx);
    let (a0, b0) = (f.ra.get(), f.rb.get());
    vcx.update(|_, cx| f.store.dispatch(IncA, cx));
    vcx.run_until_parked();
    assert_eq!(f.store.state(&vcx.app.borrow()).a, 1);
    assert!(f.ra.get() > a0, "view A re-rendered");
    if gpui_kit::GPUI_FAST {
        assert_eq!(f.rb.get(), b0, "view B untouched");
    }
    // An unrelated change never notifies A's selection.
    let a1 = f.ra.get();
    vcx.update(|_, cx| f.store.dispatch(Push(1), cx));
    vcx.run_until_parked();
    if gpui_kit::GPUI_FAST {
        assert_eq!(f.ra.get(), a1, "unrelated change: no re-render of A");
    }
    let observed = vcx.update(|_, cx| f.a.read(cx).value.get(cx).to_owned());
    assert_eq!(observed, 1);
}

#[gpui_kit::test]
fn selection_notifies_only_on_value_change(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx);
    let notifies = Rc::new(Cell::new(0));
    let n = notifies.clone();
    let _sub = vcx.update(|_, cx| cx.observe(&f.a, move |_, _| n.set(n.get() + 1)));
    vcx.update(|_, cx| f.store.dispatch(IncB, cx));
    vcx.run_until_parked();
    assert_eq!(notifies.get(), 0, "B changed; A's view not notified");
    vcx.update(|_, cx| f.store.dispatch(IncA, cx));
    vcx.run_until_parked();
    assert_eq!(notifies.get(), 1);
}

#[gpui_kit::test]
fn dispatches_in_one_tick_are_batched(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx);
    let notifies = Rc::new(Cell::new(0));
    let n = notifies.clone();
    let _sub = vcx.update(|_, cx| cx.observe(&f.a, move |_, _| n.set(n.get() + 1)));
    vcx.update(|_, cx| {
        for _ in 0..5 {
            f.store.dispatch(IncA, cx);
        }
    });
    vcx.run_until_parked();
    let st = vcx.update(|_, cx| (f.store.state(cx).a, f.store.stats(cx).events_processed));
    assert_eq!(st, (5, 5));
    assert_eq!(notifies.get(), 1, "five events, one notification");
}

#[derive(Debug, PartialEq)]
struct Save(i32);

#[derive(Debug)]
struct SaveA;
impl Event<App> for SaveA {
    fn handle(self, fx: &mut Fx<'_, App>) {
        let a = fx.state().a;
        fx.effect(Save(a));
    }
}

#[gpui_kit::test]
fn effects_run_and_may_dispatch_reentrantly(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx);
    let saved = Rc::new(Cell::new(-1));
    let sv = saved.clone();
    vcx.update(|_, cx| {
        f.store.on_effect::<Save>(cx, move |Save(n), store, cx| {
            sv.set(n);
            store.dispatch(IncB, cx); // reentrant dispatch from an effect
        });
        f.store.dispatch(IncA, cx);
        f.store.dispatch(SaveA, cx);
    });
    vcx.run_until_parked();
    assert_eq!(saved.get(), 1);
    assert_eq!(vcx.update(|_, cx| f.store.state(cx).b), 1);
    #[derive(Debug)]
    struct Unknown;
    impl Event<App> for Unknown {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.effect(42u64);
        }
    }
    vcx.update(|_, cx| f.store.dispatch(Unknown, cx));
    vcx.run_until_parked();
    let unhandled = vcx.update(|_, cx| f.store.stats(cx).unhandled_effects);
    assert_eq!(
        unhandled,
        vec!["u64"],
        "unhandled effect is recorded by type name, not a panic"
    );
}

#[gpui_kit::test]
fn dispatch_later_uses_the_app_clock(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx);
    #[derive(Debug)]
    struct Later;
    impl Event<App> for Later {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.dispatch_later(Duration::from_millis(300), IncA);
        }
    }
    vcx.update(|_, cx| f.store.dispatch(Later, cx));
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| f.store.state(cx).a), 0);
    vcx.executor().advance_clock(Duration::from_millis(301));
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| f.store.state(cx).a), 1);
}

#[gpui_kit::test]
fn memo_selection_computes_only_when_input_changes(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    let computes = Rc::new(Cell::new(0));
    let c = computes.clone();
    let s = store.clone();
    struct Sum(Selected<i32>);
    impl Render for Sum {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().child(self.0.get(cx).to_string())
        }
    }
    let (view, vcx) = cx.add_window_view(move |_, cx| {
        Sum(s.select_memo(
            cx,
            |st: &App| st.items.clone(),
            move |items: &Vec<i32>| {
                c.set(c.get() + 1);
                items.iter().sum()
            },
        ))
    });
    assert_eq!(computes.get(), 1);
    vcx.update(|_, cx| store.dispatch(IncA, cx));
    vcx.run_until_parked();
    assert_eq!(
        computes.get(),
        1,
        "input unchanged: expensive compute skipped"
    );
    vcx.update(|_, cx| {
        store.dispatch(Push(2), cx);
        store.dispatch(Push(3), cx);
    });
    vcx.run_until_parked();
    assert_eq!(computes.get(), 2, "batched: computed once for two pushes");
    assert_eq!(vcx.update(|_, cx| *view.read(cx).0.get(cx)), 5);
}

#[gpui_kit::test]
fn undo_through_the_store_updates_selections(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx);
    vcx.update(|_, cx| {
        f.store.enable_undo(10, cx);
        f.store.dispatch(IncA, cx);
        f.store.dispatch(IncA, cx);
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| f.store.dispatch(UndoEvent, cx));
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| *f.a.read(cx).value.get(cx)), 1);
    assert!(vcx.update(|_, cx| f.store.can_redo(cx)));
}

#[gpui_kit::test]
fn dropped_views_release_their_selections(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    let s = store.clone();
    let field = cx.update(|cx| {
        cx.open_window(Default::default(), |_, cx| {
            cx.new(|cx| Field {
                value: s.select(cx, |st: &App| st.a),
                renders: Rc::new(Cell::new(0)),
            })
        })
    });
    let Ok(handle) = field else { panic!("window") };
    assert_eq!(cx.update(|cx| store.stats(cx).selections), 1);
    cx.update(|cx| handle.update(cx, |_, w, _| w.remove_window()))
        .ok();
    cx.run_until_parked();
    cx.update(|cx| {
        store.dispatch(IncA, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| store.stats(cx).selections),
        0,
        "dead slots are pruned on refresh"
    );
}

#[gpui_kit::test]
fn dispatch_loop_is_reported_not_hung(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    #[derive(Debug)]
    struct Forever;
    impl Event<App> for Forever {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.dispatch(Forever);
        }
    }
    cx.update(|cx| store.dispatch(Forever, cx));
    cx.run_until_parked();
    let err = cx.update(|cx| store.stats(cx).last_error);
    assert!(matches!(err, Some(EngineError::CascadeLimit { .. })));
}

#[gpui_kit::test]
fn panic_in_an_effect_does_not_wedge_the_store(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    #[derive(Debug)]
    struct Boom;
    #[derive(Debug)]
    struct Explode;
    impl Event<App> for Explode {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.effect(Boom);
        }
    }
    cx.update(|cx| store.on_effect::<Boom>(cx, |_, _, _| panic!("effect exploded")));
    let s2 = store.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cx.update(|cx| {
            s2.dispatch(Explode, cx);
            s2.flush(cx);
        })
    }));
    assert!(result.is_err());
    cx.update(|cx| {
        store.dispatch(IncA, cx);
        store.flush(cx);
    });
    assert_eq!(
        cx.update(|cx| store.state(cx).a),
        1,
        "store keeps working after a panic"
    );
}

#[gpui_kit::test]
fn loops_through_effects_are_caught(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    #[derive(Debug)]
    struct Again;
    #[derive(Debug)]
    struct Ping;
    impl Event<App> for Ping {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.effect(Again);
        }
    }
    let errors = Rc::new(Cell::new(0));
    let e2 = errors.clone();
    cx.update(|cx| {
        store.set_cascade_limit(50, cx);
        store.on_effect::<Again>(cx, |_, store, cx| store.dispatch(Ping, cx));
        store.on_error(cx, move |_, _| e2.set(e2.get() + 1));
        store.dispatch(Ping, cx);
    });
    cx.run_until_parked();
    assert_eq!(errors.get(), 1);
    assert!(matches!(
        cx.update(|cx| store.stats(cx).last_error),
        Some(EngineError::CascadeLimit { .. })
    ));
}

#[gpui_kit::test]
fn store_is_freed_when_handles_drop_even_with_effect_handlers_and_timers(cx: &mut TestAppContext) {
    #[derive(Debug)]
    struct Later;
    impl Event<App> for Later {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.dispatch_later(Duration::from_secs(30), IncA);
        }
    }
    let weak = cx.update(|cx| {
        let store = Store::new(App::default(), cx);
        store.on_effect::<Save>(cx, |_, store, cx| store.dispatch(IncB, cx));
        store.dispatch(Later, cx);
        store.flush(cx);
        store.downgrade()
    });
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "no cycle: handler uses &Store, timer holds a weak handle"
    );
    cx.executor().advance_clock(Duration::from_secs(31));
    cx.run_until_parked(); // the timer fires against a dead store without panicking
}

#[gpui_kit::test]
fn debounced_dispatch_keeps_only_the_latest(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    #[derive(Debug)]
    struct Typed;
    impl Event<App> for Typed {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.dispatch_debounced("search", Duration::from_millis(200), IncA);
        }
    }
    for _ in 0..5 {
        cx.update(|cx| store.dispatch(Typed, cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(50));
    }
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| store.state(cx).a),
        1,
        "five keystrokes, one search"
    );
}

#[gpui_kit::test]
fn selections_created_repeatedly_do_not_pile_up(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    let s = store.clone();
    let (view, vcx) = cx.add_window_view(move |_, cx| Field {
        value: s.select(cx, |st: &App| st.a),
        renders: Rc::new(Cell::new(0)),
    });
    for _ in 0..20 {
        vcx.update(|_, cx| {
            view.update(cx, |field, cx| {
                field.value = store.select(cx, |st: &App| st.a)
            });
        });
    }
    assert_eq!(
        vcx.update(|_, cx| store.stats(cx).selections),
        1,
        "replaced selections are pruned on registration"
    );
}

#[gpui_kit::test]
fn synchronous_writes_preserve_history_interceptors_and_effect_order(cx: &mut TestAppContext) {
    use std::{cell::RefCell, sync::Arc};

    #[derive(Debug)]
    struct SetA(i32);

    impl Event<App> for SetA {
        fn handle(self, fx: &mut Fx<'_, App>) {
            fx.set_field(self.0, |state| &state.a, |state, value| state.a = value);
        }
    }

    type RecordedEvent = (u64, String, bool);
    struct Observe(Rc<RefCell<Vec<RecordedEvent>>>);

    impl Interceptor<App> for Observe {
        fn after(&mut self, record: &EventRecord, before: &Arc<App>, after: &Arc<App>) {
            assert_eq!(record.changed, before != after);
            self.0
                .borrow_mut()
                .push((record.seq, record.description.clone(), record.changed));
        }
    }

    let (f, window) = fixture(cx);
    let events = Rc::new(RefCell::new(Vec::new()));
    window.update(|_, cx| {
        f.store.enable_undo(8, cx);
        f.store.add_interceptor(Observe(events.clone()), cx);
        let snapshot = f.store.state(cx);
        f.store.dispatch_now(SetA(0), cx);
        assert!(Arc::ptr_eq(&snapshot, &f.store.state(cx)));
        assert!(!f.store.can_undo(cx));

        f.store.dispatch_now(SetA(1), cx);
        assert_eq!(f.store.state(cx).a, 1);
        f.store.dispatch_now(UndoEvent, cx);
        assert_eq!(f.store.state(cx).a, 0);
        f.store.dispatch_now(SetA(0), cx);
        assert!(f.store.can_redo(cx));
        f.store.dispatch_now(RedoEvent, cx);
        assert_eq!(f.store.state(cx).a, 1);

        f.store.on_effect::<Save>(cx, |Save(value), store, cx| {
            assert_eq!(value, 2);
            store.dispatch_now(SetA(3), cx);
            assert_eq!(
                store.state(cx).a,
                2,
                "effect appends the event to the batch"
            );
        });
        f.store.dispatch(SetA(2), cx);
        f.store.dispatch_now(SaveA, cx);
        assert_eq!(
            f.store.state(cx).a,
            1,
            "the earlier batch still owns dispatch order"
        );
    });

    assert_eq!(window.update(|_, cx| f.store.state(cx).a), 3);
    let description = [
        "SetA(0)",
        "SetA(1)",
        "UndoEvent",
        "SetA(0)",
        "RedoEvent",
        "SetA(2)",
        "SaveA",
        "SetA(3)",
    ];
    let changed = [false, true, true, false, true, true, false, true];
    assert_eq!(events.borrow().len(), description.len());
    for (index, (seq, text, change)) in events.borrow().iter().enumerate() {
        assert_eq!(
            (*seq, text.as_str(), *change),
            (index as u64 + 1, description[index], changed[index])
        );
    }
}
