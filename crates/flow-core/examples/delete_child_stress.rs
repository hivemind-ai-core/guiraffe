//! Child deletion + undo, including incident-route and hierarchy-cache updates.
use guiraffe_flow_core::*;
use std::{collections::BTreeSet, hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = Graph::from_parts(
        (0..5000).map(|id| {
            let mut node = Node::new(
                id,
                "node",
                Point::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
            );
            if id == 1 {
                node.parent = Some(0);
            }
            node
        }),
        (0..10000).map(|id| {
            Edge::new(
                id,
                id % 5000,
                (id % 5000 + if id < 5000 { 1 } else { 100 }) % 5000,
            )
        }),
    )?;
    let mut editor = Editor::new(graph)?;
    editor.set_undo_limits(1);
    let mut times = Vec::with_capacity(100);
    for sample in 0..120 {
        editor.select(&BTreeSet::from([1]), false);
        let started = Instant::now();
        if !editor.delete_selected()? {
            return Err("child deletion unexpectedly did nothing".into());
        }
        if !editor.undo()? {
            return Err("child restoration unexpectedly did nothing".into());
        }
        black_box(editor.graph().nodes.get(&1));
        if sample >= 20 {
            times.push(started.elapsed().as_micros());
        }
    }
    times.sort_unstable();
    println!(
        "5000 nodes / 10000 edges; child delete + undo, 100 samples after 20 warmups: p50={}us p95={}us p99={}us",
        times[50], times[95], times[99]
    );
    Ok(())
}
