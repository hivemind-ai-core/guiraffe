//! Warm-repaint allocation gate for the devtools overlay. The panel rebuilds its
//! live telemetry text each sample, so it has a documented per-repaint budget
//! rather than a zero-allocation target; the gate is that it stays inside that
//! budget and does not grow frame over frame.

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use guiraffe_devtools::{Devtools, init, root};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Measured on both backends with the panel open, paused and one tracked view
/// (upstream ~758, gpui-fast ~490): see plans/ALLOC-AUDIT.md. The panel samples
/// its own frames, so a single frame can be an outlier; the growth check uses
/// medians. Raising the budget needs a recorded reason.
const BUDGET: u64 = 900;

struct AppView {
    tools: Devtools,
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.tools
            .track("example-editor", div().w(px(400.)).child("Inspected app"))
    }
}

struct Host {
    root: Entity<guiraffe_devtools::DevtoolsRoot>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.root.clone())
    }
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

fn mount(cx: &mut TestAppContext, open: bool) -> &mut VisualTestContext {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        init(cx);
    });
    let (_host, vcx) = cx.add_window_view(|_, cx| {
        let tools = Devtools::new(cx);
        let app = cx.new(|_| AppView {
            tools: tools.clone(),
        });
        let root = root(app, &tools, cx);
        // Pause so the 250 ms telemetry ticker cannot dirty the frame under test.
        tools.set_open(open, cx);
        tools.set_paused(true, cx);
        Host { root }
    });
    vcx.run_until_parked();
    vcx
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

#[gpui_kit::test]
fn warm_devtools_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let closed = mount(cx, false);
    count_repaint(closed);
    let closed_baseline = count_repaint(closed).allocations;

    let open = mount(cx, true);
    count_repaint(open);
    let series: Vec<u64> = (0..8).map(|_| count_repaint(open).allocations).collect();
    eprintln!("devtools: closed {closed_baseline}; open series {series:?}");

    let peak = series.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= BUDGET,
        "open devtools panel repaint peaks at {peak}, over its budget of {BUDGET}: {series:?}"
    );
    // Growth is a regression even under budget: the panel must not accumulate work.
    let early = median(&series[..4]);
    let late = median(&series[4..]);
    assert!(
        late <= early,
        "open devtools panel repaint grows (median): early {early}, late {late}: {series:?}"
    );
    assert!(
        closed_baseline < late,
        "the panel should cost more than the bare host: {closed_baseline} vs {late}"
    );
}
