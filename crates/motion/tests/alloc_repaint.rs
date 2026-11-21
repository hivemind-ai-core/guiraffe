//! Warm-repaint allocation gate for the motion drivers: a settled `Animated`
//! and a settled `Presence` must repaint without growing their allocation count.

use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use guiraffe_motion::{Animated, Easing, Frame, Presence, Timing, framed, init, sample_presence};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Leaf {
    glow: Animated<f32>,
    presence: Presence,
    visible: Rc<Cell<bool>>,
}

impl Render for Leaf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let glow = self.glow.sample(window, cx);
        let visibility = sample_presence(&mut self.presence, window, cx);
        self.visible.set(visibility.is_some());
        let content = framed(
            div()
                .w(px(120.))
                .h(px(40.))
                .opacity(glow.clamp(0.0, 1.0))
                .child("motion"),
            Frame::REST,
        );
        div()
            .child(content)
            .child(div().opacity(visibility.unwrap_or(0.0)).child("presence"))
    }
}

struct Page {
    leaf: Entity<Leaf>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.leaf.clone())
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
fn settled_motion_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    cx.update(init);
    let (_page, vcx) = cx.add_window_view(|_, cx| {
        let leaf = cx.new(|_| Leaf {
            glow: Animated::new(0.5, Timing::tween(Duration::ZERO, Easing::Linear)),
            presence: Presence::new(
                true,
                Timing::tween(Duration::ZERO, Easing::Linear),
                Timing::tween(Duration::ZERO, Easing::Linear),
            ),
            visible: Rc::new(Cell::new(false)),
        });
        Page { leaf }
    });
    vcx.run_until_parked();
    // Settle both drivers: the value is already the target, so nothing is in flight.
    vcx.executor().advance_clock(Duration::from_millis(50));
    count_repaint(vcx);
    count_repaint(vcx);
    let series: Vec<u64> = (0..6).map(|_| count_repaint(vcx).allocations).collect();
    eprintln!("motion: {series:?}");
    let peak = series.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= 80,
        "motion repaint peaks at {peak}, over its budget of 80: {series:?}"
    );
    let early = median(&series[..3]);
    let late = median(&series[3..]);
    assert!(
        late <= early,
        "motion repaint grows (median): early {early}, late {late}"
    );
}
