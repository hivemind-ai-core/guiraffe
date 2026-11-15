//! Compare persistent edits with full scene reconstruction on the same document.
use guiraffe_flow_core::*;
use std::{collections::BTreeMap, hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = Graph::from_parts(
        (0..5000).map(|id| {
            Node::new(
                id,
                "node",
                Point::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
            )
        }),
        (0..10000).map(|id| {
            Edge::new(
                id,
                id % 5000,
                (id % 5000 + if id < 5000 { 1 } else { 100 }) % 5000,
            )
        }),
    )?;
    let mut editor = Editor::new(graph.clone())?;
    let mut rebuild = graph;
    let (mut edits, mut full) = (Vec::new(), Vec::new());
    for step in 0..120 {
        let at = Point::new(30. + step as f32, 20. + step as f32);
        let start = Instant::now();
        editor.move_nodes(&BTreeMap::from([(1000, at)]), None)?;
        if step >= 20 {
            edits.push(start.elapsed().as_micros());
        }
        let start = Instant::now();
        rebuild.nodes.get_mut(&1000).ok_or("node missing")?.pos = at;
        black_box(Scene::new(rebuild.clone())?);
        if step >= 20 {
            full.push(start.elapsed().as_micros());
        }
    }
    edits.sort_unstable();
    full.sort_unstable();
    println!(
        "5000 nodes/10000 edges; 100 measured moves after 20 warmups; history limit=100; persistent edit p50={}us p95={}us p99={}us; full reconstruction reference p50={}us p95={}us p99={}us",
        edits[50], edits[95], edits[99], full[50], full[95], full[99]
    );
    Ok(())
}
