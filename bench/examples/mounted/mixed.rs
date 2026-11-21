use super::*;
use crate::{flow_graph, memory_kib, streaming_spec};
use gpui_kit::AnyView;
use guiraffe_charts::{ChartOptions, ChartSource, ChartView, Stream};
use guiraffe_chat::{ChatState, Message, MessageId, Role};
use guiraffe_code::{CodeDocument, CodeHandle, CodeViewer, Position, SourceRange};
use guiraffe_datagrid::{CellPos, CellRef, ColumnDef, DataGrid, FnSource, GridHandle};
use guiraffe_flow::{
    Flow,
    core::{Point as FlowPoint, Viewport},
};
use guiraffe_overlays::{
    OverlayHost,
    stack::Policy,
    toast::{Toast, ToastId},
};
use guiraffe_timeline::{Channel, DopeSheet, GanttView, TimeFormat};

#[derive(Clone)]
enum Data {
    Grid(GridHandle),
    Code(CodeHandle),
}

impl Render for Data {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Grid(grid) => DataGrid::new(grid).into_any_element(),
            Self::Code(code) => CodeViewer::new(code).into_any_element(),
        }
    }
}

impl Data {
    fn new(component: &str, count: usize, cx: &mut App) -> Self {
        if component == "grid" {
            Self::Grid(GridHandle::new(
                FnSource::new(count, |row, col| CellRef::Int((row * 100 + col) as i64)),
                (0..50)
                    .map(|col| ColumnDef::new(format!("c{col}"), format!("Column {col}"), col))
                    .collect(),
                cx,
            ))
        } else {
            Self::Code(CodeHandle::new(
                1,
                Arc::new(CodeDocument::new(Arc::<str>::from(
                    "let value = read();\n".repeat(count - 1) + "let value = read();",
                ))),
                cx,
            ))
        }
    }

    fn step(&self, count: usize, i: usize, cx: &mut App) {
        let row = match i % 3 {
            0 => i % count.min(96),
            1 => count / 2,
            _ => count - 1,
        };
        match self {
            Self::Grid(grid) => {
                grid.select(CellPos { row, col: i % 6 }, false, false, cx);
                grid.scroll_to_row(row, cx);
            }
            Self::Code(code) => {
                code.select(
                    Some(SourceRange::new(
                        Position::new(row, 0),
                        Position::new(row, 3),
                    )),
                    cx,
                );
                code.scroll_to_line(row, cx);
            }
        }
    }
}

pub fn run_data(
    component: &str,
    started: Instant,
    run: impl FnOnce(Frame, Duration),
) -> Result<()> {
    let count = scaled(1_000_000);
    let (mut window, view) = mount(
        "data_soak",
        |_| {},
        |_, cx| {
            let data = Data::new(component, count, cx);
            cx.new(|_| data)
        },
    )?;
    let elapsed = started.elapsed();
    let data = window.update(|_, cx| view.read(cx).clone());
    run(
        &mut |i| {
            window.update(|_, cx| data.step(count, i, cx));
            settle(&mut window);
            Ok(())
        },
        elapsed,
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum Phase {
    Grid,
    Chart,
    Flow,
    Gantt,
    Timeline,
    Markdown,
    Chat,
    Tree,
    PropertyGrid,
    Code,
    Overlays,
    Theme,
}

const PHASES: [(Phase, &str); 12] = [
    (Phase::Grid, "grid"),
    (Phase::Chart, "chart-stream"),
    (Phase::Flow, "flow"),
    (Phase::Gantt, "gantt"),
    (Phase::Timeline, "timeline"),
    (Phase::Markdown, "markdown"),
    (Phase::Chat, "chat"),
    (Phase::Tree, "tree"),
    (Phase::PropertyGrid, "property-grid"),
    (Phase::Code, "code"),
    (Phase::Overlays, "overlays"),
    (Phase::Theme, "theme"),
];
const STEPS_PER_PHASE: usize = 8;
const RATE: u32 = 10_000;

struct Tabs {
    views: Vec<AnyView>,
    active: Phase,
}

impl Render for Tabs {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.views[(self.active as usize).min(Phase::Code as usize)].clone())
    }
}

pub fn run(short: bool) -> Result<()> {
    let flow_nodes = scaled(5_000);
    let graph = flow_graph(flow_nodes as u64)?;
    let gantt = gantt_tasks(scaled(10_000) as u32, GanttGraph::Sparse);
    let branches = scaled(10_000);
    let groups = scaled(1_000);
    let rows = scaled(1_000_000);
    let markdown_seed: gpui_kit::SharedString = document(scaled(5_000)).into();
    let chat_seed = scaled(1_000);
    let mut stream = Stream::default();
    stream.append((0..RATE).collect());
    let mut state = None;
    let (mut window, tabs) = open(
        "mixed_soak",
        |cx| {
            guiraffe_overlays::init(cx);
            guiraffe_timeline::init(cx);
            guiraffe_charts::init(cx);
            guiraffe_chat::init(cx);
        },
        |window, cx| {
            let grid = Data::new("grid", rows, cx);
            let code = Data::new("code", rows, cx);
            let chart = ChartView::new(
                streaming_spec(stream.clone()).prepare(),
                ChartOptions {
                    legend: false,
                    ..Default::default()
                },
                cx,
            );
            let flow = cx.new(|cx| Flow::new(graph, cx).expect("valid fixture graph"));
            let gantt = GanttView::new(gantt, TimeFormat::Seconds, cx);
            let timeline = {
                DopeSheet::new(
                    (0..100)
                        .map(|i| {
                            Channel::new(
                                format!("Lane {i}"),
                                (0..scaled(1_000)).map(|k| k as f64).collect(),
                            )
                        })
                        .collect(),
                    TimeFormat::Frames(24.),
                    1_000.,
                    cx,
                )
            };
            let markdown =
                cx.new(|cx| guiraffe_markdown::MarkdownState::new(markdown_seed.clone(), cx));
            let chat = cx.new(|cx| {
                let mut chat = ChatState::new(window, cx);
                for i in 0..chat_seed {
                    chat.push_message(
                        Message::new(
                            MessageId(i as u64 + 1),
                            if i % 2 == 0 {
                                Role::User
                            } else {
                                Role::Assistant
                            },
                            format!("Message {i} with **emphasis** and `inline code`."),
                        ),
                        cx,
                    );
                }
                chat
            });
            let tree = cx.new(|cx| {
                guiraffe_tree::TreeList::new(Rc::new(Files(Forest::new(branches, 9))), cx)
            });
            tree.update(cx, |tree, cx| expand_all(tree, branches, cx));
            let property = cx.new(|cx| guiraffe_tree::PropertyGrid::new(settings(groups), cx));
            property.update(cx, |property, cx| expand_all(property, groups, cx));
            let views = vec![
                cx.new(|_| grid.clone()).into(),
                chart.clone().into(),
                flow.clone().into(),
                gantt.into(),
                timeline.into(),
                cx.new(|_| MarkdownHost(markdown.clone())).into(),
                cx.new(|_| ChatHost(chat.clone())).into(),
                tree.clone().into(),
                property.clone().into(),
                cx.new(|_| code.clone()).into(),
            ];
            state = Some((grid, code, chart, flow, markdown, chat, tree, property));
            cx.new(|_| Tabs {
                views,
                active: Phase::Grid,
            })
        },
    )?;
    let (grid, code, chart, flow, markdown, chat, tree, property) =
        state.ok_or("missing mixed state")?;
    window.update(|w, cx| w.render_frame(cx));
    settle(&mut window);

    let mut input = 0usize;
    let mut next = RATE;
    let mut markdown_chunks = 0usize;
    let mut reply = None;
    let mut tokens = 0usize;
    let mut chat_stream_bytes = 0usize;
    let mut overlay = None;
    let mut baseline = None;
    let mut failed = false;
    let clock = Instant::now();
    let mut times = std::array::from_fn::<_, 12, _>(|_| Vec::with_capacity(1_200));
    let profile_frames = std::env::var("SOAK_FRAMES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    for sample in 0..if profile_frames.is_some() {
        1
    } else if short {
        2
    } else {
        180
    } {
        let deadline = clock + Duration::from_secs(10 * (sample + 1));
        while profile_frames.map_or_else(|| Instant::now() < deadline, |limit| input < limit) {
            let started = Instant::now();
            let active = PHASES[(input / STEPS_PER_PHASE) % PHASES.len()].0;
            let cycle = input / (STEPS_PER_PHASE * PHASES.len());
            let step = cycle * STEPS_PER_PHASE + input % STEPS_PER_PHASE;
            window.update(|w, cx| -> Result<()> {
                // Test apps draw dirty windows when they flush effects, but
                // have no platform frame loop to deliver animation callbacks.
                w.simulate_next_frame(cx);
                if input.is_multiple_of(STEPS_PER_PHASE) {
                    tabs.update(cx, |tabs, cx| {
                        tabs.active = active;
                        cx.notify();
                    });
                }
                let due = RATE + (clock.elapsed().as_secs_f64() * f64::from(RATE)) as u32;
                if due - next >= 500 {
                    stream.append((next..due).collect());
                    stream.retain_last(RATE as usize);
                    next = due;
                    chart.update(cx, |chart, cx| {
                        chart.replace_spec(streaming_spec(stream.clone()), cx)
                    });
                }
                match active {
                    Phase::Grid => grid.step(rows, step, cx),
                    Phase::Flow => flow.update(cx, |flow, cx| {
                        flow.set_viewport(
                            Viewport {
                                pan: FlowPoint::new(
                                    -((step % flow_nodes.min(41)) as f32) * 120.,
                                    -((step % flow_nodes.div_ceil(100).min(23)) as f32) * 120.,
                                ),
                                zoom: if step % 16 < 8 { 1. } else { 1.25 },
                            },
                            cx,
                        )
                    })?,
                    Phase::Markdown if step.is_multiple_of(2) => {
                        markdown.update(cx, |markdown, cx| {
                            if markdown_chunks == 256 {
                                markdown.set_source(markdown_seed.clone(), cx);
                                markdown_chunks = 0;
                            }
                            markdown.append(
                                if markdown_chunks % 16 == 15 {
                                    "\n\n"
                                } else {
                                    "token "
                                },
                                cx,
                            );
                            markdown_chunks += 1;
                        })
                    }
                    Phase::Chat => {
                        let producer = reply.get_or_insert_with(|| {
                            chat.update(cx, |chat, cx| chat.begin_reply(cx).1)
                        });
                        if producer.push("streamed token ".into()).is_ok() {
                            tokens += 1;
                            chat_stream_bytes += "streamed token ".len();
                        }
                        // A conversation grows by design. Report its logical payload with RSS.
                        if tokens == 128 {
                            reply = None;
                            tokens = 0;
                        }
                    }
                    Phase::Tree if step.is_multiple_of(4) => tree.update(cx, |tree, cx| {
                        tree.set_expanded(1, step % 8 == 4, cx);
                    }),
                    Phase::PropertyGrid if step.is_multiple_of(4) => {
                        property.update(cx, |property, cx| {
                            property.set_expanded(1, step % 8 == 4, cx);
                        })
                    }
                    Phase::Code => code.step(rows, step, cx),
                    Phase::Overlays if step.is_multiple_of(2) => {
                        if let Some(id) = overlay.take() {
                            OverlayHost::close(w, cx, id)?;
                            OverlayHost::dismiss_toast(w, cx, ToastId(1))?;
                        } else {
                            overlay = Some(OverlayHost::open(
                                w,
                                cx,
                                Policy::default(),
                                |dialog, _, _| dialog.child("Mixed soak"),
                            )?);
                            OverlayHost::notify(
                                w,
                                cx,
                                Toast::new(ToastId(1), "Background work completed"),
                            )?;
                        }
                    }
                    Phase::Theme if step.is_multiple_of(4) => guiraffe_core::set_scheme(
                        if step.is_multiple_of(8) {
                            guiraffe_core::Scheme::Dark
                        } else {
                            guiraffe_core::Scheme::Light
                        },
                        cx,
                    ),
                    _ => {}
                }
                Ok(())
            })?;
            if matches!(
                active,
                Phase::Chart
                    | Phase::Flow
                    | Phase::Gantt
                    | Phase::Timeline
                    | Phase::Markdown
                    | Phase::Chat
                    | Phase::Tree
                    | Phase::PropertyGrid
            ) {
                let id = match active {
                    Phase::Chart => Some("chart-canvas"),
                    Phase::Gantt => Some("gantt"),
                    Phase::Timeline => Some("dope-body"),
                    Phase::Chat => Some("chat-history"),
                    _ => None,
                };
                let at = match id {
                    Some(id) => require(&mut window, id)?.center(),
                    None => point(px(500.), px(180.)),
                };
                match step % 4 {
                    0 => hover(&mut window, at),
                    1 => window.simulate_click(at, Modifiers::default()),
                    _ => wheel(&mut window, at, 0., 32. * sign(step, 8), false),
                }
            }
            settle(&mut window);
            let elapsed = started.elapsed();
            failed |= elapsed > Duration::from_millis(50);
            times[active as usize].push(elapsed.as_micros());
            input += 1;
            if profile_frames.is_none() {
                std::thread::sleep(Duration::from_micros(8_334).saturating_sub(elapsed));
            }
        }
        let retained = stream.len();
        let [rss, pss] = memory_kib()?;
        let base = *baseline.get_or_insert(rss);
        let drift = rss * 100 > base * 102;
        failed |= drift;
        let (markdown_bytes, chat_messages) = window.update(|_, cx| {
            (
                markdown.read(cx).source().len(),
                chat.read(cx).message_count(),
            )
        });
        let Data::Grid(grid) = &grid else {
            return Err("missing grid handle".into());
        };
        let Data::Code(code) = &code else {
            return Err("missing code handle".into());
        };
        let grid_pool = grid.stats().pool;
        let code_visible = code.stats().last_visible_rows;
        let (modals, toasts, toast_queue) = window.update(|w, cx| {
            let host = OverlayHost::entity(w, cx)?;
            let host = host.read(cx);
            Ok::<_, guiraffe_overlays::OverlayError>((
                host.stack().entries().len(),
                host.toasts().visible().len(),
                host.toasts().queued().len(),
            ))
        })?;
        println!(
            "sample={sample} rss_kib={rss} pss_kib={pss} warmed_rss_kib={base} drifted={drift} grid_pool={grid_pool} chart_retained_points={retained} chart_ingested_points={} markdown_bytes={markdown_bytes} chat_messages={chat_messages} chat_stream_bytes={chat_stream_bytes} code_visible={code_visible} flow_nodes={flow_nodes} tree_nodes={} property_nodes={} modals={modals} toasts={toasts} toast_queue={toast_queue}",
            next - RATE,
            branches * 10 + 1,
            groups * 10 + 1
        );
        for ((_, name), times) in PHASES.into_iter().zip(&mut times) {
            if times.is_empty() {
                println!("component={name} frames=0");
                continue;
            }
            times.sort_unstable();
            let p = |q: usize| times[(times.len() * q).div_ceil(100usize) - 1];
            println!(
                "component={name} frames={} p50_us={} p95_us={} p99_us={} max_us={}",
                times.len(),
                p(50),
                p(95),
                p(99),
                p(100)
            );
            times.clear();
        }
    }
    if failed && profile_frames.is_none() {
        return Err("mixed soak exceeded the 50 ms frame or 2% warmed RSS budget".into());
    }
    Ok(())
}
