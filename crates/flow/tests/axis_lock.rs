//! Axis locks on node moves (issue 150): every lock value by pointer and by bus, the per-node
//! override, the Shift modifier's dominant-axis lock, and a multi-node bus move as one undo
//! step.
use std::collections::BTreeMap;

use gpui_kit::{TestAppContext, VisualTestContext};
use guiraffe_core::protocol::{ActionBus, Source};
use guiraffe_flow::{
    FlowAction, FlowState, Interaction,
    core::{AxisLock, Graph, Node, Point},
};

fn graph(lock: AxisLock, node_lock: Option<AxisLock>) -> Graph {
    let mut first = Node::new(1, "One", Point::new(80., 120.));
    first.axis_lock = node_lock;
    let mut graph = Graph::from_parts([first, Node::new(2, "Two", Point::new(480., 120.))], [])
        .unwrap_or_else(|e| panic!("{e:?}"));
    graph.axis_lock = lock;
    graph
}

/// Drag node 1 by (50, 90) on screen at zoom 1, with or without Shift.
fn drag(lock: AxisLock, node_lock: Option<AxisLock>, shift: bool) -> (Point, Point) {
    let mut i = Interaction::new(graph(lock, node_lock)).unwrap_or_else(|e| panic!("{e:?}"));
    i.grid = None;
    i.press(Point::new(100., 140.), false, false);
    i.motion_with(Point::new(150., 230.), shift);
    let preview = i.preview(1).unwrap_or_else(|| panic!("preview"));
    assert!(i.release(None).is_ok());
    let committed = i.editor.graph().nodes[&1].pos;
    (preview, committed)
}

#[test]
fn pointer_drags_honour_every_lock() {
    for (lock, want) in [
        (AxisLock::Unlocked, Point::new(130., 210.)),
        (AxisLock::LockX, Point::new(80., 210.)),
        (AxisLock::LockY, Point::new(130., 120.)),
    ] {
        let (preview, committed) = drag(lock, None, false);
        assert_eq!(committed, want, "{lock:?}");
        assert_eq!(
            preview, committed,
            "{lock:?}: the preview matches the commit"
        );
    }
}

#[test]
fn a_node_lock_overrides_the_graph_default() {
    let (_, committed) = drag(AxisLock::LockY, Some(AxisLock::LockX), false);
    assert_eq!(committed, Point::new(80., 210.));
    let (_, committed) = drag(AxisLock::LockX, Some(AxisLock::Unlocked), false);
    assert_eq!(committed, Point::new(130., 210.));
}

#[test]
fn shift_locks_the_dominant_axis_unless_a_lock_is_set() {
    // The pointer moved further down than across: x stays.
    let (preview, committed) = drag(AxisLock::Unlocked, None, true);
    assert_eq!(committed, Point::new(80., 210.));
    assert_eq!(preview, committed);
    // An explicit lock wins over the modifier.
    let (_, committed) = drag(AxisLock::LockY, None, true);
    assert_eq!(committed, Point::new(130., 120.));
}

#[test]
fn a_locked_axis_keeps_its_off_grid_coordinate() {
    let mut i = Interaction::new(graph(AxisLock::LockX, None)).unwrap_or_else(|e| panic!("{e:?}"));
    i.grid = Some(20.);
    let mut g = i.editor.graph().clone();
    if let Some(n) = g.nodes.get_mut(&1) {
        n.pos = Point::new(83., 120.);
    }
    i = Interaction::new(g).unwrap_or_else(|e| panic!("{e:?}"));
    i.press(Point::new(100., 140.), false, false);
    i.motion_with(Point::new(150., 230.), false);
    let preview = i.preview(1).unwrap_or_else(|| panic!("preview"));
    assert!(i.release(None).is_ok());
    let committed = i.editor.graph().nodes[&1].pos;
    assert_eq!(committed, Point::new(83., 220.), "y snaps, x keeps 83");
    assert_eq!(preview, committed);
}

fn mount(
    cx: &mut TestAppContext,
    graph: Graph,
) -> (gpui_kit::Entity<FlowState>, &mut VisualTestContext) {
    cx.update(guiraffe_core::init);
    let (view, vcx) =
        cx.add_window_view(|_, cx| FlowState::new(graph, cx).unwrap_or_else(|e| panic!("{e:?}")));
    vcx.run_until_parked();
    (view, vcx)
}

fn pos(view: &gpui_kit::Entity<FlowState>, vcx: &mut VisualTestContext, id: u64) -> Point {
    vcx.update(|_, cx| view.read(cx).graph().nodes[&id].pos)
}

#[gpui_kit::test]
fn bus_moves_honour_every_lock(cx: &mut TestAppContext) {
    for (lock, want) in [
        (AxisLock::Unlocked, Point::new(300., 400.)),
        (AxisLock::LockX, Point::new(80., 400.)),
        (AxisLock::LockY, Point::new(300., 120.)),
    ] {
        let (view, vcx) = mount(cx, graph(lock, None));
        vcx.update(|window, cx| {
            let id = FlowState::register(&view, "graph", window, cx);
            let action = FlowAction::move_node(1, Point::new(300., 400.)).to_action();
            assert!(ActionBus::dispatch(id, &action, Source::Script, window, cx).is_ok());
        });
        assert_eq!(pos(&view, vcx, 1), want, "{lock:?}");
    }
}

#[gpui_kit::test]
fn a_node_lock_overrides_the_graph_default_on_the_bus(cx: &mut TestAppContext) {
    let (view, vcx) = mount(cx, graph(AxisLock::LockY, Some(AxisLock::LockX)));
    vcx.update(|window, cx| {
        let id = FlowState::register(&view, "graph", window, cx);
        let action = FlowAction::move_node(1, Point::new(300., 400.)).to_action();
        assert!(ActionBus::dispatch(id, &action, Source::Script, window, cx).is_ok());
    });
    assert_eq!(pos(&view, vcx, 1), Point::new(80., 400.));
}

#[gpui_kit::test]
fn a_multi_node_bus_move_is_one_undo_step(cx: &mut TestAppContext) {
    let (view, vcx) = mount(cx, graph(AxisLock::LockX, None));
    vcx.update(|window, cx| {
        let id = FlowState::register(&view, "graph", window, cx);
        let action = FlowAction::MoveNodes(BTreeMap::from([
            (1, Point::new(300., 400.)),
            (2, Point::new(600., 500.)),
        ]))
        .to_action();
        assert!(ActionBus::dispatch(id, &action, Source::Script, window, cx).is_ok());
    });
    assert_eq!(pos(&view, vcx, 1), Point::new(80., 400.));
    assert_eq!(pos(&view, vcx, 2), Point::new(480., 500.));
    vcx.update(|_, cx| view.update(cx, |v, cx| v.undo(cx)));
    assert_eq!(pos(&view, vcx, 1), Point::new(80., 120.));
    assert_eq!(pos(&view, vcx, 2), Point::new(480., 120.));
}
