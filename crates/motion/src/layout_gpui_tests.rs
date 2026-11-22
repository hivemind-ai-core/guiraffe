//! GPUI tests for layout animation, on both backends.

use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext,
    Window, canvas, div, px,
};

use crate::*;

const ROW: f32 = 20.0;

fn linear() -> Timing {
    Timing::tween(Duration::from_millis(200), Easing::Linear)
}

/// A column of `rows` fixed-height rows inside an `AnimatedSize`, then a probe that records
/// its own y (= the animated height).
struct Host {
    rows: usize,
    enabled: Option<bool>,
    probe: Rc<Cell<f32>>,
    renders: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let probe = self.probe.clone();
        div()
            .flex()
            .flex_col()
            .w(px(200.))
            .child(
                AnimatedSize::new(
                    "box",
                    div().children((0..self.rows).map(|_| div().h(px(ROW)).w_full())),
                )
                .enabled_opt(self.enabled)
                .timing(linear()),
            )
            .child(
                canvas(
                    move |b, _, _| probe.set(f32::from(b.origin.y)),
                    |_, _, _, _| {},
                )
                .w_full()
                .h(px(1.)),
            )
    }
}

struct Fx {
    host: Entity<Host>,
    probe: Rc<Cell<f32>>,
    renders: Rc<Cell<usize>>,
}

fn host(
    cx: &mut TestAppContext,
    rows: usize,
    enabled: Option<bool>,
) -> (Fx, &mut VisualTestContext) {
    let probe = Rc::new(Cell::new(-1.0));
    let renders = Rc::new(Cell::new(0));
    let (p, r) = (probe.clone(), renders.clone());
    let (host, vcx) = cx.add_window_view(move |_, _| Host {
        rows,
        enabled,
        probe: p,
        renders: r,
    });
    vcx.run_until_parked();
    (
        Fx {
            host,
            probe,
            renders,
        },
        vcx,
    )
}

/// Deliver the next frame after `ms`, then let the redraw happen. Returns frame callbacks run.
fn frame(vcx: &mut VisualTestContext, ms: u64) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(ms));
    let n = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    n
}

fn set_rows(f: &Fx, vcx: &mut VisualTestContext, rows: usize) {
    vcx.update(|_, cx| {
        f.host.update(cx, |h, cx| {
            h.rows = rows;
            cx.notify();
        })
    });
    vcx.run_until_parked();
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..40 {
        if frame(vcx, 20) == 0 {
            return;
        }
    }
}

fn enable_globally(cx: &mut TestAppContext, reduced: bool) {
    cx.update(|cx| {
        init(cx);
        MotionPrefs::set(
            MotionPrefs {
                reduced,
                layout: LayoutMotion {
                    enabled: true,
                    timing: linear(),
                },
                ..Default::default()
            },
            cx,
        );
    });
}

#[gpui_kit::test]
fn size_starts_at_old_and_ends_at_new_then_idles(cx: &mut TestAppContext) {
    cx.update(init);
    let (f, vcx) = host(cx, 2, Some(true));
    assert_eq!(
        f.probe.get(),
        2.0 * ROW,
        "first frame lays out at the natural size"
    );
    assert_eq!(frame(vcx, 16), 0, "no frames requested at rest");
    // Render once more at rest (measured mode): still no frame request.
    vcx.update(|_, cx| cx.notify(f.host.entity_id()));
    vcx.run_until_parked();
    assert_eq!(frame(vcx, 16), 0);

    set_rows(&f, vcx, 5);
    assert_eq!(
        f.probe.get(),
        2.0 * ROW,
        "the change frame shows the old size"
    );
    let mut last = f.probe.get();
    for _ in 0..4 {
        assert!(frame(vcx, 20) > 0, "frames requested while moving");
        let y = f.probe.get();
        assert!(
            y > last && y < 5.0 * ROW,
            "monotonic in between: {last} -> {y}"
        );
        last = y;
    }
    settle(vcx);
    assert_eq!(f.probe.get(), 5.0 * ROW);
    let renders = f.renders.get();
    assert_eq!(frame(vcx, 500), 0, "idle after settling");
    assert_eq!(f.renders.get(), renders, "zero cost at rest");
}

#[gpui_kit::test]
fn retargets_mid_flight_from_the_current_size(cx: &mut TestAppContext) {
    cx.update(init);
    let (f, vcx) = host(cx, 0, Some(true));
    set_rows(&f, vcx, 10); // 0 -> 200 over 200 ms
    for _ in 0..5 {
        frame(vcx, 20);
    }
    let mid = f.probe.get();
    assert!((mid - 100.0).abs() < 1.0, "halfway: {mid}");
    set_rows(&f, vcx, 0);
    assert!((f.probe.get() - mid).abs() < 1.0, "no jump at the retarget");
    frame(vcx, 20);
    let after = f.probe.get();
    assert!(
        after < mid && after > 0.0,
        "now heading down from {mid}: {after}"
    );
    settle(vcx);
    assert_eq!(f.probe.get(), 0.0);
}

#[gpui_kit::test]
fn disabled_container_is_a_plain_pass_through(cx: &mut TestAppContext) {
    enable_globally(cx, false);
    let (f, vcx) = host(cx, 2, Some(false));
    set_rows(&f, vcx, 5);
    assert_eq!(f.probe.get(), 5.0 * ROW, "applied in the same frame");
    assert_eq!(frame(vcx, 16), 0, "no frames requested");
}

#[gpui_kit::test]
fn reduced_motion_is_instant(cx: &mut TestAppContext) {
    enable_globally(cx, true);
    let (f, vcx) = host(cx, 2, Some(true));
    set_rows(&f, vcx, 5);
    assert_eq!(f.probe.get(), 5.0 * ROW, "applied in the same frame");
    assert_eq!(frame(vcx, 16), 0, "no frames requested");
}

#[gpui_kit::test]
fn global_setting_is_opt_in(cx: &mut TestAppContext) {
    cx.update(init);
    let (f, vcx) = host(cx, 2, None);
    set_rows(&f, vcx, 5);
    assert_eq!(f.probe.get(), 5.0 * ROW, "off by default");
}

#[gpui_kit::test]
fn global_opt_in_animates(cx: &mut TestAppContext) {
    enable_globally(cx, false);
    let (f, vcx) = host(cx, 2, None);
    set_rows(&f, vcx, 5);
    assert_eq!(f.probe.get(), 2.0 * ROW, "global opt-in animates");
    settle(vcx);
    assert_eq!(f.probe.get(), 5.0 * ROW);
}

/// A keyed list: each row records its painted y.
struct ListHost {
    order: Vec<usize>,
    ys: Rc<[Cell<f32>; 3]>,
}

impl Render for ListHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.order.iter().map(|&k| {
            let ys = self.ys.clone();
            (
                k,
                div().h(px(ROW)).w_full().child(
                    canvas(
                        move |b, _, _| {
                            if let Some(c) = ys.get(k) {
                                c.set(f32::from(b.origin.y));
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                ),
            )
        });
        div().w(px(200.)).child(
            AnimatedList::new("list")
                .enabled(true)
                .timing(linear())
                .children(rows),
        )
    }
}

#[gpui_kit::test]
fn flip_reorder_starts_at_old_positions_and_ends_at_new(cx: &mut TestAppContext) {
    cx.update(init);
    let ys: Rc<[Cell<f32>; 3]> = Rc::new([Cell::new(-1.), Cell::new(-1.), Cell::new(-1.)]);
    let y2 = ys.clone();
    let (list, vcx) = cx.add_window_view(move |_, _| ListHost {
        order: vec![0, 1, 2],
        ys: y2,
    });
    vcx.run_until_parked();
    assert_eq!((ys[0].get(), ys[2].get()), (0.0, 2.0 * ROW));
    assert_eq!(frame(vcx, 16), 0, "idle list requests nothing");
    vcx.update(|_, cx| {
        list.update(cx, |l, cx| {
            l.order = vec![2, 1, 0];
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert_eq!(
        (ys[0].get(), ys[2].get()),
        (0.0, 2.0 * ROW),
        "painted where they were"
    );
    assert!(frame(vcx, 100) > 0);
    assert!((ys[0].get() - ROW).abs() < 1.0, "halfway: {}", ys[0].get());
    assert!((ys[2].get() - ROW).abs() < 1.0, "halfway: {}", ys[2].get());
    settle(vcx);
    assert_eq!(
        (ys[0].get(), ys[1].get(), ys[2].get()),
        (2.0 * ROW, ROW, 0.0)
    );
    assert_eq!(frame(vcx, 100), 0, "idle again");
}

#[gpui_kit::test]
fn flip_removal_slides_neighbours_up(cx: &mut TestAppContext) {
    cx.update(init);
    let ys: Rc<[Cell<f32>; 3]> = Rc::new([Cell::new(-1.), Cell::new(-1.), Cell::new(-1.)]);
    let y2 = ys.clone();
    let (list, vcx) = cx.add_window_view(move |_, _| ListHost {
        order: vec![0, 1, 2],
        ys: y2,
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        list.update(cx, |l, cx| {
            l.order = vec![0, 2];
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert_eq!(ys[2].get(), 2.0 * ROW, "starts at its old place");
    settle(vcx);
    assert_eq!(ys[2].get(), ROW);
}
