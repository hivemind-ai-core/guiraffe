//! The headless app: the renderer of the snapshots serves the API, so a
//! capture is a picture and a script runs without a display server.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, HeadlessAppContext, IntoElement, Render, Role, TestAppContext,
    TestSupportExt, Window, div, px, size,
};
use guiraffe_test::drive::{Action, Driver};
use guiraffe_test::inspect::{EventSink, HeadlessTarget, InspectTarget, NodeId};
use guiraffe_test::replay::{Assertion, Script, Step};

struct App {
    events: EventSink,
    clicks: Rc<Cell<usize>>,
    focus: FocusHandle,
}

impl Render for App {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        let clicks = self.clicks.clone();
        div().id("panel").test_support().child(
            div()
                .id("save")
                .test_support()
                .track_focus(&self.focus)
                .focusable()
                .role(Role::Button)
                .aria_label("Save")
                .w(px(120.))
                .h(px(40.))
                .on_click(move |_, _, _| {
                    clicks.set(clicks.get() + 1);
                    events.emit("save.clicked", None);
                })
                .child("Save"),
        )
    }
}

/// The headless app: the context with the renderer of the snapshots, and the
/// window of the app.
fn headless() -> (
    HeadlessAppContext,
    gpui_kit::AnyWindowHandle,
    EventSink,
    Rc<Cell<usize>>,
) {
    let events = EventSink::new();
    let clicks = Rc::new(Cell::new(0));
    let app_events = events.clone();
    let app_clicks = clicks.clone();
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(guiraffe_icons::GuiraffeAssets),
        gpui_kit::platform::current_headless_renderer,
    );
    let window = cx
        .open_window(size(px(400.), px(300.)), move |_, app| {
            app.new(|cx| App {
                events: app_events,
                clicks: app_clicks,
                focus: cx.focus_handle().tab_stop(true),
            })
        })
        .expect("the headless window opens");
    (cx, window.into(), events, clicks)
}

#[gpui_kit::test]
fn the_headless_app_serves_a_picture_and_runs_a_script(cx: &mut TestAppContext) {
    let _ = cx;
    let (mut cx, window, events, clicks) = headless();
    let mut target = HeadlessTarget::new(&mut cx, window);
    target.park();
    let mut driver = Driver::new(&mut target, events);

    // The capture is a picture, not nothing: the headless context has the
    // renderer of the snapshots.
    let png = driver.screenshot(None).expect("the window is captured");
    assert!(png.starts_with(b"\x89PNG"), "the capture is a PNG");

    // The script runs again against the same app.
    let script = Script::new(
        "save",
        vec![Step {
            action: Action::Click {
                id: NodeId::new("save"),
            },
            assert: vec![
                Assertion::Present {
                    id: NodeId::new("save"),
                },
                Assertion::Events {
                    names: vec!["save.clicked".into()],
                },
            ],
        }],
    );
    let result = guiraffe_test::replay::run(&mut driver, &script);
    assert!(result.ok(), "{result:?}");
    assert_eq!(clicks.get(), 1, "the action ran");
}

#[gpui_kit::test]
fn the_headless_app_answers_the_inspection(cx: &mut TestAppContext) {
    let _ = cx;
    let (mut cx, window, events, _clicks) = headless();
    let mut target = HeadlessTarget::new(&mut cx, window);
    target.park();
    let mut driver = Driver::new(&mut target, events);
    let nodes = driver.nodes();
    assert!(
        nodes.node(&NodeId::new("save")).is_some(),
        "the nodes of the headless app are found"
    );
    let a11y = driver.a11y();
    assert!(!a11y.focus_order.is_empty(), "the focus order is measured");
}
