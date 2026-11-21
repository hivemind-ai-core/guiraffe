//! guiraffe-timeline: Gantt, DAW tracks, dope sheet, curve editor on one shared time axis.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};
use guiraffe_timeline::*;

use gpui_kit::component::button::Button;
use guiraffe_components::{SegmentedControl, SegmentedEvent, Text};
use guiraffe_core::{ColorName, Size};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "timeline",
            name: "Event timeline · Git history",
            summary: "Follow a release from branch to merge, with tags and multiple events at the same position.",
            covers: &["EventTimeline", "EventTimelineState"],
            demo: git_history,
        },
        Entry {
            krate: "timeline",
            name: "Event timeline · Concurrent processes",
            summary: "Trace parallel workers and the process that joins their results.",
            covers: &["EventTimeline"],
            demo: concurrent_processes,
        },
        Entry {
            krate: "timeline",
            name: "Gantt chart",
            summary: "Critical-path scheduling; drag a bar to reschedule and dependents follow. Wheel pans, Ctrl+wheel zooms.",
            covers: &["GanttView"],
            demo: gantt,
        },
        Entry {
            krate: "timeline",
            name: "Track timeline",
            summary: "DAW-style clip tracks: drag to move, drag edges to trim, click the ruler to seek; clips never overlap.",
            covers: &["TracksView"],
            demo: tracks,
        },
        Entry {
            krate: "timeline",
            name: "Dope sheet",
            summary: "Hierarchical channels, summary keys, locks, box/group selection and frame-snapped editing (keyboard opt-in).",
            covers: &["DopeSheet"],
            demo: dope,
        },
        Entry {
            krate: "timeline",
            name: "Bar curve editor",
            summary: "Draw a quantized bipolar curve across 32 bars; fast strokes fill every bar.",
            covers: &["BarCurveView"],
            demo: bars,
        },
        Entry {
            krate: "timeline",
            name: "Curve editor",
            summary: "Multiple curves, draggable tangent handles; external toolbar edits keys, interpolation and tangents.",
            covers: &["CurveView"],
            demo: curve,
        },
    ]
}

struct Holder<V: 'static>(Entity<V>, &'static str, f32, Entity<EventLog>);

impl<V: Render> Render for Holder<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                self.1,
                div().h(px(self.2)).overflow_hidden().child(self.0.clone()),
            ),
            example("Events", self.3.clone()),
        ])
    }
}

fn hold<V: Render>(
    e: Entity<V>,
    label: &'static str,
    h: f32,
    log: Entity<EventLog>,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| Holder(e, label, h, log)).into()
}

const DAY: f64 = 86_400.0;

const EDGES: [ResizeEdges; 4] = [
    ResizeEdges::Both,
    ResizeEdges::Start,
    ResizeEdges::End,
    ResizeEdges::None,
];

/// Live editing options shared by the Gantt and track demos: which edges resize, and a
/// global lock. Changing an option touches only the timeline view, not the shell.
struct EditOptions<V: 'static> {
    view: Entity<V>,
    label: &'static str,
    height: f32,
    edges: Entity<SegmentedControl>,
    lock: Entity<SegmentedControl>,
    style: Entity<SegmentedControl>,
    grid: Entity<SegmentedControl>,
    log: Entity<EventLog>,
    note: &'static str,
    _subs: [gpui_kit::Subscription; 4],
}

const LOCK_STYLES: [LockStyle; 4] = [
    LockStyle::Badge,
    LockStyle::Dimmed,
    LockStyle::Hatched,
    LockStyle::None,
];

trait Editable: Render {
    /// Labels for the snapping / keyboard-step choices.
    const GRID: [&'static str; 3];
    fn set_edges(&mut self, edges: ResizeEdges, cx: &mut Context<Self>);
    fn set_all_editable(&mut self, editable: bool, cx: &mut Context<Self>);
    fn set_grid(&mut self, choice: usize, cx: &mut Context<Self>);
    fn set_style(&mut self, style: LockStyle, cx: &mut Context<Self>);
    fn watch(log: &Entity<EventLog>, view: &Entity<Self>, cx: &mut App);
}

impl Editable for GanttView {
    const GRID: [&'static str; 3] = ["Arrow step: 1 day", "1 week", "free"];
    fn set_grid(&mut self, choice: usize, cx: &mut Context<Self>) {
        self.set_key_step(
            [Some(DAY), Some(7. * DAY), None]
                .get(choice)
                .copied()
                .flatten(),
        );
        cx.notify();
    }
    fn watch(log: &Entity<EventLog>, view: &Entity<Self>, cx: &mut App) {
        EventLog::watch::<GanttEvent, _>(log, view, cx);
    }
    fn set_style(&mut self, style: LockStyle, cx: &mut Context<Self>) {
        self.set_lock_style(style, style, cx)
    }
    fn set_edges(&mut self, e: ResizeEdges, cx: &mut Context<Self>) {
        self.set_resize_edges(e, cx)
    }
    fn set_all_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_editable(on, cx)
    }
}

impl Editable for TracksView {
    const GRID: [&'static str; 3] = ["Snap: 0.25s", "1s", "off"];
    fn set_grid(&mut self, choice: usize, cx: &mut Context<Self>) {
        self.set_snap([Some(0.25), Some(1.0), None].get(choice).copied().flatten());
        cx.notify();
    }
    fn watch(log: &Entity<EventLog>, view: &Entity<Self>, cx: &mut App) {
        EventLog::watch::<TracksEvent, _>(log, view, cx);
    }
    fn set_style(&mut self, style: LockStyle, cx: &mut Context<Self>) {
        self.set_lock_style(style, style, cx)
    }
    fn set_edges(&mut self, e: ResizeEdges, cx: &mut Context<Self>) {
        self.set_resize_edges(e, cx)
    }
    fn set_all_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_editable(on, cx)
    }
}

fn with_options<V: Editable>(
    view: Entity<V>,
    label: &'static str,
    height: f32,
    note: &'static str,
    cx: &mut App,
) -> Entity<EditOptions<V>> {
    let edges = SegmentedControl::new(["Resize: both", "Start", "End", "None"], cx);
    let lock = SegmentedControl::new(["Editable", "Locked"], cx);
    let style = SegmentedControl::new(["Lock: badge", "Dimmed", "Hatched", "None"], cx);
    let grid = SegmentedControl::new(V::GRID, cx);
    view.update(cx, |view, cx| view.set_grid(0, cx));
    for c in [&edges, &lock, &style, &grid] {
        c.update(cx, |c, cx| c.set_size(Some(Size::Xs), cx));
    }
    let log = EventLog::new(cx);
    V::watch(&log, &view, cx);
    cx.new(|cx| {
        let v1 = view.clone();
        let v2 = view.clone();
        let v3 = view.clone();
        let v4 = view.clone();
        let s3 = cx.subscribe(&style, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            let style = LOCK_STYLES.get(*i).copied().unwrap_or_default();
            v3.update(cx, |v, cx| v.set_style(style, cx));
        });
        let s4 = cx.subscribe(&grid, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            v4.update(cx, |v, cx| v.set_grid(*i, cx));
        });
        let s1 = cx.subscribe(&edges, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            let edges = EDGES.get(*i).copied().unwrap_or_default();
            v1.update(cx, |v, cx| v.set_edges(edges, cx));
        });
        let s2 = cx.subscribe(&lock, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            v2.update(cx, |v, cx| v.set_all_editable(*i == 0, cx));
        });
        EditOptions {
            view,
            label,
            height,
            edges,
            lock,
            style,
            grid,
            log,
            note,
            _subs: [s1, s2, s3, s4],
        }
    })
}

impl<V: Editable> Render for EditOptions<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Options",
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(16.))
                    .child(self.edges.clone())
                    .child(self.lock.clone())
                    .child(self.style.clone())
                    .child(self.grid.clone())
                    .child(Text::new(self.note).size(Size::Xs).dimmed(true)),
            ),
            example(
                self.label,
                div()
                    .h(px(self.height))
                    .overflow_hidden()
                    .child(self.view.clone()),
            ),
            example("Events", self.log.clone()),
        ])
    }
}

fn gantt(_: &mut Window, cx: &mut App) -> AnyView {
    gantt_options(cx).into()
}

fn gantt_options(cx: &mut App) -> Entity<EditOptions<GanttView>> {
    let start = 20_400.0 * DAY;
    let t = |id, name: &str, d: f64, after: &[u32]| Task {
        id,
        name: name.into(),
        not_before: start,
        duration: d * DAY,
        after: after.to_vec(),
    };
    let v = GanttView::new(
        vec![
            t(1, "Research", 3.0, &[]),
            t(2, "Design", 4.0, &[1]),
            t(3, "Build", 8.0, &[2]),
            t(4, "Docs", 3.0, &[2]),
            t(5, "QA", 3.0, &[3]),
            t(6, "Launch", 1.0, &[4, 5]),
        ],
        TimeFormat::Days,
        cx,
    );
    // Item locks keep their own small badge; global lock does not change styling.
    v.update(cx, |v, cx| {
        v.set_focusable(Some(true), cx);
        v.set_locked(6, true, cx);
    });
    with_options(
        v,
        "Project plan (critical path in red; Launch is locked)",
        280.,
        "Select a bar. ←/→ move by the Arrow step, Shift+←/→ resize the end, and Alt+←/→ resize the start.",
        cx,
    )
}

fn tracks(_: &mut Window, cx: &mut App) -> AnyView {
    let mk = |clips: &[(f64, f64)]| {
        let mut t = Track::default();
        for (s, l) in clips {
            t.insert(*s, *l);
        }
        t
    };
    let v = TracksView::new(
        vec![
            ("Drums".into(), mk(&[(0.0, 4.0), (5.0, 3.0)])),
            ("Bass".into(), mk(&[(1.0, 6.0)])),
            ("Keys".into(), mk(&[(2.0, 1.5), (4.0, 1.5), (6.0, 2.0)])),
            ("Vocals".into(), mk(&[(3.0, 5.0)])),
        ],
        TimeFormat::Seconds,
        10.0,
        cx,
    );

    // Lock the vocal take.
    v.update(cx, |v, cx| {
        v.set_focusable(Some(true), cx);
        if let Some(id) = v
            .tracks()
            .get(3)
            .and_then(|(_, t)| t.clips().first())
            .map(|c| c.id)
        {
            v.set_locked(3, id, true, cx);
        }
    });
    with_options(
        v,
        "Four tracks, 0.25s snap (vocals locked)",
        220.,
        "Select a clip. ←/→ move, Shift+←/→ trim the end, and Alt+←/→ trim the start.",
        cx,
    )
    .into()
}

fn dope(_: &mut Window, cx: &mut App) -> AnyView {
    let view = DopeSheet::new(
        vec![
            Channel::group(
                "Scene summary",
                vec![
                    Channel::group(
                        "Camera",
                        vec![
                            Channel::with_keyframes(
                                "position.x",
                                vec![
                                    Key::new(0., 0., Interp::Constant),
                                    Key::new(0.5, 0., Interp::Bezier),
                                    Key::new(1., 1., Interp::Linear),
                                    Key::new(2., 1., Interp::Constant),
                                ],
                            )
                            .with_color(ColorName::Blue),
                            Channel::with_keyframes(
                                "position.y",
                                vec![
                                    Key::new(0., 0., Interp::Bezier),
                                    Key::new(0.75, 1., Interp::Bezier),
                                    Key::new(1.5, 0., Interp::Bezier),
                                ],
                            )
                            .with_color(ColorName::Teal),
                        ],
                    )
                    .with_color(ColorName::Violet),
                    Channel::group(
                        "Object",
                        vec![
                            Channel::new("rotation", vec![0.25, 1.25, 2.25])
                                .with_color(ColorName::Orange),
                            Channel::with_keyframes(
                                "opacity",
                                vec![
                                    Key::new(0., 0., Interp::Constant),
                                    Key::new(0.2, 1., Interp::Constant),
                                    Key::new(2.8, 1., Interp::Constant),
                                    Key::new(3., 0., Interp::Constant),
                                ],
                            )
                            .with_color(ColorName::Pink),
                        ],
                    )
                    .with_color(ColorName::Orange),
                ],
            )
            .with_color(ColorName::Violet),
        ],
        TimeFormat::Frames(24.),
        3.,
        cx,
    );
    view.update(cx, |view, cx| {
        view.set_focusable(true, cx);
        view.press(2, view.axis().x(0.5));
        view.release();
    });
    let lock = SegmentedControl::new(["Editable", "Locked"], cx);
    let style = SegmentedControl::new(["Lock: badge", "Dimmed", "Hatched", "None"], cx);
    for c in [&lock, &style] {
        c.update(cx, |c, cx| c.set_size(Some(Size::Xs), cx));
    }
    let log = EventLog::new(cx);
    EventLog::watch::<DopeEvent, _>(&log, &view, cx);
    cx.new(|cx| {
        let target = view.clone();
        let sub = cx.subscribe(&lock, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            target.update(cx, |view, cx| view.set_editable(*i == 0, cx));
        });
        let target = view.clone();
        let sub2 = cx.subscribe(&style, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            let style = LOCK_STYLES.get(*i).copied().unwrap_or_default();
            target.update(cx, |view, cx| view.set_lock_style(style, cx));
        });
        DopeDemo {
            view,
            lock,
            style,
            log,
            _sub: [sub, sub2],
        }
    })
    .into()
}
struct DopeDemo {
    view: Entity<DopeSheet>,
    lock: Entity<SegmentedControl>,
    style: Entity<SegmentedControl>,
    log: Entity<EventLog>,
    _sub: [gpui_kit::Subscription; 2],
}
impl Render for DopeDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Options",
                div()
                    .flex()
                    .gap(px(16.))
                    .child(self.lock.clone())
                    .child(self.style.clone()),
            ),
            example(
                "24 fps · Shift-click / box select · Tab to focus · Ctrl+A / Ctrl+D · Delete · Arrows",
                div().h(px(260.)).child(self.view.clone()),
            ),
            example("Events", self.log.clone()),
        ])
    }
}

fn curve(_: &mut Window, cx: &mut App) -> AnyView {
    let mut c = Curve::default();
    let mut k = Key::new(0.0, 0.0, Interp::Bezier);
    k.out_handle = (0.4, 0.0);
    c.set(k);
    let mut k = Key::new(1.0, 1.0, Interp::Linear);
    k.in_handle = (-0.4, 0.0);
    c.set(k);
    c.set(Key::new(2.0, 0.3, Interp::Constant));
    c.set(Key::new(3.0, 0.8, Interp::Linear));
    let view = CurveView::new(c, 0.0, 3.0, -0.2, 1.2, cx);
    let mut secondary = Curve::default();
    secondary.set(Key::new(0., 0.8, Interp::Linear));
    secondary.set(Key::new(3., 0.2, Interp::Linear));
    view.update(cx, |v, cx| {
        v.add_curve(
            CurveSeries::new("Opacity", secondary, ColorName::Orange),
            cx,
        );
        v.select_key(0, 0, cx);
    });
    let interp = SegmentedControl::new(["Constant", "Linear", "Bezier"], cx);
    let tangent = SegmentedControl::new(["Free", "Aligned", "Mirrored", "Flat"], cx);
    cx.new(|cx| {
        let a = view.clone();
        let b = view.clone();
        let s1 = cx.subscribe(&interp, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            a.update(cx, |v, cx| {
                v.set_interpolation(
                    [Interp::Constant, Interp::Linear, Interp::Bezier]
                        .get(*i)
                        .copied()
                        .unwrap_or(Interp::Linear),
                    cx,
                );
            });
        });
        let s2 = cx.subscribe(&tangent, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            b.update(cx, |v, _| {
                v.set_tangent_mode(
                    [
                        TangentMode::Free,
                        TangentMode::Aligned,
                        TangentMode::Mirrored,
                        TangentMode::Flat,
                    ]
                    .get(*i)
                    .copied()
                    .unwrap_or_default(),
                )
            });
        });
        let log = EventLog::new(cx);
        EventLog::watch::<CurveEvent, _>(&log, &view, cx);
        CurveDemo {
            view,
            interp,
            tangent,
            log,
            _subs: [s1, s2],
        }
    })
    .into()
}

struct CurveDemo {
    view: Entity<CurveView>,
    interp: Entity<SegmentedControl>,
    tangent: Entity<SegmentedControl>,
    log: Entity<EventLog>,
    _subs: [gpui_kit::Subscription; 2],
}
impl Render for CurveDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let add = self.view.clone();
        let delete = self.view.clone();
        let fit = self.view.clone();
        column([
            example(
                "External editor toolbar",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(Size::Sm.spacing()))
                    .child(
                        div()
                            .flex()
                            .gap(px(Size::Xs.spacing()))
                            .child(
                                Button::new("curve-add")
                                    .label("Add key")
                                    .tab_stop(false)
                                    .on_click(move |_, _, cx| {
                                        add.update(cx, |view, cx| {
                                            let time = (view.axis().start + view.axis().end()) / 2.;
                                            view.insert_at(time, cx);
                                        })
                                    }),
                            )
                            .child(
                                Button::new("curve-delete")
                                    .label("Delete key")
                                    .tab_stop(false)
                                    .on_click(move |_, _, cx| {
                                        delete.update(cx, |view, cx| {
                                            view.remove_selected(cx);
                                        })
                                    }),
                            )
                            .child(
                                Button::new("curve-fit")
                                    .label("Fit + normalize")
                                    .tab_stop(false)
                                    .on_click(move |_, _, cx| {
                                        fit.update(cx, |view, cx| view.fit_view(true, cx))
                                    }),
                            ),
                    )
                    .child(self.interp.clone())
                    .child(self.tangent.clone()),
            ),
            example(
                "Select keys to reveal tangent handles",
                div().h(px(260.)).child(self.view.clone()),
            ),
            example("Events", self.log.clone()),
        ])
    }
}
fn bars(_: &mut Window, cx: &mut App) -> AnyView {
    let mut model = BarCurve::new(32, -1., 1.);
    model.bipolar = true;
    model.set_steps(Some(16));
    model.draw((0., -0.8), (15., 0.8));
    model.draw((16., 0.8), (31., -0.4));
    let view = BarCurveView::new(model, cx);
    let log = EventLog::new(cx);
    EventLog::watch::<BarEvent, _>(&log, &view, cx);
    hold(
        view,
        "32 bars · bipolar · 16 steps · drag to draw",
        220.,
        log,
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        Focusable, TestAppContext,
        test::{TestQueryExt, TestWindowExt},
    };

    struct GanttDemo(Entity<EditOptions<GanttView>>);

    impl Render for GanttDemo {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    #[gpui_kit::test]
    fn gantt_bar_click_and_live_grid_knob_control_arrow_step(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let options = cx.update(gantt_options);
        let (_, window) = cx.add_window_view(|_, _| GanttDemo(options.clone()));
        window.update(|w, cx| w.render_frame(cx));
        let gantt = options.read_with(window, |opts, _| opts.view.clone());
        let start = |window: &mut gpui_kit::VisualTestContext| {
            gantt.read_with(window, |view, _| {
                view.schedule().as_ref().expect("schedule")[&1].early_start
            })
        };
        let before = start(window);
        window.update(|w, cx| {
            w.click(("gantt-bar", 0usize), cx);
            assert!(gantt.read(cx).focus_handle(cx).is_focused(w));
            w.press("right", cx);
        });
        assert_eq!(start(window), before + DAY);
        let week = window.update(|w, _| w.find_by_label("1 week").bounds().center());
        window.simulate_click(week, Default::default());
        assert_eq!(
            options.read_with(window, |opts, cx| opts.grid.read(cx).selected()),
            1
        );
        window.update(|w, cx| {
            w.render_frame(cx);
            w.click(("gantt-bar", 0usize), cx);
            w.press("right", cx);
        });
        assert_eq!(start(window), before + 8. * DAY);
    }
}

struct EventHistory(Entity<EventTimelineState>, Entity<EventTimelineState>);

impl Render for EventHistory {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(Text::new("Vertical").size(Size::Sm).dimmed(true))
                .child(div().h(px(275.)).child(EventTimeline::new(&self.0)))
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(Text::new("Horizontal").size(Size::Sm).dimmed(true))
                .child(div().h(px(158.)).child(EventTimeline::new(&self.1)))
                .into_any_element(),
        ])
    }
}

type HistoryRow = (
    u64,
    i64,
    &'static str,
    &'static str,
    &'static [&'static str],
);

fn history(
    labels: [&str; 3],
    rows: &[HistoryRow],
    endpoints: &[(u64, u64, LinkKind)],
    cx: &mut App,
) -> AnyView {
    let colors = [ColorName::Blue, ColorName::Teal, ColorName::Grape];
    let make_model = || {
        let tracks = labels
            .into_iter()
            .zip(colors)
            .enumerate()
            .map(|(id, (label, color))| EventTrack {
                id: id as u64,
                label: label.into(),
                color: Some(color),
            })
            .collect();
        let events = rows
            .iter()
            .enumerate()
            .map(
                |(id, &(track, position, label, content, tags))| TimelineEvent {
                    id: id as u64,
                    track,
                    position,
                    label: label.into(),
                    content: Some(content.into()),
                    tags: tags.iter().map(|tag| (*tag).into()).collect(),
                    color: (label == "Publish").then_some(ColorName::Green),
                },
            )
            .collect();
        let links = endpoints
            .iter()
            .map(|&(source, destination, kind)| EventLink {
                source,
                destination,
                kind,
                color: (kind == LinkKind::Merge && destination == 5).then_some(ColorName::Violet),
            })
            .collect();
        EventModel::new(tracks, events, links)
    };
    let (Ok(vertical), Ok(horizontal)) = (make_model(), make_model()) else {
        return cx.new(|_| InvalidHistory).into();
    };
    let vertical = cx.new(|cx| {
        EventTimelineState::new(vertical, cx)
            .size(Size::Sm)
            .focusable(true)
    });
    let horizontal = cx.new(|cx| {
        EventTimelineState::new(horizontal, cx)
            .orientation(EventOrientation::Horizontal)
            .size(Size::Xs)
            .focusable(true)
            .item_content(|item, _, _| {
                div()
                    .truncate()
                    .child(
                        Text::new(item.content.as_deref().unwrap_or_default().to_owned())
                            .size(Size::Xs)
                            .dimmed(true),
                    )
                    .into_any_element()
            })
    });
    cx.new(|_| EventHistory(vertical, horizontal)).into()
}

struct InvalidHistory;

impl Render for InvalidHistory {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("Invalid event history")
    }
}

fn git_history(_: &mut Window, cx: &mut App) -> AnyView {
    history(
        ["main", "release/0.2", "fix/focus"],
        &[
            (0, 0, "Prepare release", "f312cad · Update changelog", &[]),
            (1, 0, "Cut release", "6bd829e · Release branch", &["0.2"]),
            (1, 0, "Pin dependencies", "fa401c1 · Refresh lockfile", &[]),
            (
                2,
                1,
                "Preserve focus",
                "d119b3f · Restore keyboard focus",
                &[],
            ),
            (
                1,
                2,
                "Merge focus fix",
                "3d81cf4 · Verify both backends",
                &[],
            ),
            (0, 3, "Publish", "1b4f2f0 · Release GUIraffe", &["v0.2.0"]),
        ],
        &[
            (0, 1, LinkKind::Branch),
            (2, 3, LinkKind::Branch),
            (3, 4, LinkKind::Merge),
            (4, 5, LinkKind::Merge),
        ],
        cx,
    )
}

fn concurrent_processes(_: &mut Window, cx: &mut App) -> AnyView {
    history(
        ["Coordinator", "API worker", "UI worker"],
        &[
            (
                0,
                0,
                "Dispatch workers",
                "Run 47 · Two independent jobs",
                &[],
            ),
            (1, 1, "Run contract tests", "68 requests checked", &[]),
            (1, 1, "Upload trace", "Saved request timings", &[]),
            (2, 1, "Run visual tests", "Light and dark snapshots", &[]),
            (2, 2, "Upload captures", "Four references checked", &[]),
            (0, 3, "Join results", "All checks passed", &["Passed"]),
        ],
        &[
            (0, 1, LinkKind::Branch),
            (0, 3, LinkKind::Branch),
            (2, 5, LinkKind::Merge),
            (4, 5, LinkKind::Merge),
        ],
        cx,
    )
}
