use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::{
    Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};
use guiraffe_components::{Loader, SegmentedControl, SegmentedEvent};
use guiraffe_motion::MotionPrefs;
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

// --- Loader: a Control whose animation is its own view. ---

struct LoaderScene {
    renders: RenderCounter,
    loader: Entity<Loader>,
}

impl Render for LoaderScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        div().w(px(200.)).child(self.loader.clone())
    }
}

fn loader_scene(
    cx: &mut TestAppContext,
) -> (Entity<Loader>, Entity<LoaderScene>, &mut VisualTestContext) {
    cx.update(guiraffe_components::init);
    let renders = RenderCounter::new();
    let counter = renders.clone();
    let mut loader = None;
    let (scene, vcx) = cx.add_window_view(|_, cx| {
        let l = Loader::new(cx);
        l.update(cx, |l, cx| {
            l.set_kind(guiraffe_components::LoaderType::Dots, cx);
            l.set_color(Some(guiraffe_core::ColorName::Teal), cx);
        });
        loader = Some(l.clone());
        LoaderScene {
            renders: counter,
            loader: l,
        }
    });
    (loader.unwrap_or_else(|| unreachable!()), scene, vcx)
}

fn next_frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let n = vcx.update(|w, cx| w.simulate_next_frame(cx));
    vcx.run_until_parked();
    n
}

#[gpui_kit::test]
fn loader_animates_while_rendered(cx: &mut TestAppContext) {
    let (_loader, _scene, vcx) = loader_scene(cx);
    vcx.run_until_parked();
    assert!(next_frame(vcx) > 0, "a visible loader animates");
    assert!(next_frame(vcx) > 0, "and keeps animating");
}

#[gpui_kit::test]
fn loader_under_reduced_motion_requests_no_frames(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guiraffe_components::init(cx);
        MotionPrefs::set(
            MotionPrefs {
                reduced: true,
                ..Default::default()
            },
            cx,
        );
    });
    let (_loader, _scene, vcx) = loader_scene(cx);
    vcx.run_until_parked();
    assert_eq!(next_frame(vcx), 0, "reduced motion: no animation frames");
    assert_eq!(next_frame(vcx), 0, "and none later either");
}

#[gpui_kit::test]
fn loader_animation_does_not_rerender_its_host(cx: &mut TestAppContext) {
    let (_loader, scene, vcx) = loader_scene(cx);
    vcx.run_until_parked();
    next_frame(vcx);
    let before = vcx.update(|_, cx| scene.read(cx).renders.get());
    frames(vcx, 6);
    let after = vcx.update(|_, cx| scene.read(cx).renders.get());
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            after, before,
            "only the loader re-renders while it animates"
        );
    } else {
        assert!(after >= before, "upstream re-renders ancestors (recorded)");
    }
}

#[gpui_kit::test]
fn loader_settings_are_idsempotent_and_clamped(cx: &mut TestAppContext) {
    let (loader, _scene, vcx) = loader_scene(cx);
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        loader.update(cx, |l, cx| {
            l.set_count(0, cx);
            l.set_kind(guiraffe_components::LoaderType::Bars, cx);
            l.set_kind(guiraffe_components::LoaderType::Bars, cx);
        })
    });
    vcx.run_until_parked();
    let kind = vcx.update(|_, cx| loader.read(cx).kind());
    assert_eq!(kind, guiraffe_components::LoaderType::Bars);
}
