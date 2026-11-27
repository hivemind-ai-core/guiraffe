//! Coloured edges cost no more per frame than plain ones (#160): edges batch by colour, so
//! an unchanged frame of one-colour edges allocates no more than a frame of plain edges does.

use gpui_kit::{AppContext, TestAppContext, WindowOptions, test::TestWindowExt};
use guiraffe_flow::{
    Flow,
    core::{Edge, Graph, Hue, Node, Point},
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn graph(color: Option<Hue>) -> Graph {
    let nodes = (0..12).map(|i| {
        Node::new(
            i,
            "n",
            Point::new((i % 4) as f32 * 220., (i / 4) as f32 * 140.),
        )
    });
    let edges = (1..12).map(|i| {
        let mut edge = Edge::new(i, i - 1, i);
        edge.color = color;
        edge
    });
    Graph::from_parts(nodes, edges).unwrap_or_else(|e| panic!("{e:?}"))
}

fn still_frame(cx: &mut TestAppContext, color: Option<Hue>) -> u64 {
    let (window, flow) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| Flow::new(graph(color), cx).unwrap_or_else(|e| panic!("{e:?}")))
        })
        .unwrap_or_else(|e| panic!("{e:?}"))
    });
    let canvas = cx.update(|cx| flow.read(cx).viewport_entity().clone());
    let frame = |cx: &mut TestAppContext| {
        count_allocations(|| {
            cx.update(|cx| canvas.update(cx, |_, cx| cx.notify()));
            window
                .update(cx, |_, window, cx| window.render_frame(cx))
                .unwrap_or_else(|e| panic!("{e:?}"));
            cx.run_until_parked();
        })
        .1
        .allocations
    };
    for _ in 0..4 {
        frame(cx);
    }
    let mut counts: Vec<u64> = (0..9).map(|_| frame(cx)).collect();
    counts.sort_unstable();
    counts[counts.len() / 2]
}

#[gpui_kit::test]
fn coloured_edges_allocate_as_plain_ones(cx: &mut TestAppContext) {
    // A window opened after another measures a little lower, so compare coloured edges
    // with plain ones measured after them too.
    let plain = still_frame(cx, None);
    let coloured = still_frame(cx, Some(Hue::Teal));
    let plain_after = still_frame(cx, None);
    assert!(
        coloured <= plain.max(plain_after),
        "{coloured} with coloured edges, {plain} and {plain_after} with plain"
    );
    eprintln!("coloured {coloured}, plain {plain} then {plain_after}");
}
