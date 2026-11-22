//! Warm-repaint allocation gate: a view that reads a memoised selection and
//! renders unchanged must not allocate more than the previous frame.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};
use guiraffe_state::{Event, Fx, Selected, Store};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Clone, Debug, Default, PartialEq)]
struct App {
    a: i32,
    items: Vec<i32>,
}

#[derive(Debug)]
struct IncA;
impl Event<App> for IncA {
    fn handle(self, fx: &mut Fx<'_, App>) {
        fx.update(|s| s.a += 1);
    }
}

struct Field {
    value: Selected<i32>,
    sum: Selected<i32>,
    renders: Rc<Cell<usize>>,
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
            .w(px(200.))
            .child(self.value.get(cx).to_string())
            .child(self.sum.get(cx).to_string())
    }
}

#[gpui_kit::test]
fn warm_selected_view_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let store = cx.update(|cx| Store::new(App::default(), cx));
    let renders = Rc::new(Cell::new(0));
    let (r, s) = (renders.clone(), store.clone());
    let (_view, vcx) = cx.add_window_view(move |_, cx| {
        let sum = s.select_memo(
            cx,
            |app: &App| app.items.clone(),
            |items: &Vec<i32>| items.iter().sum(),
        );
        Field {
            value: s.select(cx, |app: &App| app.a),
            sum,
            renders: r,
        }
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| store.dispatch(IncA, cx));
    vcx.run_until_parked();
    count_repaint(vcx);
    let series: Vec<u64> = (0..6).map(|_| count_repaint(vcx).allocations).collect();
    eprintln!("state: {series:?}");
    let peak = series.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= 60,
        "state repaint peaks at {peak}, over its budget of 60: {series:?}"
    );
    let early = median(&series[..3]);
    let late = median(&series[3..]);
    assert!(
        late <= early,
        "state repaint grows (median): early {early}, late {late}"
    );
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn count_repaint(vcx: &mut VisualTestContext) -> guiraffe_test::alloc::Counts {
    let (_, counts) = count_allocations(|| {
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        vcx.run_until_parked();
    });
    counts
}
