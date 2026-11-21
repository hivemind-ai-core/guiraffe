//! Headless CPU interaction timing. Samples include GPUI event dispatch and render/paint.
use gpui_kit::{
    AppContext, Bounds, MouseButton, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext,
    TestDispatcher, TouchPhase, VisualTestContext, WindowBounds, WindowOptions, point, px,
    test::TestWindowExt,
};
use guiraffe_charts::{Channels, ChartOptions, ChartSpec, ChartView, Layer, Mark, Stream};
use guiraffe_flow::{
    Flow, FlowOptions,
    core::{Edge, Graph, Node, Point, Viewport},
};
use guiraffe_timeline::{Channel, DopeSheet, TimeFormat};
use std::time::{Duration, Instant};

fn memory_kib() -> Result<[u64; 2]> {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup")?;
    let read = |field: &str| -> Result<u64> {
        Ok(rollup
            .lines()
            .find_map(|line| line.strip_prefix(field))
            .and_then(|value| value.split_whitespace().next())
            .ok_or_else(|| format!("missing {field} in smaps_rollup"))?
            .parse()?)
    };
    Ok([read("Rss:")?, read("Pss:")?])
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// `SOAK_SCALE=n` divides every dataset size by `n` (profiling under callgrind).
fn scaled(size: usize) -> usize {
    let divisor = std::env::var("SOAK_SCALE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    (size / divisor).max(1)
}

fn flow(no_minimap: bool, run: impl FnOnce(&mut dyn FnMut(usize) -> Result<()>)) -> Result<()> {
    let nodes = scaled(50_000) as u64;
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
    run(&mut |i| {
        let zoom = if i % 31 < 15 { 1.25 } else { 1. };
        view.update(&mut cx, |v, cx| {
            v.set_viewport(
                Viewport {
                    pan: Point::new(
                        -((i % 97) as f32) * 240. * zoom,
                        -((i % 490) as f32) * 150. * zoom,
                    ),
                    zoom,
                },
                cx,
            )
        })?;
        window.update(&mut cx, |_, w, cx| w.render_frame(cx))?;
        Ok(())
    });
    Ok(())
}

fn timeline(run: impl FnOnce(&mut dyn FnMut(usize) -> Result<()>)) {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("timeline_soak"));
    cx.update(|cx| {
        guiraffe_core::init(cx);
        guiraffe_timeline::init(cx);
    });
    let (_, window) = cx.add_window_view(|window, cx| {
        let channels = (0..100)
            .map(|i| {
                Channel::new(
                    format!("Lane {i} · عربي 日本語 🎵"),
                    (0..scaled(1_000)).map(|k| k as f64).collect(),
                )
            })
            .collect();
        let sheet = DopeSheet::new(channels, TimeFormat::Frames(24.), 1_000., cx);
        gpui_kit::base::Root::new(sheet, window, cx)
    });
    window.update(|window, cx| window.render_frame(cx));
    window.run_until_parked();
    run(&mut |i| {
        let b = window
            .update(|window, _| window.try_find("dope-body").map(|item| item.bounds()))
            .ok_or("missing dope sheet body")?;
        let at = b.origin + point(b.size.width * 0.45, px(50.));
        if i % 4 == 0 {
            let target = at + point(px(8.), px(0.));
            window.simulate_mouse_down(at, MouseButton::Left, Default::default());
            window.simulate_mouse_move(target, Some(MouseButton::Left), Default::default());
            window.simulate_mouse_up(target, MouseButton::Left, Default::default());
        } else {
            let delta = if i % 2 == 0 { -20. } else { 20. };
            window.simulate_event(ScrollEvent {
                position: at,
                delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
                source: ScrollSource::Pointer,
            });
        }
        window.run_until_parked();
        Ok(())
    });
}

fn streaming_spec(stream: Stream<u32>) -> ChartSpec {
    ChartSpec::new("Streaming observations").layer(Layer::new(
        Mark::Line,
        stream,
        Channels::new(|i: &u32| f64::from(*i), |i| (f64::from(*i) * 0.002).sin()),
    ))
}

fn charts(
    scatter: bool,
    streaming: bool,
    run: impl FnOnce(&mut dyn FnMut(usize) -> Result<()>, Duration),
) -> Result<()> {
    const STREAM_RATE: u32 = 10_000;
    const STREAM_BATCH: u32 = 500;
    let mut stream = Stream::default();
    if streaming {
        stream.append((0..STREAM_RATE).collect());
    }
    let mount_started = Instant::now();
    let cx = TestAppContext::build(TestDispatcher::new(0), Some("charts_soak"));
    cx.update(|cx| {
        gpui_kit::base::init(cx);
        guiraffe_charts::init(cx);
        gpui_kit::component::init(cx);
        guiraffe_core::apply(Default::default(), cx);
    });
    let (handle, view) = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::maximized(None, cx))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| {
            let (mark, count) = if scatter {
                (Mark::Point, scaled(1_000_000) as u32)
            } else {
                (Mark::Line, scaled(10_000_000) as u32)
            };
            let options = ChartOptions {
                legend: false,
                navigation: !streaming,
                ..Default::default()
            };
            if streaming {
                ChartView::new(streaming_spec(stream.clone()).prepare(), options, cx)
            } else {
                let spec = ChartSpec::new("Observations · عربي 日本語 🎵").layer(Layer::new(
                    mark,
                    (0..count).collect::<Vec<u32>>(),
                    Channels::new(|i: &u32| f64::from(*i), |i| (f64::from(*i) * 0.002).sin())
                        .group(move |i| if scatter { i % 100 } else { 0 }),
                ));
                ChartView::from_spec(spec, options, cx)
            }
        })
    })?;
    // TestAppContext::add_window_view drains the background work before returning.
    let mut window = VisualTestContext::from_window(handle, &cx);
    window.update(|window, cx| window.render_frame(cx));
    let first_frame = mount_started.elapsed();
    println!("first_chart_frame_us={}", first_frame.as_micros());
    window.run_until_parked();
    if !streaming {
        if window.update(|_, cx| view.read(cx).scene_build_count()) < 2 {
            return Err("full chart scene did not publish".into());
        }
        println!(
            "full_chart_scene_us={}",
            mount_started.elapsed().as_micros()
        );
    }
    let stream_started = Instant::now();
    let mut next = STREAM_RATE;
    let mut due = STREAM_RATE;
    run(
        &mut |i| {
            if streaming {
                due = STREAM_RATE
                    + (stream_started.elapsed().as_secs_f64() * f64::from(STREAM_RATE)) as u32;
                if due.saturating_sub(next) >= STREAM_BATCH {
                    let end = due - (due - next) % STREAM_BATCH;
                    stream.append((next..end).collect());
                    stream.retain_last(STREAM_RATE as usize);
                    next = end;
                    let model = streaming_spec(stream.clone()).prepare();
                    window.update(|_, cx| view.update(cx, |chart, cx| chart.replace(model, cx)));
                }
            }
            let bounds = window
                .update(|window, _| window.try_find("chart-canvas").map(|item| item.bounds()))
                .ok_or("missing chart canvas")?;
            let at = bounds.origin
                + point(
                    bounds.size.width * (0.35 + 0.05 * (i % 7) as f32),
                    bounds.size.height * 0.5,
                );
            match i % 4 {
                0 => window.simulate_mouse_move(at, None, Default::default()),
                1 => window.simulate_click(at, Default::default()),
                _ => {
                    let delta = if i % 2 == 0 { 12. } else { -12. };
                    window.simulate_event(ScrollEvent {
                        position: at,
                        delta: ScrollDelta::Pixels(point(px(delta), px(delta))),
                        modifiers: Default::default(),
                        touch_phase: TouchPhase::Moved,
                        source: ScrollSource::Pointer,
                    });
                }
            }
            window.run_until_parked();
            Ok(())
        },
        first_frame,
    );
    if streaming {
        let elapsed = stream_started.elapsed().as_secs_f64();
        let points = next - STREAM_RATE;
        let pending = due.saturating_sub(next);
        println!("stream_points={points} stream_seconds={elapsed:.3} stream_pending={pending}");
        if pending >= STREAM_BATCH {
            return Err("stream failed to sustain 10,000 points per second".into());
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let component = args
        .first()
        .map(String::as_str)
        .ok_or("usage: soak <flow|timeline|chart-line|chart-scatter|chart-stream> [--short] [--no-minimap]")?;
    let short = args.iter().any(|arg| arg == "--short");
    let mount_started = Instant::now();
    let mut result = Ok(());
    match component {
        "flow" => flow(args.iter().any(|arg| arg == "--no-minimap"), |frame| {
            result = sample(frame, mount_started.elapsed(), short);
        })?,
        "timeline" => timeline(|frame| result = sample(frame, mount_started.elapsed(), short)),
        "chart-line" => charts(false, false, |frame, mount| {
            result = sample(frame, mount, short)
        })?,
        "chart-scatter" => charts(true, false, |frame, mount| {
            result = sample(frame, mount, short)
        })?,
        "chart-stream" => charts(false, true, |frame, mount| {
            result = sample(frame, mount, short)
        })?,
        _ => return Err("unknown soak component".into()),
    }
    result
}

fn sample(frame: &mut dyn FnMut(usize) -> Result<()>, mount: Duration, short: bool) -> Result<()> {
    println!("setup_and_first_frame_us={}", mount.as_micros());
    let interval = Duration::from_secs(10);
    let mut index = 0;
    let mut times = Vec::with_capacity(1_200);
    let mut warmed_memory = None;
    // `SOAK_FRAMES=n`: run exactly n frames and stop (profiling; no timing verdict).
    if let Some(frames) = std::env::var("SOAK_FRAMES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        for index in 0..frames {
            frame(index)?;
        }
        println!("profiled_frames={frames}");
        return Ok(());
    }
    let mut failed = !short && mount > Duration::from_millis(200);
    // The first interval warms caches. The following 180 intervals measure drift.
    for sample in 0..if short { 2 } else { 181 } {
        times.clear();
        let start = Instant::now();
        while start.elapsed() < interval {
            let started = Instant::now();
            frame(index)?;
            let elapsed = started.elapsed();
            index += 1;
            times.push(elapsed.as_micros());
            std::thread::sleep(Duration::from_micros(8_334).saturating_sub(elapsed));
        }
        times.sort_unstable();
        let percentile = |p: usize| times[(times.len() * p).div_ceil(100) - 1];
        let (p50, p95, p99, max) = (
            percentile(50),
            percentile(95),
            percentile(99),
            percentile(100),
        );
        let [rss, pss] = memory_kib()?;
        let [base_rss, base_pss] = *warmed_memory.get_or_insert([rss, pss]);
        let drifted = rss * 100 > base_rss * 102 || pss * 100 > base_pss * 102;
        println!(
            "sample={sample} frames={} p50_us={p50} p95_us={p95} p99_us={p99} max_us={max} rss_kib={rss} pss_kib={pss} drifted={drifted}",
            times.len()
        );
        // A 16 ms frame maximum also enforces the 50 ms long-task limit inside measured frames.
        failed |= !short && sample > 0 && (drifted || p99 > 8_000 || max > 16_000);
    }
    if failed {
        return Err("Headless soak exceeded a timing or memory budget".into());
    }
    Ok(())
}
