#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use gpui_kit::{AppContext, TestAppContext, TestDispatcher, WindowOptions, test::TestWindowExt};
use guiraffe_flow::FlowState;
use guiraffe_flow::core::{Edge, Graph, Node, Point, Viewport};

#[test]
fn mounted_50k_graph_stays_within_peak_rss_budget() -> anyhow::Result<()> {
    // The live input fixture is 47,426,111 bytes on the reference x86_64 host.
    // The component budget is 1.25 times the input plus 64 MiB.
    const LIMIT_KIB: u64 = (47_426_111 * 5 / 4 + 64 * 1024 * 1024) / 1024;
    let nodes = 50_000;
    let graph = Graph::from_parts(
        (0..nodes).map(|id| {
            Node::new(
                id,
                format!("Node {id}"),
                Point::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
            )
        }),
        (0..nodes * 2).map(|id| {
            let source = id % nodes;
            let step = if id < nodes { 1 } else { 100 };
            Edge::new(id, source, (source + step) % nodes)
        }),
    )?;
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("flow_retained_memory"));
    let (window, view) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| FlowState::new(graph, cx).expect("fixture validates"))
        })
    })?;
    window.update(&mut cx, |_, window, cx| window.render_frame(cx))?;
    cx.run_until_parked();
    for frame in 0..240 {
        let zoom = if frame % 31 < 15 { 1.25 } else { 1. };
        view.update(&mut cx, |view, cx| {
            view.set_viewport(
                Viewport {
                    pan: Point::new(
                        -(frame % 97) as f32 * 240. * zoom,
                        -(frame % 490) as f32 * 150. * zoom,
                    ),
                    zoom,
                },
                cx,
            )
        })?;
        window.update(&mut cx, |_, window, cx| window.render_frame(cx))?;
    }
    let status = std::fs::read_to_string("/proc/self/status")?;
    let peak = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_ascii_whitespace().next())
        .ok_or_else(|| anyhow::anyhow!("VmHWM unavailable"))?
        .parse::<u64>()?;
    assert!(
        peak <= LIMIT_KIB,
        "peak RSS {peak} KiB exceeds {LIMIT_KIB} KiB"
    );
    Ok(())
}
