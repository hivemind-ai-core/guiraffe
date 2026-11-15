use crate::{Flow, FlowError, Graph, Node, NodeRenderer, Viewport, WorldPoint};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Entity, IntoElement, ParentElement, ScrollDelta,
    Window, WindowOptions, div, point, px,
};
use gpui_kit::{TestAppContext, test::TestWindowExt};
use std::{cell::Cell, rc::Rc};
fn fixture() -> Graph {
    Graph::from_parts(
        [
            Node::new(1, "Source", WorldPoint::new(80., 120.)),
            Node::new(2, "Target", WorldPoint::new(480., 120.)),
        ],
        [],
    )
    .expect("fixture")
}
#[gpui_kit::test]
fn canvas_mounts_and_controls_change_viewport(cx: &mut TestAppContext) {
    let (window, view) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| Flow::new(fixture(), cx).expect("flow"))
        })
        .expect("window")
    });
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.find("flow");
            window.click("flow-zoom-in", cx);
        })
        .expect("update");
    assert!(view.read_with(cx, |v, _| v.viewport().zoom) > 1.);
    window
        .update(cx, |_, window, cx| window.click("flow-fit", cx))
        .expect("fit");
    assert!(view.read_with(cx, |v, _| v.viewport().valid()));
}
fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Flow>) {
    let result = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| Flow::new(fixture(), cx).expect("flow"))
        })
        .expect("window")
    });
    result
        .0
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .expect("frame");
    result
}
#[gpui_kit::test]
fn real_pointer_drag_snaps_and_keyboard_undo_redo(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    w.update(cx, |_, w, cx| {
        let bounds = w.find(("flow-node", 1u64)).bounds();
        let from = bounds.center();
        w.drag(from, from + point(px(60.), px(40.)), cx);
    })
    .expect("drag");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(140., 160.)
    );
    w.update(cx, |_, w, cx| w.press("ctrl-z", cx))
        .expect("undo");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(80., 120.)
    );
    w.update(cx, |_, w, cx| w.press("ctrl-shift-z", cx))
        .expect("redo");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(140., 160.)
    );
}
#[gpui_kit::test]
fn real_handle_drag_creates_edge_and_duplicate_emits_error(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    for _ in 0..2 {
        w.update(cx, |_, w, cx| {
            w.drag(point(px(240.), px(156.)), point(px(480.), px(156.)), cx)
        })
        .expect("connect");
    }
    assert_eq!(v.read_with(cx, |v, _| v.graph().edges.len()), 1);
    assert_eq!(
        v.read_with(cx, |v, _| v.last_error().cloned()),
        Some(FlowError::DuplicateConnection)
    );
}
#[gpui_kit::test]
fn keyboard_selection_delete_and_controls_restore_document(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    w.update(cx, |_, w, cx| {
        w.click(("flow-node", 1u64), cx);
        w.press("ctrl-a", cx);
        w.press("delete", cx);
    })
    .expect("delete");
    assert!(v.read_with(cx, |v, _| v.graph().nodes.is_empty()));
    w.update(cx, |_, w, cx| w.click("flow-undo", cx))
        .expect("undo");
    assert_eq!(v.read_with(cx, |v, _| v.graph().nodes.len()), 2);
}
#[gpui_kit::test]
fn wheel_pans_and_keyboard_zoom_preserves_center(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    w.update(cx, |_, w, cx| {
        w.click_at("flow", point(px(30.), px(400.)), cx);
        w.scroll("flow", ScrollDelta::Pixels(point(px(20.), px(-30.))), cx);
    })
    .expect("wheel");
    assert_eq!(
        v.read_with(cx, |v, _| v.viewport().pan),
        WorldPoint::new(20., -30.)
    );
    let before = v.read_with(cx, |v, _| v.viewport().world(v.size() * 0.5));
    w.update(cx, |_, w, cx| w.press("=", cx)).expect("zoom");
    let after = v.read_with(cx, |v, _| v.viewport().world(v.size() * 0.5));
    assert!(before.distance(after) < 0.001);
}
#[gpui_kit::test]
fn culling_unmounts_distant_nodes_and_custom_renderer_is_called(cx: &mut TestAppContext) {
    struct Renderer(Rc<Cell<usize>>);
    impl NodeRenderer for Renderer {
        fn render(&self, _: &Node, _: f32, _: &mut Window, _: &mut App) -> AnyElement {
            self.0.set(self.0.get() + 1);
            div().child("custom").into_any_element()
        }
    }
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .node_renderer(Renderer(observed))
            })
        })
        .expect("window")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
        w.find(("flow-node", 1u64));
    })
    .expect("frame");
    assert!(count.get() > 0);
    v.update(cx, |v, cx| {
        v.set_viewport(
            Viewport {
                pan: WorldPoint::new(-10000., -10000.),
                zoom: 1.,
            },
            cx,
        )
        .expect("camera")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.try_find(("flow-node", 1u64)).is_none());
    })
    .expect("cull");
}
#[gpui_kit::test]
fn native_pinch_preserves_world_point_under_pointer(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent, PinchEvent};
    let (w, v) = open(cx);
    let at = WorldPoint::new(330., 270.);
    let before = v.read_with(cx, |v, _| v.viewport().world(at));
    w.update(cx, |_, w, cx| {
        w.dispatch_event(
            PinchEvent {
                position: point(px(at.x), px(at.y)),
                delta: 0.3,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        w.render_frame(cx);
    })
    .expect("pinch");
    assert!(
        v.read_with(cx, |v, _| v.viewport().world(at))
            .distance(before)
            < 0.001
    );
    assert!(v.read_with(cx, |v, _| v.viewport().zoom) > 1.);
}
#[gpui_kit::test]
fn pan_updates_observed_node_bounds(cx: &mut TestAppContext) {
    struct Renderer(Rc<Cell<usize>>);
    impl NodeRenderer for Renderer {
        fn render(&self, _: &Node, _: f32, _: &mut Window, _: &mut App) -> AnyElement {
            self.0.set(self.0.get() + 1);
            div().child("retained").into_any_element()
        }
    }
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .node_renderer(Renderer(observed))
            })
        })
        .expect("window")
    });
    let before = w
        .update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
            w.find(("flow-node", 1u64)).bounds().origin
        })
        .expect("frame");
    v.update(cx, |v, cx| {
        v.set_viewport(
            Viewport {
                pan: WorldPoint::new(10., 20.),
                zoom: 1.,
            },
            cx,
        )
        .expect("camera")
    });
    let after = w
        .update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.find(("flow-node", 1u64)).bounds().origin
        })
        .expect("pan");
    assert!(count.get() > 0);
    assert_eq!(after, before + point(px(10.), px(20.)));
}
