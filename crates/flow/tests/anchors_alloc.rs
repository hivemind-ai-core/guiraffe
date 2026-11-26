//! Node and port anchors cost nothing to query, and a repaint that moves nothing records
//! them without allocating (#151).

use gpui_kit::{AppContext, TestAppContext, WindowOptions, test::TestWindowExt};
use guiraffe_flow::{
    Flow,
    core::{Graph, Node, Point},
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[gpui_kit::test]
fn anchor_queries_and_an_unchanged_frame_allocate_nothing(cx: &mut TestAppContext) {
    let (window, flow) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(
                    Graph::from_parts(
                        [
                            Node::new(1, "Source", Point::new(80., 120.)),
                            Node::new(2, "Target", Point::new(480., 120.)),
                        ],
                        [],
                    )
                    .expect("graph"),
                    cx,
                )
                .expect("flow")
            })
        })
        .expect("window")
    });
    let frame = |cx: &mut TestAppContext| {
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .expect("frame");
        cx.run_until_parked();
    };
    for _ in 0..3 {
        frame(cx);
    }
    let (anchors, counts) = count_allocations(|| {
        cx.update(|cx| {
            let flow = flow.read(cx);
            (
                flow.node_bounds(1, cx),
                flow.port_anchor(1, "out", cx),
                flow.port_anchor(2, "in", cx),
            )
        })
    });
    assert!(anchors.0.is_some() && anchors.1.is_some() && anchors.2.is_some());
    assert_eq!(counts.allocations, 0, "anchor queries allocated");

    // A repaint that moves nothing: the anchor check compares and emits nothing, so an
    // unchanged frame allocates no more than the one before it.
    let canvas = cx.update(|cx| flow.read(cx).viewport_entity().clone());
    let repaint = |cx: &mut TestAppContext| {
        count_allocations(|| {
            cx.update(|cx| canvas.update(cx, |_, cx| cx.notify()));
            frame(cx);
        })
        .1
        .allocations
    };
    repaint(cx);
    let first = repaint(cx);
    let second = repaint(cx);
    assert_eq!(first, second, "an unchanged frame's allocations vary");
}
