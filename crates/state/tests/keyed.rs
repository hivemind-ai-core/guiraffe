//! Keyed selections: a keyed field write refreshes only the selections of its
//! key (and unkeyed ones), while unkeyed writes, undo and redo refresh all.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{Context, IntoElement, Render, TestAppContext, Window, div};
use guiraffe_state::{Event, Fx, Selected, Store};

#[derive(Clone, Debug, Default, PartialEq)]
struct Rows {
    labels: Vec<u32>,
    title: u32,
}

#[derive(Debug)]
struct SetLabel(usize, u32);
impl Event<Rows> for SetLabel {
    fn handle(self, fx: &mut Fx<'_, Rows>) {
        let SetLabel(row, value) = self;
        fx.set_keyed_field(
            row as u64,
            value,
            move |state| &state.labels[row],
            move |state| &mut state.labels[row],
        );
    }
}

#[derive(Debug)]
struct SetAll(u32);
impl Event<Rows> for SetAll {
    fn handle(self, fx: &mut Fx<'_, Rows>) {
        fx.update(|state| state.labels.iter_mut().for_each(|label| *label = self.0));
    }
}

#[derive(Debug)]
struct UndoableLabel(usize, u32);
impl Event<Rows> for UndoableLabel {
    fn handle(self, fx: &mut Fx<'_, Rows>) {
        let UndoableLabel(row, value) = self;
        fx.set_keyed_field(
            row as u64,
            value,
            move |state| &state.labels[row],
            move |state| &mut state.labels[row],
        );
    }
    fn undoable(&self) -> bool {
        true
    }
}

struct View {
    rows: Vec<Selected<u32>>,
    all: Selected<u32>,
}

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Fixture {
    store: Store<Rows>,
    view: gpui_kit::Entity<View>,
    calls: Vec<Rc<Cell<usize>>>,
    all_calls: Rc<Cell<usize>>,
}

fn fixture(cx: &mut TestAppContext, n: usize) -> (Fixture, &mut gpui_kit::VisualTestContext) {
    let store = cx.update(|cx| {
        Store::new(
            Rows {
                labels: vec![0; n],
                title: 0,
            },
            cx,
        )
    });
    let calls: Vec<_> = (0..n).map(|_| Rc::new(Cell::new(0))).collect();
    let all_calls = Rc::new(Cell::new(0));
    let (s, c, a) = (store.clone(), calls.clone(), all_calls.clone());
    let (view, vcx) = cx.add_window_view(move |_, cx| View {
        rows: (0..n)
            .map(|row| {
                let calls = c[row].clone();
                s.select_key(row as u64, cx, move |state: &Rows| {
                    calls.set(calls.get() + 1);
                    state.labels[row]
                })
            })
            .collect(),
        all: s.select(cx, move |state: &Rows| {
            a.set(a.get() + 1);
            state.labels.iter().sum::<u32>() + state.title
        }),
    });
    vcx.run_until_parked();
    (
        Fixture {
            store,
            view,
            calls,
            all_calls,
        },
        vcx,
    )
}

fn reset(f: &Fixture) {
    f.calls.iter().for_each(|c| c.set(0));
    f.all_calls.set(0);
}

fn values(f: &Fixture, cx: &mut gpui_kit::VisualTestContext) -> (Vec<u32>, u32) {
    cx.update(|_, cx| {
        let view = f.view.read(cx);
        (
            view.rows.iter().map(|row| *row.get(cx)).collect(),
            *view.all.get(cx),
        )
    })
}

#[gpui_kit::test]
fn a_keyed_write_refreshes_only_its_key_and_unkeyed_selections(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, 100);
    reset(&f);
    vcx.update(|_, cx| f.store.dispatch(SetLabel(7, 5), cx));
    vcx.run_until_parked();
    let refreshed: Vec<usize> = (0..100).filter(|&row| f.calls[row].get() > 0).collect();
    assert_eq!(refreshed, vec![7]);
    assert_eq!(
        f.all_calls.get(),
        1,
        "unkeyed selections observe keyed writes"
    );
    let (rows, all) = values(&f, vcx);
    assert_eq!(rows[7], 5);
    assert_eq!(all, 5);
}

#[gpui_kit::test]
fn an_equal_keyed_write_refreshes_nothing(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, 10);
    reset(&f);
    vcx.update(|_, cx| f.store.dispatch(SetLabel(3, 0), cx));
    vcx.run_until_parked();
    assert!(f.calls.iter().all(|c| c.get() == 0));
    assert_eq!(f.all_calls.get(), 0);
}

#[gpui_kit::test]
fn an_unkeyed_write_refreshes_every_keyed_selection(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, 10);
    reset(&f);
    vcx.update(|_, cx| f.store.dispatch(SetAll(2), cx));
    vcx.run_until_parked();
    assert!(f.calls.iter().all(|c| c.get() == 1));
    assert_eq!(values(&f, vcx), (vec![2; 10], 20));
}

#[gpui_kit::test]
fn undo_of_a_keyed_write_refreshes_keyed_selections(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, 10);
    vcx.update(|_, cx| f.store.enable_undo(16, cx));
    vcx.update(|_, cx| f.store.dispatch(UndoableLabel(4, 9), cx));
    vcx.run_until_parked();
    assert_eq!(values(&f, vcx).0[4], 9);
    vcx.update(|_, cx| f.store.dispatch(guiraffe_state::UndoEvent, cx));
    vcx.run_until_parked();
    assert_eq!(values(&f, vcx), (vec![0; 10], 0));
}

#[gpui_kit::test]
fn keyed_writes_in_one_batch_refresh_each_key(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, 10);
    reset(&f);
    vcx.update(|_, cx| {
        f.store.dispatch(SetLabel(1, 1), cx);
        f.store.dispatch(SetLabel(8, 8), cx);
        f.store.dispatch(SetLabel(1, 3), cx);
    });
    vcx.run_until_parked();
    let refreshed: Vec<usize> = (0..10).filter(|&row| f.calls[row].get() > 0).collect();
    assert_eq!(refreshed, vec![1, 8]);
    let (rows, all) = values(&f, vcx);
    assert_eq!((rows[1], rows[8], all), (3, 8, 11));
}

#[gpui_kit::test]
fn keyed_selections_released_leave_no_group_behind(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, 10);
    assert_eq!(vcx.update(|_, cx| f.store.stats(cx).selections), 11);
    vcx.update(|_, cx| f.view.update(cx, |view, _| view.rows.clear()));
    assert_eq!(vcx.update(|_, cx| f.store.stats(cx).selections), 1);
    reset(&f);
    vcx.update(|_, cx| f.store.dispatch(SetLabel(2, 4), cx));
    vcx.run_until_parked();
    assert_eq!(f.all_calls.get(), 1);
}
