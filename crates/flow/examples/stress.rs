//! Headless GPUI render/paint CPU benchmark, including culling, meshes and minimap.
//! This does not measure GPU presentation or establish desktop refresh rate.
use gpui_kit::{AppContext, TestAppContext, TestDispatcher, WindowOptions, test::TestWindowExt};
use guiraffe_flow::{
    Flow, FlowOptions,
    core::{Edge, Graph, Node, Point, Viewport},
};
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = Graph::from_parts(
        (0..5000).map(|i| {
            Node::new(
                i,
                format!("Node {i}"),
                Point::new((i % 100) as f32 * 240., (i / 100) as f32 * 150.),
            )
        }),
        (0..10000).map(|i| {
            Edge::new(
                i,
                i % 5000,
                ((i % 5000) + if i < 5000 { 1 } else { 100 }) % 5000,
            )
        }),
    )?;
    let no_minimap = std::env::args().any(|arg| arg == "--no-minimap");
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("flow_stress"));
    let (window, view) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(graph, cx)
                    .expect("validated stress graph")
                    .options(FlowOptions {
                        minimap: !no_minimap,
                        ..Default::default()
                    })
            })
        })
        .expect("window")
    });
    let mut times = Vec::new();
    for i in 0..220 {
        let start = Instant::now();
        view.update(&mut cx, |v, cx| {
            v.set_viewport(
                Viewport {
                    pan: Point::new(-(i % 100) as f32 * 200., -(i % 50) as f32 * 100.),
                    zoom: 1.,
                },
                cx,
            )
            .expect("camera")
        });
        window
            .update(&mut cx, |_, w, cx| w.render_frame(cx))
            .expect("frame");
        if i >= 20 {
            times.push(start.elapsed().as_micros());
        }
    }
    times.sort_unstable();
    println!(
        "last paint grid/edges/minimap (us): {:?}",
        view.read_with(&cx, |v, _| v.paint_timings())
    );
    println!(
        "visible nodes/edges: {:?}",
        view.read_with(&cx, |v, _| v.visible_counts())
    );
    println!(
        "5000 nodes / 10000 edges; GPUI headless render+paint CPU, 200 pan frames: p50={}us p95={}us p99={}us (no GPU/presentation; minimap={})",
        times[100], times[190], times[198], !no_minimap
    );
    Ok(())
}
