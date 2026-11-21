//! Warm-repaint allocation gate: a mounted `Router` view that has not navigated
//! must not allocate more on each repaint.

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};
use guiraffe_router::Router;
use guiraffe_state::Selected;
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

guiraffe_router::routes! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum AppRoute {
        Home = "/",
        Project { id: u64 } = "/projects/:id",
    }
}

struct Nav {
    is_project: Selected<bool>,
}

impl Render for Nav {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).child(if *self.is_project.get(cx) {
            "project"
        } else {
            "home"
        })
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
fn warm_router_view_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let router = cx.update(|cx| Router::new(AppRoute::Home, cx));
    let r = router.clone();
    let (_view, vcx) = cx.add_window_view(move |_, cx| Nav {
        is_project: r.select(cx, |route| matches!(route, AppRoute::Project { .. })),
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        router.navigate(AppRoute::Project { id: 1 }, cx);
    });
    vcx.run_until_parked();
    count_repaint(vcx);
    let series: Vec<u64> = (0..6).map(|_| count_repaint(vcx).allocations).collect();
    eprintln!("router: {series:?}");
    let peak = series.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= 60,
        "router repaint peaks at {peak}, over its budget of 60: {series:?}"
    );
    let early = median(&series[..3]);
    let late = median(&series[3..]);
    assert!(
        late <= early,
        "router repaint grows (median): early {early}, late {late}"
    );
}
