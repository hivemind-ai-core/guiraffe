//! Criterion-free spatial/path benchmark; CPU geometry timings are not GPU FPS.
use guiraffe_flow_core::*;
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nodes = (0..5000).map(|i| {
        Node::new(
            i,
            format!("Node {i}"),
            Point::new((i % 100) as f32 * 240., (i / 100) as f32 * 150.),
        )
    });
    let edges = (0..10000).map(|i| {
        Edge::new(
            i,
            i % 5000,
            ((i % 5000) + if i < 5000 { 1 } else { 100 }) % 5000,
        )
    });
    let graph = Graph::from_parts(nodes, edges)?;
    let started = Instant::now();
    let scene = Scene::new(graph)?;
    let build = started.elapsed();
    let mut visible = Visible::default();
    visible.nodes.reserve(5000);
    visible.edges.reserve(10000);
    let mut timings = Vec::with_capacity(10000);
    let mut checksum = 0;
    for i in 0..10100 {
        let area = Rect::new(
            (i % 100) as f32 * 200.,
            (i % 50) as f32 * 100.,
            1920.,
            1080.,
        );
        let start = Instant::now();
        scene.visible(black_box(area), &mut visible);
        for id in &visible.edges {
            checksum += black_box(scene.paths[id].points.len());
        }
        if i >= 100 {
            timings.push(start.elapsed().as_nanos());
        }
    }
    timings.sort_unstable();
    println!(
        "5000 nodes / 10000 edges; build={build:?}; query + cached route access: p50={}us p95={}us p99={}us; visible={} nodes/{} edges; branches={}; checksum={checksum}",
        timings[5000] / 1000,
        timings[9500] / 1000,
        timings[9900] / 1000,
        visible.nodes.len(),
        visible.edges.len(),
        visible.visited
    );
    Ok(())
}
