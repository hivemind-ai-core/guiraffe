use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, div,
};

use crate::*;

struct Leaf {
    value: Animated<f32>,
    seen: Rc<Cell<f32>>,
    renders: Rc<Cell<usize>>,
}

impl Render for Leaf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let v = self.value.sample(window, cx);
        self.seen.set(v);
        div().opacity(v.clamp(0.0, 1.0))
    }
}

struct Sibling {
    renders: Rc<Cell<usize>>,
}

impl Render for Sibling {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
    }
}

struct Page {
    leaf: Entity<Leaf>,
    sibling: Entity<Sibling>,
    renders: Rc<Cell<usize>>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().child(self.leaf.clone()).child(self.sibling.clone())
    }
}

struct Fixture {
    leaf: Entity<Leaf>,
    seen: Rc<Cell<f32>>,
    leaf_renders: Rc<Cell<usize>>,
    page_renders: Rc<Cell<usize>>,
    sibling_renders: Rc<Cell<usize>>,
}

fn fixture(cx: &mut TestAppContext) -> (Fixture, &mut gpui_kit::VisualTestContext) {
    let seen = Rc::new(Cell::new(-1.0));
    let (lr, pr, sr) = (
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    let (s2, lr2, pr2, sr2) = (seen.clone(), lr.clone(), pr.clone(), sr.clone());
    let mut leaf_out = None;
    let (_page, vcx) = cx.add_window_view(|_, cx| {
        let leaf = cx.new(|_| Leaf {
            value: Animated::new(
                0.0,
                Timing::tween(Duration::from_millis(200), Easing::Linear),
            ),
            seen: s2,
            renders: lr2,
        });
        leaf_out = Some(leaf.clone());
        let sibling = cx.new(|_| Sibling { renders: sr2 });
        Page {
            leaf,
            sibling,
            renders: pr2,
        }
    });
    let leaf = leaf_out.unwrap_or_else(|| unreachable!());
    (
        Fixture {
            leaf,
            seen,
            leaf_renders: lr,
            page_renders: pr,
            sibling_renders: sr,
        },
        vcx,
    )
}

/// Tests have no platform frame loop: deliver the next frame, then let the redraw happen.
fn frame(vcx: &mut gpui_kit::VisualTestContext, ms: u64) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(ms));
    let n = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    n
}

#[gpui_kit::test]
fn animation_reaches_target_and_only_the_leaf_rerenders(cx: &mut TestAppContext) {
    cx.update(init);
    let (f, vcx) = fixture(cx);
    vcx.run_until_parked();
    let (page0, sib0) = (f.page_renders.get(), f.sibling_renders.get());
    vcx.update(|_, cx| {
        f.leaf.update(cx, |leaf, cx| {
            if leaf.value.set(1.0, cx) {
                cx.notify();
            }
        })
    });
    vcx.run_until_parked();
    let leaf_after_start = f.leaf_renders.get();
    let mut mid = None;
    for i in 0..12 {
        frame(vcx, 20);
        if i == 4 {
            mid = Some(f.seen.get());
        }
    }
    let mid = mid.unwrap_or_default();
    assert!(mid > 0.3 && mid < 0.7, "linear midpoint ~0.5, got {mid}");
    assert_eq!(f.seen.get(), 1.0);
    assert!(
        f.leaf_renders.get() > leaf_after_start + 3,
        "animated over several frames"
    );
    if gpui_kit::GPUI_FAST {
        // Retained rendering: only the animating leaf is rebuilt.
        assert_eq!(
            f.page_renders.get(),
            page0,
            "parent not re-rendered by the animation"
        );
        assert_eq!(
            f.sibling_renders.get(),
            sib0,
            "sibling not re-rendered by the animation"
        );
    } else {
        // Upstream GPUI re-renders ancestors (and their uncached children) when a child is
        // notified. Recorded here so a behaviour change upstream is noticed.
        assert!(
            f.page_renders.get() > page0,
            "upstream re-renders ancestors on child notify"
        );
    }
    // At rest: no more frames are requested.
    let settled = f.leaf_renders.get();
    assert_eq!(frame(vcx, 500), 0, "no frame requested at rest");
    assert_eq!(f.leaf_renders.get(), settled, "zero cost at rest");
}

#[gpui_kit::test]
fn setting_same_target_is_noop(cx: &mut TestAppContext) {
    cx.update(init);
    let (f, vcx) = fixture(cx);
    let changed = vcx.update(|_, cx| f.leaf.update(cx, |leaf, cx| leaf.value.set(0.0, cx)));
    assert!(!changed);
}

#[gpui_kit::test]
fn reduced_motion_jumps(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init(cx);
        MotionPrefs::set(
            MotionPrefs {
                reduced: true,
                ..Default::default()
            },
            cx,
        );
    });
    let (f, vcx) = fixture(cx);
    vcx.update(|_, cx| {
        f.leaf.update(cx, |leaf, cx| {
            leaf.value.set(1.0, cx);
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert_eq!(f.seen.get(), 1.0);
}

#[gpui_kit::test]
fn init_keeps_existing_prefs(cx: &mut TestAppContext) {
    cx.update(|cx| {
        MotionPrefs::set(
            MotionPrefs {
                time_scale: 3.0,
                ..Default::default()
            },
            cx,
        );
        init(cx);
        assert_eq!(MotionPrefs::global(cx).time_scale, 3.0);
    });
}

#[gpui_kit::test]
fn platform_reduce_motion_is_respected(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init(cx);
        cx.set_reduce_motion(true);
        assert!(MotionPrefs::global(cx).reduced);
    });
}

#[test]
fn retargeting_a_spring_keeps_velocity_continuous() {
    let p = MotionPrefs::default();
    let t0 = std::time::Instant::now();
    let mut a = Animated::new(0.0f32, Timing::Spring(Spring::BOUNCY));
    a.set_at(100.0, t0, &p);
    let mid = t0 + Duration::from_millis(60);
    let h = Duration::from_millis(1);
    let before = (a.value_at(mid, &p).0 - a.value_at(mid - h, &p).0) / 0.001;
    a.set_at(200.0, mid, &p);
    let after = (a.value_at(mid + h, &p).0 - a.value_at(mid, &p).0) / 0.001;
    assert!(before > 50.0, "moving fast before retarget: {before}");
    assert!(
        (after - before).abs() / before < 0.1,
        "no kink: {before} -> {after}"
    );
}
