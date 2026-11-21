//! Warm-repaint allocation gate for `ui!`-generated views. The macro erases each
//! child into an `AnyElement` through a children `Vec`, so a macro view has a
//! per-node cost that must stay bounded and must not grow frame over frame.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};
use guiraffe_ui::ui;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Measured on the upstream backend: see plans/ALLOC-AUDIT.md.
const BUDGET: u64 = 120;

struct MacroView {
    renders: Rc<Cell<usize>>,
}

impl Render for MacroView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        ui! {
            div {
                size_full flex flex_col
                div { h: px(24.) "header" }
                div { flex flex_row
                    div { w: px(70.) "left" }
                    div { w: px(30.) "right" }
                }
                if true { div { "conditional" } }
            }
        }
    }
}

struct Host {
    view: Entity<MacroView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
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

#[gpui_kit::test]
fn warm_generated_view_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let renders = Rc::new(Cell::new(0));
    let counted = renders.clone();
    let (_host, vcx) = cx.add_window_view(|_, cx| {
        let view = cx.new(|_| MacroView { renders: counted });
        Host { view }
    });
    vcx.run_until_parked();
    let series: Vec<u64> = (0..6).map(|_| count_repaint(vcx).allocations).collect();
    eprintln!(
        "ui! generated view series: {series:?} after {} renders",
        renders.get()
    );
    assert!(
        renders.get() > 0,
        "the generated view never rendered; the probe measured only the host"
    );
    let last = series[series.len() - 1];
    assert!(
        last <= BUDGET,
        "a generated view repaint allocated {last}, over its budget of {BUDGET}: {series:?}"
    );
    let early = median(&series[..3]);
    let late = median(&series[3..]);
    assert!(
        late <= early,
        "generated view repaint grows (median): early {early}, late {late}: {series:?}"
    );
}
