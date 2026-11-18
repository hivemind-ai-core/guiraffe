use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::{
    Context, Entity, IntoElement, KeyUpEvent, Keystroke, Modifiers, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, assets::IconName, div, point, px,
};
use guiraffe_components::*;
use guiraffe_test::render::RenderCounter;

struct Host<T: Render> {
    control: Entity<T>,
    renders: RenderCounter,
}

impl<T: Render> Render for Host<T> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        div().w(px(340.)).p(px(20.)).child(self.control.clone())
    }
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn frames(cx: &mut VisualTestContext, n: usize) {
    for _ in 0..n {
        cx.executor().advance_clock(Duration::from_millis(16));
        redraw(cx);
    }
}

fn tap_enter(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("enter");
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("enter").expect("valid key"),
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn expanding_list_opens_and_returns_siblings_to_their_original_bounds(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (host, window) = cx.add_window_view(|_, cx| Host {
        renders: RenderCounter::new(),
        control: ExpandingList::new(
            ["First", "Second"]
                .into_iter()
                .map(|title| DisclosureItem::new(title, || "Details".into_any_element()))
                .collect(),
            px(64.),
            cx,
        ),
    });
    let (control, counter) = window.update(|_, cx| {
        let host = host.read(cx);
        (host.control.clone(), host.renders.clone())
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    window.update(|_, cx| {
        cx.subscribe(&control, move |_, event: &ExpandingListChange, _| {
            sink.borrow_mut().push(*event);
        })
        .detach()
    });
    frames(window, 2);
    let first = window
        .debug_bounds("expanding-trigger-0")
        .expect("first trigger");
    let second = window
        .debug_bounds("expanding-trigger-1")
        .expect("second trigger");
    window.simulate_click(first.center(), Modifiers::none());
    // Process pointer feedback and the open event before advancing the spring clock.
    redraw(window);
    assert_eq!(window.debug_bounds("expanding-trigger-1"), Some(second));
    let renders = counter.get();
    frames(window, 2);
    let in_flight = window
        .debug_bounds("expanding-trigger-1")
        .expect("moving sibling");
    assert!(in_flight.origin.y > second.origin.y);
    frames(window, 45);
    let expanded = window
        .debug_bounds("expanding-trigger-1")
        .expect("settled sibling");
    assert!(expanded.origin.y > in_flight.origin.y);
    if gpui_kit::GPUI_FAST {
        assert_eq!(counter.get(), renders);
    }
    let first = window
        .debug_bounds("expanding-trigger-0")
        .expect("open trigger");
    window.simulate_click(first.center(), Modifiers::none());
    frames(window, 48);
    assert_eq!(window.debug_bounds("expanding-trigger-1"), Some(second));
    assert_eq!(
        *events.borrow(),
        [ExpandingListChange(Some(0)), ExpandingListChange(None)]
    );
}

#[gpui_kit::test]
fn dial_keeps_anchor_stable_and_closes_on_action_or_outside_press(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (host, window) = cx.add_window_view(|_, cx| Host {
        renders: RenderCounter::new(),
        control: SpeedDial::new(
            IconName::Plus,
            "Actions",
            vec![DialAction {
                icon: IconName::Search,
                label: "Search".into(),
            }],
            DialLayout::Horizontal { spacing: 64. },
            cx,
        ),
    });
    let (control, counter) = window.update(|_, cx| {
        let host = host.read(cx);
        (host.control.clone(), host.renders.clone())
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    window.update(|_, cx| {
        cx.subscribe(&control, move |_, event: &SpeedDialEvent, _| {
            sink.borrow_mut().push(*event);
        })
        .detach()
    });
    frames(window, 2);
    let primary = window.debug_bounds("speed-dial-primary").expect("primary");
    window.simulate_click(primary.center(), Modifiers::none());
    redraw(window);
    assert_eq!(window.debug_bounds("speed-dial-primary"), Some(primary));
    let renders = counter.get();
    frames(window, 45);
    assert_eq!(window.debug_bounds("speed-dial-primary"), Some(primary));
    if gpui_kit::GPUI_FAST {
        assert_eq!(counter.get(), renders);
    }
    let action = window.debug_bounds("speed-dial-action-0").expect("action");
    window.simulate_click(action.center(), Modifiers::none());
    frames(window, 45);
    window.simulate_click(primary.center(), Modifiers::none());
    frames(window, 45);
    window.simulate_click(point(px(300.), px(40.)), Modifiers::none());
    assert_eq!(
        *events.borrow(),
        [
            SpeedDialEvent::Open(true),
            SpeedDialEvent::Action(0),
            SpeedDialEvent::Open(false),
            SpeedDialEvent::Open(true),
            SpeedDialEvent::Open(false),
        ]
    );
}

#[gpui_kit::test]
fn accordion_controlled_requests_leave_the_authoritative_value_in_place(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (host, window) = cx.add_window_view(|_, cx| Host {
        renders: RenderCounter::new(),
        control: Accordion::new(
            ["One", "Two"]
                .into_iter()
                .map(|title| DisclosureItem::new(title, || "Details".into_any_element()))
                .collect(),
            AccordionOptions {
                focusable: Some(true),
                ..AccordionOptions::default()
            },
            vec![0],
            cx,
        ),
    });
    let control = window.update(|_, cx| host.read(cx).control.clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    window.update(|_, cx| {
        control.update(cx, |control, cx| control.set_controlled(Some(vec![0]), cx));
        cx.subscribe(&control, move |_, event: &AccordionChange, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
    });
    frames(window, 2);
    window.update(|window, cx| window.focus_next(cx));
    tap_enter(window);
    assert_eq!(*events.borrow(), [AccordionChange(vec![])]);
    assert_eq!(window.update(|_, cx| control.read(cx).open().to_vec()), [0]);
}

#[gpui_kit::test]
fn dial_keyboard_navigation_activates_actions_and_escape_dismisses(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (host, window) = cx.add_window_view(|_, cx| Host {
        renders: RenderCounter::new(),
        control: SpeedDial::new(
            IconName::Plus,
            "Actions",
            vec![DialAction {
                icon: IconName::Search,
                label: "Search".into(),
            }],
            DialLayout::Horizontal { spacing: 64. },
            cx,
        ),
    });
    let control = window.update(|_, cx| host.read(cx).control.clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    window.update(|_, cx| {
        control.update(cx, |control, cx| control.set_focusable(Some(true), cx));
        cx.subscribe(&control, move |_, event: &SpeedDialEvent, _| {
            sink.borrow_mut().push(*event);
        })
        .detach();
    });
    frames(window, 2);
    window.update(|window, cx| window.focus_next(cx));
    tap_enter(window);
    frames(window, 45);
    window.simulate_keystrokes("right");
    tap_enter(window);
    frames(window, 45);
    tap_enter(window);
    frames(window, 1);
    window.simulate_keystrokes("escape");
    assert_eq!(
        *events.borrow(),
        [
            SpeedDialEvent::Open(true),
            SpeedDialEvent::Action(0),
            SpeedDialEvent::Open(false),
            SpeedDialEvent::Open(true),
            SpeedDialEvent::Open(false),
        ]
    );
}
