//! Headless Flow CPU frame timing. Samples include camera updates and GPUI render/paint.
use gpui_kit::{AppContext, TestAppContext, TestDispatcher, WindowOptions, test::TestWindowExt};
use guiraffe_flow::{
    Flow, FlowOptions,
    core::{Edge, Graph, Node, Point, Viewport},
};
use std::time::{Duration, Instant};

fn memory_kib() -> Result<[u64; 2], Box<dyn std::error::Error>> {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup")?;
    let read = |field: &str| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(rollup
            .lines()
            .find_map(|line| line.strip_prefix(field))
            .and_then(|value| value.split_whitespace().next())
            .ok_or_else(|| format!("missing {field} in smaps_rollup"))?
            .parse()?)
    };
    Ok([read("Rss:")?, read("Pss:")?])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let no_minimap = args.iter().any(|arg| arg == "--no-minimap");
    let soak = args.iter().any(|arg| arg == "--soak");
    let short = args.iter().any(|arg| arg == "--soak-short");
    let nodes = if soak || short { 50_000 } else { 5_000 };
    let mount_start = Instant::now();
    let graph = Graph::from_parts(
        (0..nodes).map(|i| {
            Node::new(
                i,
                format!("Node {i}"),
                Point::new((i % 100) as f32 * 240., (i / 100) as f32 * 150.),
            )
        }),
        (0..nodes * 2).map(|i| {
            Edge::new(
                i,
                i % nodes,
                ((i % nodes) + if i < nodes { 1 } else { 100 }) % nodes,
            )
        }),
    )?;
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
    window.update(&mut cx, |_, w, cx| w.render_frame(cx))?;
    let mount = mount_start.elapsed();
    println!(
        "nodes={nodes} edges={} setup_and_first_frame_us={}",
        nodes * 2,
        mount.as_micros()
    );

    let mut frame = |i: usize| -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        view.update(&mut cx, |v, cx| {
            v.set_viewport(
                Viewport {
                    pan: Point::new(
                        -(i % 100) as f32 * 200.,
                        -(i % (nodes as usize / 100)) as f32 * 100.,
                    ),
                    zoom: if (soak || short) && i % 31 < 15 {
                        1.25
                    } else {
                        1.
                    },
                },
                cx,
            )
        })?;
        window.update(&mut cx, |_, w, cx| w.render_frame(cx))?;
        Ok(start.elapsed())
    };

    let interval = Duration::from_secs(10);
    let mut index = 0;
    let mut times = Vec::with_capacity(1_200);
    let mut warmed_memory = None;
    let mut failed = soak && mount > Duration::from_millis(200);
    for sample in 0..if soak {
        181
    } else if short {
        2
    } else {
        1
    } {
        times.clear();
        let start = Instant::now();
        loop {
            let elapsed = frame(index)?;
            index += 1;
            if soak || short || index > 20 {
                times.push(elapsed.as_micros());
            }
            if soak || short {
                // Cap the workload at 120 Hz to approximate an input stream and
                // keep the sample storage bounded after warm-up.
                std::thread::sleep(Duration::from_micros(8_334).saturating_sub(elapsed));
                if start.elapsed() >= interval {
                    break;
                }
            } else if index == 220 {
                break;
            }
        }
        times.sort_unstable();
        let percentile = |p: usize| times[(times.len() * p).div_ceil(100) - 1];
        let (p50, p95, p99, max) = (
            percentile(50),
            percentile(95),
            percentile(99),
            percentile(100),
        );
        if soak || short {
            let [rss, pss] = memory_kib()?;
            let [base_rss, base_pss] = *warmed_memory.get_or_insert([rss, pss]);
            let drifted = rss * 100 > base_rss * 102 || pss * 100 > base_pss * 102;
            println!(
                "sample={sample} frames={} p50_us={p50} p95_us={p95} p99_us={p99} max_us={max} rss_kib={rss} pss_kib={pss} drifted={drifted}",
                times.len()
            );
            // Frame maximum is a stricter headless long-task gate than 50 ms.
            failed |= soak && sample > 0 && (drifted || p99 > 8_000 || max > 16_000);
        } else {
            println!("headless pan p50_us={p50} p95_us={p95} p99_us={p99} max_us={max}");
        }
    }
    if failed {
        return Err("Flow headless soak exceeded a timing or memory budget".into());
    }
    Ok(())
}
