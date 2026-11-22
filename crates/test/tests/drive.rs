//! The drive API: the actions an agent performs on a running app,
//! and what they changed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, IntoElement, Render, Role, TestAppContext, TestSupportExt,
    VisualTestContext, Window, div,
};
use guiraffe_test::drive::{Action, Driver};
use guiraffe_test::inspect::{EventSink, NodeId};

struct Host {
    events: EventSink,
    clicks: Rc<Cell<usize>>,
    save_focus: FocusHandle,
    cancel_focus: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        let clicks = self.clicks.clone();
        div()
            .id("panel")
            .test_support()
            .child(
                div()
                    .id("save")
                    .test_support()
                    .track_focus(&self.save_focus)
                    .focusable()
                    .role(Role::Button)
                    .aria_label("Save")
                    .on_click(move |_, _, _| {
                        clicks.set(clicks.get() + 1);
                        events.emit("save.clicked", Some("1".into()));
                    })
                    .child("Save"),
            )
            .child(
                div()
                    .id("cancel")
                    .test_support()
                    .track_focus(&self.cancel_focus)
                    .focusable()
                    .role(Role::Button)
                    .aria_label("Cancel")
                    .child("Cancel"),
            )
    }
}

fn mount(cx: &mut TestAppContext) -> (EventSink, Rc<Cell<usize>>, &mut VisualTestContext) {
    let events = EventSink::new();
    let clicks = Rc::new(Cell::new(0));
    let host_events = events.clone();
    let host_clicks = clicks.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| Host {
        events: host_events,
        clicks: host_clicks,
        save_focus: cx.focus_handle().tab_stop(true),
        cancel_focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    (events, clicks, vcx)
}

#[gpui_kit::test]
fn a_click_runs_the_handler_and_reports_the_events(cx: &mut TestAppContext) {
    let (events, clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::Click {
        id: NodeId::new("save"),
    });
    assert!(result.ok, "{result:?}");
    assert_eq!(clicks.get(), 1, "the handler ran");
    assert_eq!(result.events.len(), 1, "the event was reported");
    assert_eq!(result.events[0].name, "save.clicked");
}

#[gpui_kit::test]
fn a_click_on_a_missing_node_fails_with_a_reason(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::Click {
        id: NodeId::new("nope"),
    });
    assert!(!result.ok);
    let error = result.error.clone().unwrap_or_default();
    assert!(error.contains("nope"), "{error:?}");
}

#[gpui_kit::test]
fn a_key_moves_the_focus_and_the_result_says_so(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::Key { key: "tab".into() });
    assert!(result.ok, "{result:?}");
    assert!(result.focus.moved(), "the focus moved: {result:?}");
    assert_eq!(result.focus.after, Some(NodeId::new("save")));
}

#[gpui_kit::test]
fn a_focus_action_walks_the_focus_to_the_node(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::Focus {
        id: NodeId::new("cancel"),
    });
    assert!(result.ok, "{result:?}");
    assert_eq!(result.focus.after, Some(NodeId::new("cancel")));
}

#[gpui_kit::test]
fn a_hover_and_a_scroll_run(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let hovered = driver.drive(&Action::Hover {
        id: NodeId::new("save"),
    });
    assert!(hovered.ok, "{hovered:?}");
    let scrolled = driver.drive(&Action::Scroll {
        id: NodeId::new("panel"),
        dx: 0.,
        dy: 10.,
    });
    assert!(scrolled.ok, "{scrolled:?}");
}

#[gpui_kit::test]
fn a_resize_moves_the_nodes_that_follow_the_window(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::Resize {
        width: 640.,
        height: 480.,
    });
    assert!(result.ok, "{result:?}");
}

#[gpui_kit::test]
fn the_theme_and_the_fixture_are_switched_through_the_hooks(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let switched = Rc::new(RefCell::new(String::new()));
    let recorded = Rc::clone(&switched);
    let mut driver = Driver::new(vcx, events)
        .with_theme(Box::new({
            let recorded = Rc::clone(&recorded);
            move |scheme, _cx| {
                *recorded.borrow_mut() = scheme.to_string();
                true
            }
        }))
        .with_fixture(Box::new({
            let recorded = Rc::clone(&recorded);
            move |name, _cx| {
                *recorded.borrow_mut() = name.to_string();
                true
            }
        }));
    let themed = driver.drive(&Action::SetTheme {
        scheme: "dark".into(),
    });
    assert!(themed.ok, "{themed:?}");
    assert_eq!(switched.borrow().as_str(), "dark");
    let fixture = driver.drive(&Action::SetFixture {
        name: "empty".into(),
    });
    assert!(fixture.ok, "{fixture:?}");
    assert_eq!(switched.borrow().as_str(), "empty");

    // An app without a hook says so.
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::SetTheme {
        scheme: "dark".into(),
    });
    assert!(!result.ok);
    let error = result.error.clone().unwrap_or_default();
    assert!(error.contains("theme"), "{error:?}");
}

#[gpui_kit::test]
fn a_type_action_writes_into_the_focused_node(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = driver.drive(&Action::Type {
        id: NodeId::new("save"),
        text: "hello".into(),
    });
    assert!(result.ok, "{result:?}");
    assert_eq!(result.focus.after, Some(NodeId::new("save")));
}

#[test]
fn an_action_reports_its_name() {
    assert_eq!(
        Action::Click {
            id: NodeId::new("save")
        }
        .name(),
        "click"
    );
    assert_eq!(
        Action::SetTheme {
            scheme: "dark".into()
        }
        .name(),
        "set_theme"
    );
}
