//! The inspect API (spec/21 §2): stable node ids map to the element tree and
//! to the accessibility facts of the platform, and the inspector answers what
//! the last frame says.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, IntoElement, Render, Role, TestAppContext, TestSupportExt,
    VisualTestContext, Window, div,
};
use guiraffe_test::inspect::{EventSink, Inspector, NodeId, NodeIndex};

/// A small app with two identified controls and a plain text.
struct Host {
    clicks: Rc<Cell<usize>>,
    save_focus: FocusHandle,
    cancel_focus: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .id("panel")
            .test_support()
            .p_4()
            .child(
                div()
                    .id("save")
                    .test_support()
                    .track_focus(&self.save_focus)
                    .focusable()
                    .role(Role::Button)
                    .aria_label("Save")
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
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
            .child(div().id("caption").test_support().child("A caption"))
    }
}

fn mount(cx: &mut TestAppContext) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    let clicks = Rc::new(Cell::new(0));
    let host_clicks = clicks.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| Host {
        clicks: host_clicks,
        save_focus: cx.focus_handle().tab_stop(true),
        cancel_focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    (clicks, vcx)
}

#[gpui_kit::test]
fn a_node_id_maps_to_the_bounds_and_the_accessibility_facts(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let index = vcx.update(|window, _| NodeIndex::collect(window));

    let save = index.node(&NodeId::new("save")).expect("the save node");
    assert_eq!(save.a11y.role.as_deref(), Some("Button"));
    assert_eq!(save.a11y.name.as_deref(), Some("Save"));
    assert!(save.layout.bounds.width > 0.0, "the node has a width");
    assert!(save.layout.bounds.height > 0.0, "the node has a height");
    assert!(save.layout.visible);

    let cancel = index.node(&NodeId::new("cancel")).expect("the cancel node");
    assert_eq!(cancel.a11y.role.as_deref(), Some("Button"));
    assert_eq!(cancel.a11y.name.as_deref(), Some("Cancel"));

    // The ids of the nodes are the ids the widgets carry.
    let ids = index.ids();
    assert!(ids.contains(&NodeId::new("panel")), "{ids:?}");
    assert!(ids.contains(&NodeId::new("save")), "{ids:?}");
    assert!(ids.contains(&NodeId::new("caption")), "{ids:?}");
}

#[gpui_kit::test]
fn a_missing_node_is_not_invented(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let index = vcx.update(|window, _| NodeIndex::collect(window));
    assert!(index.node(&NodeId::new("nope")).is_none());
}

#[gpui_kit::test]
fn the_layout_of_a_node_answers_where_it_is(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let mut inspector = Inspector::new(vcx);
    let report = inspector.layout(Some(&NodeId::new("save")));
    assert_eq!(report.nodes.len(), 1, "only the node is reported");
    let node = &report.nodes[0];
    assert!(node.layout.bounds.width > 0.0);
    assert!(node.layout.visible);

    // Without a node every node is reported.
    let all = inspector.layout(None);
    assert!(all.nodes.len() >= 3, "every node is reported");
}

#[gpui_kit::test]
fn the_accessibility_report_answers_the_tab_order(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let mut inspector = Inspector::new(vcx);
    let report = inspector.a11y();
    assert!(
        report
            .nodes
            .iter()
            .any(|node| node.a11y.name.as_deref() == Some("Save")),
        "the nodes carry their names"
    );
    assert!(
        report.focus_order.contains(&NodeId::new("save")),
        "the tab order visits the save button: {:?}",
        report.focus_order
    );
}

#[gpui_kit::test]
fn the_events_of_the_app_are_reported(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let mut inspector = Inspector::new(vcx);
    let sink = inspector.events().clone();
    sink.emit("save.clicked", Some("\"Save\"".into()));
    let events = inspector.event_log();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "save.clicked");
    assert_eq!(events[0].payload.as_deref(), Some("\"Save\""));
}

#[gpui_kit::test]
fn the_style_report_answers_the_resolved_tokens(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let mut inspector = Inspector::new(vcx);
    let style = inspector.style();
    assert!(style.radius > 0.0, "the theme has a radius");
    assert!(
        style.colors.contains_key("primary"),
        "the colours of the theme are resolved: {:?}",
        style.colors
    );
    assert_eq!(
        style.control_size, "Sm",
        "the context of the app is reported"
    );
}

#[gpui_kit::test]
fn a_screenshot_is_offered_when_the_platform_can_render(cx: &mut TestAppContext) {
    let (_clicks, vcx) = mount(cx);
    let mut inspector = Inspector::new(vcx);
    // The platform of the test context has no renderer, so no capture is
    // offered; the headless renderer (the snapshot harness) renders the image.
    assert!(
        inspector.screenshot(None).is_none(),
        "the test platform cannot render"
    );
}

#[test]
fn the_event_sink_keeps_the_events_in_order() {
    let sink = EventSink::new();
    assert!(sink.is_empty());
    sink.emit("first", None);
    sink.emit("second", Some("2".into()));
    let events = sink.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].name, "first");
    assert_eq!(events[1].name, "second");
    sink.clear();
    assert!(sink.is_empty());
}
