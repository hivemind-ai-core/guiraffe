//! Knob modulation arcs (#163): arc hit-testing, Alt/Ctrl depth drags and right-click context
//! events that never start the value drag, hover tooltips, and a live marker fed without a
//! knob or parent render.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    Context, Entity, Hsla, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point,
    Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use guiraffe_components::controls::*;
use guiraffe_components::mapping::ValueMap;

const SWEEP: f32 = 0.75;
const START: f32 = 0.625;

/// A point on ring radius `r` at travel `t`, relative to the dial centre.
fn at(r: f32, t: f32) -> (f32, f32) {
    let a = (START + SWEEP * t) * std::f32::consts::TAU;
    (r * a.sin(), -r * a.cos())
}

fn arcs() -> Vec<ModArc> {
    let red = Hsla::red();
    let blue = Hsla::blue();
    vec![
        ModArc::new(0.2, 0.6, red)
            .id("env-2")
            .tooltip("Envelope 2 → depth 0.40"),
        ModArc::new(0.9, 0.5, blue).id("lfo-1"),
    ]
}

#[test]
fn travel_maps_angles_and_skips_the_gap() {
    let (dx, dy) = at(10.0, 0.0);
    assert!(travel_at(dx, dy).is_some_and(|t| t.abs() < 1e-4));
    let (dx, dy) = at(10.0, 0.5);
    assert!(dx.abs() < 1e-4 && dy < 0.0, "half travel points up");
    assert!(travel_at(dx, dy).is_some_and(|t| (t - 0.5).abs() < 1e-4));
    assert_eq!(travel_at(0.0, 10.0), None, "straight down is the gap");
}

#[test]
fn arc_hits_pick_the_ring_and_respect_spans() {
    let g = DialGeometry::new(40.0);
    let arcs = arcs();
    assert!(
        g.ring_radius(0) > g.value_radius() + g.width() / 2.0,
        "rings sit outside"
    );
    assert!(g.ring_radius(1) > g.ring_radius(0));
    let hit = |r: f32, t: f32| {
        let (dx, dy) = at(r, t);
        mod_arc_at(g, &arcs, dx, dy)
    };
    assert_eq!(hit(g.ring_radius(0), 0.4), Some(0));
    assert_eq!(hit(g.ring_radius(1), 0.7), Some(1), "spans are ordered");
    assert_eq!(hit(g.ring_radius(0), 0.8), None, "outside arc 0's span");
    assert_eq!(hit(g.ring_radius(1), 0.3), None, "outside arc 1's span");
    assert_eq!(
        hit(g.value_radius() - 3.0, 0.4),
        None,
        "the value arc is not an arc hit"
    );
    assert_eq!(hit(g.ring_radius(1) + 6.0, 0.7), None, "too far out");
    // Overlapping spans: the nearer ring wins.
    assert_eq!(hit(g.ring_radius(0) + 0.3, 0.55), Some(0));
    assert_eq!(hit(g.ring_radius(1) - 0.3, 0.55), Some(1));
    // A zero-depth arc is still reachable.
    let zero = [ModArc::new(0.5, 0.5, Hsla::red())];
    let (dx, dy) = at(g.ring_radius(0), 0.51);
    assert_eq!(mod_arc_at(g, &zero, dx, dy), Some(0));
}

#[test]
fn depth_drags_scale_and_go_fine() {
    assert!((mod_depth_delta(200.0, false) - 1.0).abs() < 1e-9);
    assert!((mod_depth_delta(-20.0, true) + 0.01).abs() < 1e-9);
}

struct Host(Entity<KnobControl>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // The dial's centre sits at (40, 40).
        div().p(px(20.)).w(px(80.)).child(self.0.clone())
    }
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    Entity<KnobControl>,
    Rc<RefCell<Vec<ParamEvent>>>,
    &mut VisualTestContext,
) {
    cx.update(guiraffe_components::init);
    let mut built = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let knob = KnobControl::new()
            .mapping(ValueMap::new(0.0, 1.0))
            .value(0.5)
            .compact(true)
            .modulation_arcs(arcs())
            .build(window, cx);
        built = Some(knob.clone());
        Host(knob)
    });
    let knob = built.expect("built");
    let seen = Rc::new(RefCell::new(Vec::new()));
    vcx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&knob, move |_, e: &ParamEvent, _| {
            seen.borrow_mut().push(*e)
        })
        .detach();
    });
    vcx.run_until_parked();
    (knob, seen, vcx)
}

fn on_ring(i: usize, t: f32) -> Point<Pixels> {
    let g = DialGeometry::new(40.0);
    let (dx, dy) = at(g.ring_radius(i), t);
    point(px(40. + dx), px(40. + dy))
}

#[gpui_kit::test]
fn alt_drag_on_an_arc_emits_depth_not_a_value_drag(cx: &mut TestAppContext) {
    let (knob, seen, vcx) = mount(cx);
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let p = on_ring(1, 0.7);
    vcx.simulate_mouse_down(p, MouseButton::Left, alt);
    vcx.run_until_parked();
    assert!(
        !vcx.update(|_, cx| knob.read(cx).dragging()),
        "no value drag"
    );
    vcx.simulate_mouse_move(p - point(px(0.), px(20.)), MouseButton::Left, alt);
    vcx.run_until_parked();
    vcx.simulate_mouse_up(p - point(px(0.), px(20.)), MouseButton::Left, alt);
    vcx.run_until_parked();
    let events = seen.borrow().clone();
    assert!(
        matches!(events.as_slice(), [
            ParamEvent::ModDepth { arc: 1, delta_normal, .. },
            ParamEvent::ModDepthEnd { arc: 1, .. },
        ] if (delta_normal - 0.1).abs() < 1e-6),
        "{events:?}"
    );
    assert_eq!(
        vcx.update(|_, cx| knob.read(cx).value()),
        0.5,
        "the value is untouched"
    );
    // A Ctrl drag works the same; a plain press on the arc drags the value.
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    seen.borrow_mut().clear();
    let p = on_ring(0, 0.3);
    vcx.simulate_mouse_down(p, MouseButton::Left, ctrl);
    vcx.simulate_mouse_move(p + point(px(0.), px(10.)), MouseButton::Left, ctrl);
    vcx.simulate_mouse_up(p + point(px(0.), px(10.)), MouseButton::Left, ctrl);
    vcx.run_until_parked();
    assert!(matches!(
        seen.borrow().first(),
        Some(ParamEvent::ModDepth { arc: 0, delta_normal, .. }) if *delta_normal < 0.0
    ));
    vcx.simulate_mouse_down(p, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(
        vcx.update(|_, cx| knob.read(cx).dragging()),
        "a plain press drags the value"
    );
    vcx.simulate_mouse_up(p, MouseButton::Left, Modifiers::default());
}

#[gpui_kit::test]
fn right_click_on_an_arc_emits_context(cx: &mut TestAppContext) {
    let (knob, seen, vcx) = mount(cx);
    vcx.simulate_mouse_down(on_ring(0, 0.4), MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    assert!(matches!(
        seen.borrow().as_slice(),
        [ParamEvent::ModContext { arc: 0, .. }]
    ));
    // Off the arcs: nothing.
    vcx.simulate_mouse_down(on_ring(0, 0.9), MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(seen.borrow().len(), 1);
    // Removing the arcs removes the hit targets.
    vcx.update(|_, cx| knob.update(cx, |k, cx| k.set_modulation_arcs(Vec::new(), cx)));
    vcx.run_until_parked();
    vcx.simulate_mouse_down(on_ring(0, 0.4), MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(seen.borrow().len(), 1);
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn hovering_an_arc_shows_its_tooltip(cx: &mut TestAppContext) {
    let (knob, _, vcx) = mount(cx);
    vcx.simulate_mouse_move(on_ring(0, 0.4), None, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| knob.read(cx).hovered_arc()), Some(0));
    assert!(vcx.debug_bounds("knob-arc-tip").is_some());
    vcx.simulate_mouse_move(on_ring(1, 0.7), None, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| knob.read(cx).hovered_arc()), Some(1));
    assert!(
        vcx.debug_bounds("knob-arc-tip").is_none(),
        "arc 1 has no tooltip"
    );
}

#[gpui_kit::test]
fn the_live_marker_updates_without_a_knob_render(cx: &mut TestAppContext) {
    let (knob, _, vcx) = mount(cx);
    let feed = vcx.update(|_, cx| knob.update(cx, |k, cx| k.live_feed(cx)));
    vcx.run_until_parked();
    // Count knob renders through an observer of its notifications.
    let notified = Rc::new(std::cell::Cell::new(0));
    vcx.update(|_, cx| {
        let n = notified.clone();
        cx.observe(&knob, move |_, _| n.set(n.get() + 1)).detach();
    });
    for v in [0.1, 0.4, 0.9] {
        feed.set(Some(v));
        vcx.update(|_, cx| feed.publish(cx));
        vcx.run_until_parked();
        assert!(feed.travel().is_some_and(|t| (t - v).abs() < 1e-9));
    }
    assert_eq!(notified.get(), 0, "the knob itself is never notified");
    vcx.update(|_, cx| knob.update(cx, |k, cx| k.set_live(None, cx)));
    assert_eq!(feed.travel(), None);
}
