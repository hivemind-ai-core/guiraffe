//! Allocation bound for steady layout animation (spec/29): an animating frame of an
//! `AnimatedSize` + `AnimatedList` view allocates no more than an idle re-render of the same
//! view (GPUI's own per-frame work is the baseline), plus [`EXTRA_PER_FRAME`].
#![allow(unsafe_code)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    time::Duration,
};

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};

use crate::*;

/// Documented bound: allocations an animating frame may make beyond an idle re-render.
/// All of it is GPUI's frame scheduling, none is ours: each animating element's
/// `request_animation_frame` boxes one callback (two elements here), and GPUI re-creates the
/// next-frame callback list it takes every frame. The element state, the FLIP map, the key
/// hash and the size tracks allocate nothing in steady state.
const EXTRA_PER_FRAME: usize = 3;
/// On gpui-fast an idle re-render reuses the retained layout and paint of unchanged subtrees;
/// an animating frame changes geometry by definition, so the backend re-lays out and repaints
/// the moving subtree (measured: +7 over idle). Our own state is still allocation-free, which
/// `pure_steady_state_allocates_nothing` asserts directly on both backends.
const EXTRA_PER_FRAME_RETAINED: usize = 8;

struct Counting;

thread_local! {
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}

// SAFETY: every operation forwards its arguments unchanged to System; the counter is a
// const-initialised TLS Cell that never allocates and tolerates TLS teardown.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNT.try_with(|c| {
            if let Some(n) = c.get() {
                c.set(Some(n + 1));
            }
        });
        // SAFETY: the caller supplies a valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: ptr was allocated by System with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        let _ = COUNT.try_with(|c| {
            if let Some(n) = c.get() {
                c.set(Some(n + 1));
            }
        });
        // SAFETY: forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn counted(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(Some(0)));
    f();
    COUNT.with(|c| c.take()).unwrap_or(0)
}

struct View {
    rows: usize,
    row_h: f32,
    order: Vec<u32>,
}

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = Timing::tween(Duration::from_secs(10), Easing::Linear);
        div()
            .w(px(200.))
            .child(
                AnimatedSize::new(
                    "size",
                    div().children((0..self.rows).map(|_| div().h(px(self.row_h)))),
                )
                .enabled(true)
                .timing(t),
            )
            .child(
                AnimatedList::new("list").enabled(true).timing(t).children(
                    self.order
                        .iter()
                        .map(|&k| (k as usize, div().h(px(10.)).w_full())),
                ),
            )
    }
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let n = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    n
}

#[gpui_kit::test]
fn steady_animation_allocates_no_more_than_an_idle_rerender(cx: &mut TestAppContext) {
    cx.update(init);
    let (view, vcx) = cx.add_window_view(|_, _| View {
        rows: 8,
        row_h: 10.,
        order: (0..16).collect(),
    });
    vcx.run_until_parked();
    // Warm up: measured mode, element-state maps at capacity.
    for _ in 0..3 {
        vcx.update(|_, cx| cx.notify(view.entity_id()));
        vcx.run_until_parked();
    }
    let idle = (0..5)
        .map(|_| {
            counted(|| {
                vcx.update(|_, cx| cx.notify(view.entity_id()));
                vcx.run_until_parked();
            })
        })
        .max()
        .unwrap_or(0);
    // Start a long size animation and a FLIP of every row, then sample steady frames.
    vcx.update(|_, cx| {
        view.update(cx, |v, cx| {
            // Same element count as the baseline: only sizes and order change.
            v.row_h = 30.;
            v.order.reverse();
            cx.notify();
        })
    });
    vcx.run_until_parked();
    for _ in 0..3 {
        assert!(frame(vcx) > 0, "animating");
    }
    let animating = (0..5)
        .map(|_| {
            counted(|| {
                frame(vcx);
            })
        })
        .max()
        .unwrap_or(0);
    let bound = if gpui_kit::GPUI_FAST {
        EXTRA_PER_FRAME_RETAINED
    } else {
        EXTRA_PER_FRAME
    };
    assert!(
        animating <= idle + bound,
        "animating frame allocated {animating}, idle re-render {idle} (bound +{bound})"
    );
}

#[test]
fn pure_steady_state_allocates_nothing() {
    use crate::layout::{FlipList, SizeTrack, keys_hash, offset};
    let prefs = MotionPrefs::default();
    let t = Timing::tween(Duration::from_secs(10), Easing::Linear);
    let t0 = std::time::Instant::now();
    let keys: Vec<gpui_kit::ElementId> = (0..64usize).map(Into::into).collect();
    let mut list: FlipList<gpui_kit::ElementId> = FlipList::new(t);
    let mut size = SizeTrack::new(10.0, t);
    // Warm up: first layout, then a structural reorder that starts every item moving.
    list.begin(keys_hash(&keys), t);
    for (i, k) in keys.iter().enumerate() {
        list.place(k, offset(0.0, i as f32 * 10.0), t0, &prefs);
    }
    list.end();
    let reversed: Vec<_> = keys.iter().rev().cloned().collect();
    list.begin(keys_hash(&reversed), t);
    for (i, k) in reversed.iter().enumerate() {
        list.place(k, offset(0.0, i as f32 * 10.0), t0, &prefs);
    }
    list.end();
    size.observe(500.0, t0, &prefs);
    let n = counted(|| {
        for frame in 1..=60u64 {
            let now = t0 + Duration::from_millis(frame * 16);
            list.begin(keys_hash(&reversed), t);
            for (i, k) in reversed.iter().enumerate() {
                std::hint::black_box(list.place(k, offset(0.0, i as f32 * 10.0), now, &prefs));
            }
            list.end();
            size.observe(500.0, now, &prefs);
            std::hint::black_box(size.value_at(now, &prefs));
        }
    });
    assert_eq!(n, 0, "steady FLIP + size tracking allocates nothing");
}
