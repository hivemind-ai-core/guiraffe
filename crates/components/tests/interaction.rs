use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::{
    Entity, Focusable, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use guiraffe_components::{SegmentedControl, SegmentedEvent};
use guiraffe_test::render::RenderCounter;

struct Host {
    control: Entity<SegmentedControl>,
    renders: RenderCounter,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        self.renders.hit();
        div().w(px(400.)).child(self.control.clone())
    }
}

fn setup<'a>(
    cx: &'a mut TestAppContext,
    items: &[&'static str],
) -> (
    Entity<SegmentedControl>,
    RenderCounter,
    &'a mut VisualTestContext,
    Rc<RefCell<Vec<SegmentedEvent>>>,
) {
    cx.update(guiraffe_components::init);
    let renders = RenderCounter::new();
    let r2 = renders.clone();
    let items = items.to_vec();
    let mut control = None;
    let (_host, vcx) = cx.add_window_view(|_, cx| {
        let c = SegmentedControl::new(items, cx);
        control = Some(c.clone());
        Host {
            control: c,
            renders: r2,
        }
    });
    let control = control.unwrap_or_else(|| unreachable!());
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&control, move |_, e: &SegmentedEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach()
    });
    vcx.run_until_parked();
    (control, renders, vcx, events)
}

fn frames(vcx: &mut VisualTestContext, n: usize) {
    for _ in 0..n {
        vcx.executor().advance_clock(Duration::from_millis(16));
        vcx.update(|w, cx| w.simulate_next_frame(cx));
        vcx.run_until_parked();
    }
}

#[gpui_kit::test]
fn select_emits_once_and_clamps(cx: &mut TestAppContext) {
    let (control, _, vcx, events) = setup(cx, &["React", "Vue", "Svelte"]);
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(2, cx)));
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(2, cx)));
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(99, cx)));
    vcx.run_until_parked();
    assert_eq!(*events.borrow(), vec![SegmentedEvent::Changed(2)]);
}

#[gpui_kit::test]
fn keyboard_navigation(cx: &mut TestAppContext) {
    let (control, _, vcx, events) = setup(cx, &["A", "B", "C"]);
    vcx.update(|window, cx| {
        let handle = control.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    vcx.simulate_keystrokes("right right right");
    assert_eq!(
        vcx.update(|_, cx| control.read(cx).selected()),
        2,
        "clamped at the end"
    );
    vcx.simulate_keystrokes("home");
    assert_eq!(vcx.update(|_, cx| control.read(cx).selected()), 0);
    vcx.simulate_keystrokes("end left");
    assert_eq!(vcx.update(|_, cx| control.read(cx).selected()), 1);
    assert_eq!(
        events.borrow().len(),
        5,
        "one event per actual change: 1,2,0,2,1"
    );
}

#[gpui_kit::test]
fn indicator_animation_does_not_rerender_the_host(cx: &mut TestAppContext) {
    let (control, renders, vcx, _) = setup(cx, &["One", "Two", "Three"]);
    frames(vcx, 3);
    let before = renders.get();
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(2, cx)));
    frames(vcx, 30);
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            renders.get(),
            before,
            "spring animation re-renders only the control"
        );
    } else {
        // Upstream GPUI re-renders ancestors on child notify (DESIGN §2c); recorded, not hidden.
        assert!(renders.get() >= before);
    }
}

#[gpui_kit::test]
fn items_can_change_and_empty_is_safe(cx: &mut TestAppContext) {
    let (control, _, vcx, events) = setup(cx, &["A", "B", "C"]);
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(2, cx)));
    vcx.update(|_, cx| control.update(cx, |c, cx| c.set_items(["X"], cx)));
    frames(vcx, 2);
    assert_eq!(
        vcx.update(|_, cx| control.read(cx).selected()),
        0,
        "selection clamped to new items"
    );
    vcx.update(|_, cx| control.update(cx, |c, cx| c.set_items(Vec::<&str>::new(), cx)));
    frames(vcx, 2);
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(1, cx)));
    assert_eq!(
        events.borrow().len(),
        1,
        "selecting in an empty control is a no-op"
    );
}

#[gpui_kit::test]
fn the_first_click_selects_and_focuses_a_segment(cx: &mut TestAppContext) {
    let (control, _, vcx, events) = setup(cx, &["A", "B", "C"]);
    frames(vcx, 4);
    let position = gpui_kit::point(px(333.), px(10.));
    vcx.simulate_mouse_down(position, gpui_kit::MouseButton::Left, Default::default());
    frames(vcx, 2);
    vcx.simulate_mouse_up(position, gpui_kit::MouseButton::Left, Default::default());
    frames(vcx, 2);
    assert_eq!(control.read_with(vcx, |c, _| c.selected()), 2);
    assert_eq!(*events.borrow(), vec![SegmentedEvent::Changed(2)]);
    assert!(vcx.update(|w, cx| control.read(cx).focus_handle(cx).is_focused(w)));
    vcx.simulate_keystrokes("left");
    assert_eq!(control.read_with(vcx, |c, _| c.selected()), 1);
}

#[gpui_kit::test]
fn the_first_click_moves_the_painted_indicator_without_another_input(cx: &mut TestAppContext) {
    let (_, _, vcx, _) = setup(cx, &["A", "B", "C"]);
    for _ in 0..4 {
        vcx.update(|w, cx| {
            w.draw(cx).clear(cx);
            w.simulate_next_frame(cx);
        });
        vcx.run_until_parked();
    }
    let before = vcx
        .debug_bounds("segment-indicator")
        .expect("initial indicator");
    vcx.simulate_click(gpui_kit::point(px(333.), px(10.)), Default::default());
    vcx.update(|w, cx| w.draw(cx).clear(cx));
    let scheduled = vcx.update(|w, cx| w.simulate_next_frame(cx));
    assert!(
        scheduled > 0,
        "retargeting a resting indicator must schedule its next frame"
    );
    for _ in 0..40 {
        vcx.executor().advance_clock(Duration::from_millis(16));
        vcx.update(|w, cx| {
            w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
        });
        vcx.run_until_parked();
    }
    let after = vcx
        .debug_bounds("segment-indicator")
        .expect("indicator after one click");
    assert!(
        after.left() > before.left() + px(200.),
        "indicator stayed on the old segment: {before:?} -> {after:?}"
    );
}
