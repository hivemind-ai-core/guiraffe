//! The inspect API (spec/21 §2): stable node ids map to the element tree and
//! to the accessibility facts of the platform.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, IntoElement, Render, Role, TestAppContext, TestSupportExt, VisualTestContext, Window,
    div,
};
use guiraffe_test::inspect::{NodeId, NodeIndex};

/// A small app with two identified controls and a plain text.
struct Host {
    clicks: Rc<Cell<usize>>,
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
                    .role(Role::Button)
                    .aria_label("Save")
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                    .child("Save"),
            )
            .child(
                div()
                    .id("cancel")
                    .test_support()
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
    let (_, vcx) = cx.add_window_view(move |_, _| Host {
        clicks: host_clicks,
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
