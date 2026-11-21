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
        .ok_or("usage: soak <flow|timeline|chart-line|chart-scatter|chart-stream|gantt|tracks|curve-editor|bar-editor|flamegraph|tree|property-grid|markdown|mermaid|chat> [--short] [--no-minimap]")?;
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
        "gantt" => mounted::gantt(|frame| result = sample(frame, mount_started.elapsed(), short))?,
        "tracks" => {
            mounted::tracks(|frame| result = sample(frame, mount_started.elapsed(), short))?
        }
        "curve-editor" => {
            mounted::curve_editor(|frame| result = sample(frame, mount_started.elapsed(), short))?
        }
        "bar-editor" => {
            mounted::bar_editor(|frame| result = sample(frame, mount_started.elapsed(), short))?
        }
        "flamegraph" => mounted::flamegraph(mount_started, |frame, mount| {
            result = sample(frame, mount, short)
        })?,
        "tree" => mounted::tree(|frame| result = sample(frame, mount_started.elapsed(), short))?,
        "property-grid" => {
            mounted::property_grid(|frame| result = sample(frame, mount_started.elapsed(), short))?
        }
        "markdown" => {
            mounted::markdown(|frame| result = sample(frame, mount_started.elapsed(), short))?
        }
        "mermaid" => {
            mounted::mermaid(|frame| result = sample(frame, mount_started.elapsed(), short))?
        }
        "chat" => mounted::chat(|frame| result = sample(frame, mount_started.elapsed(), short))?,
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

/// Mounted spec/23 scale probes: each mounts the real component over a large
/// deterministic dataset and performs one realistic interaction per frame.
mod mounted {
    use super::{Instant, Result, scaled};
    use gpui_kit::{
        App, AppContext, Bounds, Context, ElementId, Entity, IntoElement, Modifiers, MouseButton,
        ParentElement, Pixels, Point, Render, ScrollDelta, ScrollEvent, ScrollSource, Styled,
        TestAppContext, TestDispatcher, TouchPhase, VisualTestContext, Window, WindowBounds,
        WindowOptions, div, point, px, size, test::TestWindowExt,
    };
    use std::{rc::Rc, sync::Arc, time::Duration};

    type Frame<'a> = &'a mut dyn FnMut(usize) -> Result<()>;

    /// Simulated time per frame for timer-driven work (stream drains, coalescing, settles).
    const FRAME: Duration = Duration::from_millis(16);
    /// Timeline wheel zoom: `1 + dy * 0.002`; out is the exact reciprocal of in.
    const ZOOM_IN: f32 = 50.;
    const ZOOM_OUT: f32 = (1. / 1.1 - 1.) / 0.002;

    /// Alternate direction every `period` interactions so long soaks stay in range.
    fn sign(i: usize, period: usize) -> f32 {
        if (i / period).is_multiple_of(2) {
            1.
        } else {
            -1.
        }
    }

    /// Open a 1280x800 window (base Root) and render the first frame.
    fn mount<V: Render>(
        name: &'static str,
        init: impl FnOnce(&mut App),
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
    ) -> Result<(VisualTestContext, Entity<V>)> {
        let (mut window, view) = open(name, init, build)?;
        window.run_until_parked();
        window.update(|window, cx| window.render_frame(cx));
        settle(&mut window);
        // Views that measure themselves (axis width, viewport bounds) lay out again.
        window.update(|window, cx| window.render_frame(cx));
        window.run_until_parked();
        Ok((window, view))
    }

    fn open<V: Render>(
        name: &'static str,
        init: impl FnOnce(&mut App),
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
    ) -> Result<(VisualTestContext, Entity<V>)> {
        let cx = TestAppContext::build(TestDispatcher::new(0), Some(name));
        cx.update(|cx| {
            guiraffe_core::init(cx);
            init(cx);
        });
        let (handle, view) = cx.update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1280.), px(800.)),
                ))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, build)
        })?;
        Ok((VisualTestContext::from_window(handle, &cx), view))
    }

    fn settle(window: &mut VisualTestContext) {
        window.run_until_parked();
        window.executor().advance_clock(FRAME);
        window.run_until_parked();
    }

    fn bounds(window: &mut VisualTestContext, id: impl Into<ElementId>) -> Option<Bounds<Pixels>> {
        let id = id.into();
        window.update(|window, _| window.try_find(id).map(|item| item.bounds()))
    }

    fn require(window: &mut VisualTestContext, id: &'static str) -> Result<Bounds<Pixels>> {
        bounds(window, id).ok_or_else(|| format!("missing {id}").into())
    }

    fn wheel(window: &mut VisualTestContext, at: Point<Pixels>, dx: f32, dy: f32, control: bool) {
        window.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            touch_phase: TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
    }

    fn hover(window: &mut VisualTestContext, at: Point<Pixels>) {
        window.simulate_mouse_move(at, None, Modifiers::default());
    }

    fn drag(window: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
        window.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        for step in 1..=4 {
            let at = from + (to - from) * (step as f32 / 4.);
            window.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::default());
        }
        window.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    }

    fn inside(b: Bounds<Pixels>, p: Point<Pixels>) -> bool {
        b.contains(&p)
    }

    // ---------------------------------------------------------------- timeline

    /// Gantt: 100,000 bars in chains of ten finish→start dependencies.
    /// The view lays out every row (no vertical virtualization or scrolling);
    /// vertical wheel input pans its time axis like horizontal input.
    pub fn gantt(run: impl FnOnce(Frame)) -> Result<()> {
        use guiraffe_timeline::{GanttView, Task, TimeFormat, gantt_view::RULER_H};
        let count = scaled(100_000) as u32;
        let tasks = (0..count)
            .map(|id| Task {
                id,
                name: format!("Task {id}"),
                not_before: f64::from((id / 10) % 100) * 3.,
                duration: 1. + f64::from(id % 3),
                after: if id % 10 == 0 { vec![] } else { vec![id - 1] },
            })
            .collect();
        let (mut window, _view) = mount("gantt_soak", guiraffe_timeline::init, |_, cx| {
            GanttView::new(tasks, TimeFormat::Seconds, cx)
        })?;
        let root = require(&mut window, "gantt")?;
        let body = root.origin + point(root.size.width * 0.6, px(RULER_H + 120.));
        run(&mut |i| {
            let d = 30. * sign(i, 64);
            match i % 4 {
                0 => wheel(&mut window, body, d, 0., false),
                1 => wheel(&mut window, body, 0., d, false),
                _ => {
                    let row = (i / 4) % 16;
                    let at = bounds(&mut window, ("gantt-bar", row))
                        .map(|b| b.center())
                        .filter(|p| inside(root, *p))
                        .unwrap_or(body);
                    hover(&mut window, at);
                }
            }
            window.run_until_parked();
            Ok(())
        });
        Ok(())
    }

    /// Tracks: 100 tracks of 1,000 clips (100,000 clips), fitted to the full length.
    pub fn tracks(run: impl FnOnce(Frame)) -> Result<()> {
        use guiraffe_timeline::{TimeFormat, Track, TracksView, tracks_view::clip_element_id};
        let per_track = scaled(1_000);
        let tracks = (0..100)
            .map(|t| {
                let mut track = Track::default();
                for k in 0..per_track {
                    let start = k as f64 * 2. + (t % 4) as f64 * 0.25;
                    track.insert(start, 1.5);
                }
                (format!("Track {t}"), track)
            })
            .collect();
        let length = per_track as f64 * 2. + 1.;
        let (mut window, view) = mount("tracks_soak", guiraffe_timeline::init, |_, cx| {
            TracksView::new(tracks, TimeFormat::Seconds, length, cx)
        })?;
        let root = require(&mut window, "tracks")?;
        let body = root.origin + point(root.size.width * 0.55, root.size.height * 0.4);
        run(&mut |i| {
            match i % 4 {
                0 => wheel(&mut window, body, 30. * sign(i, 64), 0., false),
                1 => {
                    let dy = if sign(i, 32) > 0. { ZOOM_IN } else { ZOOM_OUT };
                    wheel(&mut window, body, 0., dy, true)
                }
                2 => {
                    let track = (i / 4) % 16;
                    let clip = window.update(|_, cx| {
                        let v = view.read(cx);
                        let axis = v.axis();
                        let mid = axis.start + axis.width * axis.scale * 0.5;
                        let clips = v.tracks().get(track)?.1.clips();
                        clips
                            .get(clips.partition_point(|c| c.start < mid))
                            .map(|c| c.id)
                    });
                    let at = clip
                        .and_then(|id| bounds(&mut window, clip_element_id(track, id)))
                        .map(|b| b.center())
                        .filter(|p| inside(root, *p))
                        .unwrap_or(body);
                    window.simulate_click(at, Modifiers::default());
                }
                _ => hover(
                    &mut window,
                    body + point(px(((i * 37) % 400) as f32), px(((i * 13) % 300) as f32)),
                ),
            }
            window.run_until_parked();
            Ok(())
        });
        Ok(())
    }

    /// Curve editor: one bezier/linear curve of 100,000 keys, 1,000 keys in view.
    /// The editor has no hover state; hover frames measure pointer dispatch only.
    pub fn curve_editor(run: impl FnOnce(Frame)) -> Result<()> {
        use guiraffe_timeline::{Curve, CurveView, Interp, Key};
        let mut curve = Curve::default();
        for k in 0..scaled(100_000) {
            let t = k as f64;
            let interp = if k % 3 == 0 {
                Interp::Linear
            } else {
                Interp::Bezier
            };
            let mut key = Key::new(t, (t * 0.05).sin() + 0.3 * (t * 0.31).cos(), interp);
            key.in_handle = (-0.3, 0.);
            key.out_handle = (0.3, 0.);
            curve.set(key);
        }
        let (mut window, view) = mount("curve_soak", guiraffe_timeline::init, |_, cx| {
            CurveView::new(curve, 0., 1_000., -1.5, 1.5, cx)
        })?;
        let body = require(&mut window, "curve-body")?;
        let center = body.center();
        run(&mut |i| {
            match i % 4 {
                0 => wheel(&mut window, center, 30. * sign(i, 64), 0., false),
                1 => {
                    let dy = if sign(i, 32) > 0. { ZOOM_IN } else { ZOOM_OUT };
                    wheel(&mut window, center, 0., dy, true)
                }
                2 => hover(
                    &mut window,
                    body.origin + point(px(((i * 37) % 1000) as f32), px(((i * 13) % 600) as f32)),
                ),
                _ => {
                    let at = window.update(|_, cx| {
                        let v = view.read(cx);
                        let (axis, range) = (v.axis(), v.range());
                        let keys = v.curve().keys();
                        let t =
                            axis.start + axis.width * axis.scale * (0.2 + 0.05 * (i % 12) as f64);
                        keys.get(keys.partition_point(|k| k.t < t)).map(|k| {
                            body.origin + point(px(axis.x(k.t) as f32), px(range.y(k.v) as f32))
                        })
                    });
                    window.simulate_click(at.unwrap_or(center), Modifiers::default());
                }
            }
            window.run_until_parked();
            Ok(())
        });
        Ok(())
    }

    /// Bar curve editor: 100,000 bars, always fitted. The view has no scroll,
    /// zoom or hover handling, so frames alternate hover (dispatch only) with
    /// drawn strokes, its real editing interaction.
    pub fn bar_editor(run: impl FnOnce(Frame)) -> Result<()> {
        use guiraffe_timeline::{BarCurve, BarCurveView};
        let count = scaled(100_000);
        let mut model = BarCurve::new(count, 0., 1.);
        for k in 0..count {
            model.set(k, 0.5 + 0.45 * (k as f64 * 0.003).sin());
        }
        let (mut window, _view) = mount("bar_soak", guiraffe_timeline::init, |_, cx| {
            BarCurveView::new(model, cx)
        })?;
        let body = require(&mut window, "bar-curve")?;
        run(&mut |i| {
            let x =
                body.origin.x + body.size.width * (0.05 + 0.9 * ((i * 37) % 1000) as f32 / 1000.);
            let y = body.origin.y + body.size.height * (0.2 + 0.6 * ((i * 13) % 100) as f32 / 100.);
            if i % 4 == 3 {
                drag(
                    &mut window,
                    point(x, y),
                    point(x + px(40.), body.origin.y + body.size.height * 0.5),
                );
            } else {
                hover(&mut window, point(x, y));
            }
            window.run_until_parked();
            Ok(())
        });
        Ok(())
    }

    // -------------------------------------------------------------- flamegraph

    /// FlameGraph: 10,000,000 frames as 20,000 stacks of depth 500 (mirrors
    /// the `flamegraph_ten_million` bench source).
    pub fn flamegraph(started: Instant, run: impl FnOnce(Frame, Duration)) -> Result<()> {
        use guiraffe_flamegraph::{FlameGraph, Frame as Sample, FrameSource};
        const LABELS: [&str; 8] = [
            "main", "run", "dispatch", "render", "layout", "paint", "alloc", "hash",
        ];
        struct Profile(usize);
        impl FrameSource for Profile {
            fn len(&self) -> usize {
                self.0
            }
            fn frame(&self, index: usize) -> Sample<'_> {
                let depth = index % 500;
                Sample {
                    id: index as u64,
                    parent: (depth > 0).then(|| index as u64 - 1),
                    self_value: if depth == 499 { 1. } else { 0. },
                    total: 1.,
                    label: LABELS[(depth + index / 500) % LABELS.len()],
                    package: "bench",
                }
            }
        }
        let frames = scaled(10_000_000).div_ceil(500) * 500;
        let (mut window, view) = open("flamegraph_soak", guiraffe_flamegraph::init, |_, cx| {
            cx.new(|cx| {
                FlameGraph::from_source(Arc::new(Profile(frames)), cx)
                    .unwrap_or_else(|error| panic!("invalid soak profile: {error:?}"))
            })
        })?;
        // The deterministic test dispatcher runs its background queue when
        // parked. Draw and verify the preview before draining that queue.
        window.update(|window, cx| window.render_frame(cx));
        require(&mut window, "flame-plot")?;
        let mount = started.elapsed();
        println!(
            "preview_pending={}",
            view.read_with(&window, |view, _| view.is_loading())
        );
        settle(&mut window);
        window.update(|window, cx| window.render_frame(cx));
        if let Some(error) = view.read_with(&window, |view, _| view.load_failure()) {
            return Err(format!("invalid soak profile: {error:?}").into());
        }
        println!("setup_and_full_layout_us={}", started.elapsed().as_micros());
        let plot = require(&mut window, "flame-plot")?;
        run(
            &mut |i| {
                let at = point(
                    plot.origin.x + plot.size.width * (0.1 + 0.8 * ((i * 37) % 100) as f32 / 100.),
                    plot.origin.y + plot.size.height - px(10. + 21. * ((i * 7) % 30) as f32),
                );
                match i % 8 {
                    0 | 1 => wheel(&mut window, at, 0., 35., false),
                    2 | 3 => wheel(&mut window, at, 0., -35., false),
                    6 => drag(&mut window, at, at + point(px(22. * sign(i, 16)), px(0.))),
                    _ => hover(&mut window, at),
                }
                window.run_until_parked();
                Ok(())
            },
            mount,
        );
        Ok(())
    }

    // -------------------------------------------------------------------- tree

    const NAMES: [&str; 8] = [
        "src",
        "lib.rs",
        "assets",
        "main.rs",
        "README.md",
        "tests",
        "Cargo.toml",
        "build.rs",
    ];

    /// Root 0 → `branches` → nine leaves each (1,000,000 nodes at full scale).
    struct Forest {
        branches: Vec<u64>,
        leaves: Vec<u64>,
    }
    impl Forest {
        fn new(branches: usize, per: usize) -> Self {
            let b = branches as u64;
            Self {
                branches: (1..=b).collect(),
                leaves: (b + 1..b + 1 + b * per as u64).collect(),
            }
        }
        fn kids(&self, id: u64, per: usize) -> &[u64] {
            if id == 0 {
                return &self.branches;
            }
            if id as usize <= self.branches.len() {
                let begin = (id as usize - 1) * per;
                return self.leaves.get(begin..begin + per).unwrap_or(&[]);
            }
            &[]
        }
    }

    struct Files(Forest);
    impl guiraffe_tree::TreeSource for Files {
        fn roots(&self) -> &[u64] {
            &[0]
        }
        fn children(&self, id: u64) -> guiraffe_tree::Children<'_> {
            guiraffe_tree::Children::Ready(self.0.kids(id, 9))
        }
        fn label(&self, id: u64) -> &str {
            NAMES[id as usize % NAMES.len()]
        }
    }
    impl guiraffe_tree::TreeData for Files {
        fn badge(&self, id: u64) -> Option<&str> {
            (id != 0 && id as usize <= self.0.branches.len()).then_some("9")
        }
    }

    /// Expand every branch while the root is collapsed (cheap), then the root.
    fn expand_all<const P: bool>(
        view: &mut guiraffe_tree::TreeView<P>,
        branches: usize,
        cx: &mut Context<guiraffe_tree::TreeView<P>>,
    ) {
        for id in 1..=branches as u64 {
            view.set_expanded(id, true, cx);
        }
        view.set_expanded(0, true, cx);
    }

    /// Scroll, select by click, hover, and collapse/expand of one branch.
    fn tree_frames<const P: bool>(
        window: &mut VisualTestContext,
        view: &Entity<guiraffe_tree::TreeView<P>>,
        branches: usize,
        i: usize,
    ) {
        // The tree is a fixed 288px tall at the top of the window, rows of 36px.
        let row = point(px(110.), px(18. + 36. * ((i / 4) % 7) as f32));
        match i % 4 {
            0 => wheel(window, row, 0., -480. * sign(i, 512), false),
            1 => window.simulate_click(row, Modifiers::default()),
            2 => {
                let id = 1 + ((i / 8) % branches) as u64;
                let open = i % 8 != 2;
                window.update(|_, cx| view.update(cx, |v, cx| v.set_expanded(id, open, cx)));
            }
            _ => hover(window, row + point(px(40.), px(0.))),
        }
        window.run_until_parked();
    }

    /// TreeList: 1,000,000 nodes with all 100,000 branches expanded.
    pub fn tree(run: impl FnOnce(Frame)) -> Result<()> {
        let branches = scaled(100_000);
        let source = Rc::new(Files(Forest::new(branches, 9)));
        let (mut window, view) = mount(
            "tree_soak",
            |_| {},
            |_, cx| {
                let view = cx.new(|cx| guiraffe_tree::TreeList::new(source, cx));
                view.update(cx, |v, cx| expand_all(v, branches, cx));
                view
            },
        )?;
        run(&mut |i| {
            tree_frames(&mut window, &view, branches, i);
            Ok(())
        });
        Ok(())
    }

    const PROPERTIES_PER_GROUP: usize = 9;

    struct Settings {
        groups: Forest,
        values: [guiraffe_tree::Value; 4],
        defaults: [guiraffe_tree::Value; 4],
        choices: Vec<String>,
    }
    impl guiraffe_tree::TreeSource for Settings {
        fn roots(&self) -> &[u64] {
            &[0]
        }
        fn children(&self, id: u64) -> guiraffe_tree::Children<'_> {
            guiraffe_tree::Children::Ready(self.groups.kids(id, PROPERTIES_PER_GROUP))
        }
        fn label(&self, id: u64) -> &str {
            if id == 0 {
                "Settings"
            } else if id as usize <= self.groups.branches.len() {
                "Group"
            } else {
                ["Opacity", "Visible", "Name", "Blend", "Width"][id as usize % 5]
            }
        }
    }
    impl guiraffe_tree::TreeData for Settings {
        fn property(&self, id: u64) -> Option<guiraffe_tree::Property<'_>> {
            let kind = id as usize % 4;
            (id as usize > self.groups.branches.len()).then(|| guiraffe_tree::Property {
                value: &self.values[kind],
                default: if id.is_multiple_of(5) {
                    &self.defaults[kind]
                } else {
                    &self.values[kind]
                },
                choices: &self.choices,
                read_only: id.is_multiple_of(7),
            })
        }
    }

    /// PropertyGrid: 100,000 groups of nine properties (1,000,001 rows) all expanded.
    pub fn property_grid(run: impl FnOnce(Frame)) -> Result<()> {
        use guiraffe_tree::Value;
        let groups = scaled(100_000);
        println!(
            "property_groups={groups} visible_rows={}",
            1 + groups * (1 + PROPERTIES_PER_GROUP)
        );
        let source = Rc::new(Settings {
            groups: Forest::new(groups, PROPERTIES_PER_GROUP),
            values: [
                Value::Number(0.75),
                Value::Bool(true),
                Value::Text("Layer".into()),
                Value::Enum("Normal".into()),
            ],
            defaults: [
                Value::Number(1.),
                Value::Bool(false),
                Value::Text(String::new()),
                Value::Enum("Multiply".into()),
            ],
            choices: vec!["Normal".into(), "Multiply".into(), "Screen".into()],
        });
        let (mut window, view) = mount(
            "property_grid_soak",
            |_| {},
            |_, cx| {
                let view = cx.new(|cx| guiraffe_tree::PropertyGrid::new(source, cx));
                view.update(cx, |v, cx| expand_all(v, groups, cx));
                view
            },
        )?;
        run(&mut |i| {
            tree_frames(&mut window, &view, groups, i);
            Ok(())
        });
        Ok(())
    }

    // ---------------------------------------------------------------- markdown

    /// A deterministic GFM document of at least `lines` lines.
    fn document(lines: usize) -> String {
        let mut out = String::new();
        let mut count = 0;
        let mut block = 0;
        while count < lines {
            let n = block;
            let text = match block % 10 {
                0 if block % 100 == 0 => format!("# Chapter {}\n\n", n / 100),
                0 => format!("## Section {n}\n\n"),
                3 => format!(
                    "- item {n} with **bold** text\n- item {n}b with `inline code`\n- [ ] task {n}\n- [x] done {n}\n\n"
                ),
                6 => format!("```rust\nfn section_{n}() -> u64 {{\n    {n} * 2\n}}\n```\n\n"),
                9 => format!(
                    "| key | value | note |\n|---|---|---|\n| {n} | alpha | first |\n| {n}b | beta | second |\n| {n}c | gamma | third |\n\n"
                ),
                _ => format!(
                    "Paragraph {n} opens with *emphasis* and a [link](https://example.org/{n}).\nIt continues with ordinary prose about block {n} so wrapping matters,\nthen a second sentence that mentions `code_{n}` and **strong words**.\nThe block ends here.\n\n"
                ),
            };
            count += text.lines().count();
            out.push_str(&text);
            block += 1;
        }
        out
    }

    struct MarkdownHost(Entity<guiraffe_markdown::MarkdownState>);
    impl Render for MarkdownHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                guiraffe_markdown::MarkdownView::new("soak")
                    .state(&self.0)
                    .scrollable(true),
            )
        }
    }

    /// Markdown: a 50,000-line document, scrolled, with a streamed chunk
    /// appended every fourth frame.
    pub fn markdown(run: impl FnOnce(Frame)) -> Result<()> {
        let source = document(scaled(50_000));
        let (mut window, view) = mount(
            "markdown_soak",
            |_| {},
            |_, cx| {
                let state = cx.new(|cx| guiraffe_markdown::MarkdownState::new(source, cx));
                cx.new(|_| MarkdownHost(state))
            },
        )?;
        let state = window.update(|_, cx| view.read(cx).0.clone());
        let center = point(px(640.), px(400.));
        run(&mut |i| {
            if i % 4 == 0 {
                let chunk = match (i / 4) % 64 {
                    63 => format!("\n\n## Streamed {i}\n\n"),
                    k if k % 16 == 15 => "\n\n".to_string(),
                    _ => format!("token{i} "),
                };
                window.update(|_, cx| state.update(cx, |s, cx| s.append(&chunk, cx)));
            } else {
                wheel(&mut window, center, 0., -120. * sign(i, 256), false);
            }
            settle(&mut window);
            Ok(())
        });
        Ok(())
    }

    // ----------------------------------------------------------------- mermaid

    struct MermaidHost(Entity<guiraffe_mermaid::MermaidViewState>);
    impl Render for MermaidHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(guiraffe_mermaid::MermaidView::new("diagram").state(&self.0))
        }
    }

    /// Mermaid: a ~500-node flowchart (tree plus cross links), zoomed and panned.
    /// Each 32-frame cycle zooms, then pans, then idles so the zoom settle
    /// re-rasterizes, as when a reader pauses.
    pub fn mermaid(run: impl FnOnce(Frame)) -> Result<()> {
        let nodes = scaled(500).max(2);
        let mut source = String::from("flowchart TD\n");
        for i in 0..nodes {
            source.push_str(&format!("  n{i}[Node {i}]\n"));
        }
        for i in 1..nodes {
            source.push_str(&format!("  n{} --> n{i}\n", (i - 1) / 3));
            if i % 10 == 0 {
                source.push_str(&format!("  n{i} -.-> n{}\n", (i * 7) % nodes));
            }
        }
        let (mut window, view) = mount(
            "mermaid_soak",
            |_| {},
            |_, cx| {
                let state = guiraffe_mermaid::MermaidView::create_state("diagram", cx);
                state.update(cx, |s, cx| {
                    let dark = guiraffe_core::GuiraffeTheme::global(cx).scheme
                        == guiraffe_core::Scheme::Dark;
                    s.set_source(source.into(), dark, cx)
                });
                cx.new(|_| MermaidHost(state))
            },
        )?;
        for _ in 0..4 {
            settle(&mut window);
        }
        let state = window.update(|_, cx| view.read(cx).0.clone());
        if window.update(|_, cx| state.read(cx).image().is_none()) {
            return Err("mermaid diagram did not render".into());
        }
        let center = point(px(640.), px(380.));
        run(&mut |i| {
            let at = center
                + point(
                    px(((i * 37) % 200) as f32 - 100.),
                    px(((i * 13) % 160) as f32 - 80.),
                );
            match i % 32 {
                0..4 => wheel(&mut window, at, 0., 40. * sign(i, 32), false),
                8..16 => {
                    let d = 20. * sign(i, 16);
                    drag(&mut window, at, at + point(px(d), px(d * 0.5)));
                }
                _ => hover(&mut window, at),
            }
            settle(&mut window);
            Ok(())
        });
        Ok(())
    }

    // -------------------------------------------------------------------- chat

    struct ChatHost(Entity<guiraffe_chat::ChatState>);
    impl Render for ChatHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            guiraffe_chat::Chat::new(&self.0)
        }
    }

    /// Chat: 100,000-message history, scrolled while a reply streams one token
    /// per frame; each reply finishes after 400 tokens and the next begins.
    /// Reduced motion, as in the crate's own interaction tests.
    pub fn chat(run: impl FnOnce(Frame)) -> Result<()> {
        use guiraffe_chat::{ChatState, Message, MessageId, Role};
        let count = scaled(100_000) as u64;
        let (mut window, view) = mount(
            "chat_soak",
            |cx| {
                guiraffe_chat::init(cx);
                guiraffe_motion::MotionPrefs::set(
                    guiraffe_motion::MotionPrefs {
                        reduced: true,
                        ..guiraffe_motion::MotionPrefs::global(cx)
                    },
                    cx,
                );
            },
            |window, cx| {
                let state = cx.new(|cx| ChatState::new(window, cx));
                state.update(cx, |chat, cx| {
                    for n in 0..count {
                        let (role, text) = if n % 2 == 0 {
                            (Role::User, format!("Question {n}: how should the **cache** behave when entry {n} is evicted?"))
                        } else if n % 10 == 1 {
                            (Role::Assistant, format!("Here is answer {n}.\n\n- keep the *hot* set\n- drop the cold tail\n\n```rust\nlet entry = cache.get({n});\n```\n"))
                        } else {
                            (Role::Assistant, format!("Answer {n}: evict the least recently used entry and keep `stats` updated. See [docs](https://example.org/{n})."))
                        };
                        chat.push_message(Message::new(MessageId(n + 1), role, text), cx);
                    }
                });
                cx.new(|_| ChatHost(state))
            },
        )?;
        let state = window.update(|_, cx| view.read(cx).0.clone());
        let history = require(&mut window, "chat-history")?;
        let mut producer = None;
        let mut tokens = 0usize;
        run(&mut |i| {
            let reply = producer.get_or_insert_with(|| {
                tokens = 0;
                window.update(|_, cx| state.update(cx, |chat, cx| chat.begin_reply(cx).1))
            });
            let token = match tokens % 48 {
                47 => "\n\n".to_string(),
                _ => format!("token{tokens} "),
            };
            if reply.push(token).is_ok() {
                tokens += 1;
            }
            if tokens >= 400 {
                producer = None;
            }
            if i % 2 == 0 {
                let at = history.center() + point(px(0.), px(((i * 13) % 200) as f32 - 100.));
                wheel(&mut window, at, 0., 120. * sign(i, 256), false);
            }
            settle(&mut window);
            Ok(())
        });
        Ok(())
    }
}
