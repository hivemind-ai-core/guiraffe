//! `TracksView` on the 0.3 design: store-held heights, selection, playhead and view range;
//! user edits as bus actions; host writes without echo; a clip painter and header modes.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, IntoElement, MouseButton, ParentElement, Pixels,
    Point, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use guiraffe_core::{
    GuiraffeTheme, Scheme, apply,
    protocol::{Action, ActionBus, ActionError, ControlId, Interceptor, Outcome, Source},
};
use guiraffe_state::Store;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use guiraffe_timeline::{
    ClipPaint, ClipPainter, Edge, HeaderMode, TimeFormat, Track, TracksEvent, TracksView,
    tracks_model::{
        ClipEdit, ClipRef, Follow, SetFollow, SetPlayhead, SetSelection, SetTracks, SetView,
        TrackId, TrackRow, TrackUpdate, TracksAction, TracksModel, ViewRange, tracks_store,
    },
    tracks_view::RULER_H,
};

fn track(clips: &[(f64, f64)]) -> Track {
    let mut t = Track::default();
    for &(s, l) in clips {
        t.insert(s, l);
    }
    t
}

/// Rows of 40, 120, 30, 80, ... px; every row has clips at 1..3 and 5..6 s.
fn model(rows: usize) -> TracksModel {
    let heights = [40.0, 120.0, 30.0, 80.0];
    TracksModel::new(
        (0..rows)
            .map(|i| {
                TrackRow::new(
                    100 + i as u64,
                    format!("T{i}"),
                    track(&[(1.0, 2.0), (5.0, 1.0)]),
                )
                .with_height(heights[i % heights.len()])
            })
            .collect(),
    )
}

struct Root(Entity<TracksView>);
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

/// Everything the bus saw: actions with their source, and outcomes.
#[derive(Default)]
struct Log {
    actions: RefCell<Vec<(Source, Action)>>,
    outcomes: RefCell<Vec<Outcome>>,
    groups: RefCell<usize>,
}
struct Recorder(Rc<Log>);
impl Interceptor for Recorder {
    fn before(
        &self,
        _: ControlId,
        action: &Action,
        source: Source,
        _: &Window,
        _: &App,
    ) -> Result<(), ActionError> {
        self.0.actions.borrow_mut().push((source, action.clone()));
        Ok(())
    }
    fn after(
        &self,
        _: ControlId,
        _: &Action,
        _: Source,
        result: &Result<Outcome, ActionError>,
        _: &Window,
        _: &App,
    ) {
        if let Ok(outcome) = result {
            self.0.outcomes.borrow_mut().push(outcome.clone());
        }
    }
    fn begin_group(&self, _: guiraffe_core::protocol::Group, _: &Window, _: &App) {
        *self.0.groups.borrow_mut() += 1;
    }
}

struct Mounted<'a> {
    view: Entity<TracksView>,
    store: Store<TracksModel>,
    id: Option<ControlId>,
    log: Rc<Log>,
    events: Rc<RefCell<Vec<TracksEvent>>>,
    vcx: &'a mut VisualTestContext,
}

impl Mounted<'_> {
    fn body(&mut self) -> Bounds<Pixels> {
        self.vcx.debug_bounds("tracks-body").expect("tracks body")
    }
    /// Window point at time `t` and content y `y` (before scrolling).
    fn at(&mut self, t: f64, y: f32) -> Point<Pixels> {
        let body = self.body();
        let (x, scroll) = self.vcx.update(|_, cx| {
            let v = self.view.read(cx);
            (v.axis().x(t), v.scroll_y())
        });
        body.origin + point(px(x as f32), px(y - scroll))
    }
    fn model(&mut self) -> TracksModel {
        self.vcx
            .update(|_, cx| self.store.state(cx).as_ref().clone())
    }
    fn drag(&mut self, a: Point<Pixels>, b: Point<Pixels>) {
        self.vcx
            .simulate_mouse_down(a, MouseButton::Left, Default::default());
        self.vcx
            .simulate_mouse_move(b, Some(MouseButton::Left), Default::default());
        self.vcx
            .simulate_mouse_up(b, MouseButton::Left, Default::default());
        self.vcx.run_until_parked();
    }
    fn click(&mut self, p: Point<Pixels>) {
        self.vcx.simulate_click(p, Default::default());
        self.vcx.run_until_parked();
    }
    fn wheel(&mut self, dy: f32) {
        let body = self.body();
        self.vcx.simulate_event(gpui_kit::ScrollEvent {
            position: body.center(),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(0.0), px(dy))),
            ..Default::default()
        });
        self.vcx.run_until_parked();
    }
}

fn mount(cx: &mut TestAppContext, rows: usize, registered: bool) -> Mounted<'_> {
    cx.update(|cx| {
        gpui_kit::component::init(cx);
        guiraffe_core::apply(Default::default(), cx);
        guiraffe_timeline::init(cx);
    });
    let log = Rc::new(Log::default());
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut parts = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let store = tracks_store(model(rows), 50, cx);
        let view = TracksView::with_store(store.clone(), TimeFormat::Seconds, 10.0, cx);
        view.update(cx, |v, cx| v.set_resizable_tracks(true, cx));
        let id = registered.then(|| TracksView::register(&view, "arrange", window, cx));
        ActionBus::add_interceptor(Rc::new(Recorder(log.clone())), window, cx);
        let ev = events.clone();
        cx.subscribe(&view, move |_, _, e: &TracksEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach();
        parts = Some((view.clone(), store, id));
        Root(view)
    });
    vcx.simulate_resize(size(px(1140.), px(260.)));
    vcx.run_until_parked();
    let (view, store, id) = parts.expect("mounted");
    Mounted {
        view,
        store,
        id,
        log,
        events,
        vcx,
    }
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn mixed_heights_lay_out_hit_test_and_scroll(cx: &mut TestAppContext) {
    let mut m = mount(cx, 8, true);
    // Content tops: 0, 40, 160, 190, 270, 310, 430, 460 (total 540; the body is ~236 tall).
    let tops = m.vcx.update(|_, cx| {
        let l = m.view.read(cx).layout().clone();
        (0..8).map(|i| l.top(i)).collect::<Vec<_>>()
    });
    assert_eq!(tops, [0.0, 40.0, 160.0, 190.0, 270.0, 310.0, 430.0, 460.0]);
    // Row 1 is 120 px tall: its lower part still hits row 1.
    let p = m.at(2.0, 150.0);
    m.click(p);
    assert_eq!(m.model().selection, Some(ClipRef::new(101, 1)));
    // Scroll down; content y 175 is row 2 at any scroll.
    m.wheel(-100.0);
    let scroll = m.vcx.update(|_, cx| m.view.read(cx).scroll_y());
    assert!(scroll > 0.0, "the rows overflow, so the wheel scrolls them");
    let p = m.at(2.0, 175.0);
    m.click(p);
    assert_eq!(m.model().selection, Some(ClipRef::new(102, 1)));
    // Resize row 3 by its bottom edge (content y 270) while scrolled.
    let (a, b) = (m.at(4.0, 270.0), m.at(4.0, 300.0));
    m.drag(a, b);
    assert_eq!(m.model().row(3).map(|r| r.height), Some(110.0));
    let p = m.at(2.0, 290.0);
    m.click(p);
    assert_eq!(
        m.model().selection,
        Some(ClipRef::new(103, 1)),
        "content y 290 is now inside the taller row 3"
    );
    let p = m.at(2.0, 305.0);
    m.click(p);
    assert_eq!(m.model().selection, Some(ClipRef::new(104, 1)));
    // The scroll is clamped to the new content height.
    m.wheel(-10_000.0);
    let (scroll, total) = m.vcx.update(|_, cx| {
        let v = m.view.read(cx);
        (v.scroll_y(), v.layout().total())
    });
    let height = f32::from(m.body().size.height);
    assert_eq!(scroll, total - height);
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn a_resize_drag_is_one_bus_batch_and_one_undo_step(cx: &mut TestAppContext) {
    let mut m = mount(cx, 4, true);
    let a = m.at(4.0, 40.0);
    m.vcx
        .simulate_mouse_down(a, MouseButton::Left, Default::default());
    for dy in [10.0, 20.0, 35.0] {
        let p = m.at(4.0, 40.0 + dy);
        m.vcx
            .simulate_mouse_move(p, Some(MouseButton::Left), Default::default());
        m.vcx.run_until_parked();
    }
    assert_eq!(
        m.model().row(0).map(|r| r.height),
        Some(40.0),
        "a preview until release"
    );
    let b = m.at(4.0, 75.0);
    m.vcx
        .simulate_mouse_up(b, MouseButton::Left, Default::default());
    m.vcx.run_until_parked();
    assert_eq!(m.model().row(0).map(|r| r.height), Some(75.0));
    assert_eq!(*m.log.groups.borrow(), 1, "committed as one dispatch_batch");
    let actions: Vec<_> = m
        .log
        .actions
        .borrow()
        .iter()
        .map(|(s, a)| (*s, a.name.clone()))
        .collect();
    assert_eq!(actions, [(Source::Pointer, "resize_track".into())]);
    assert_eq!(
        *m.events.borrow(),
        [TracksEvent::TrackResized {
            track: TrackId(100),
            height: 75.0
        }]
    );
    m.vcx
        .update(|_, cx| m.store.dispatch(guiraffe_state::UndoEvent, cx));
    m.vcx.run_until_parked();
    assert_eq!(
        m.model().row(0).map(|r| r.height),
        Some(40.0),
        "one undo step"
    );
}

/// Each user action, through the pointer, and the same action through the bus.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn pointer_and_bus_paths_are_identical(cx: &mut TestAppContext) {
    let clip = ClipRef::new(100, 1);
    let mut pointer = mount(cx, 3, true);
    // Select by click, move by body drag, trim the end past the content, resize, seek.
    let p = pointer.at(2.0, 20.0);
    pointer.click(p);
    let (a, b) = (pointer.at(2.0, 20.0), pointer.at(2.5, 20.0));
    pointer.drag(a, b);
    let (a, b) = (pointer.at(3.5, 20.0), pointer.at(4.5, 20.0));
    pointer.drag(a, b); // the clip now ends at 3.5; drag its end edge to 4.5
    let (a, b) = (pointer.at(4.0, 40.0), pointer.at(4.0, 90.0));
    pointer.drag(a, b);
    let ruler = pointer.body().origin - point(px(0.0), px(RULER_H / 2.0));
    let x = pointer
        .vcx
        .update(|_, cx| pointer.view.read(cx).axis().x(4.2));
    pointer.click(ruler + point(px(x as f32), px(0.0)));
    let after_pointer = pointer.model();
    let sent: Vec<_> = pointer
        .log
        .actions
        .borrow()
        .iter()
        .map(|(s, a)| (*s, a.clone()))
        .collect();
    assert!(sent.iter().all(|(s, _)| *s == Source::Pointer));
    let names: Vec<_> = sent.iter().map(|(_, a)| a.name.to_string()).collect();
    assert_eq!(
        names,
        ["select", "move_clip", "trim_clip", "resize_track", "seek"]
    );
    let pointer_outcomes = pointer.log.outcomes.borrow().clone();
    let pointer_events = pointer.events.borrow().clone();
    assert!(matches!(
        pointer_events[2],
        TracksEvent::ClipChanged {
            edit: ClipEdit::Trim(Edge::End),
            ..
        }
    ));
    assert_eq!(
        after_pointer.clip(clip).map(|c| c.end()),
        Some(4.5),
        "the end trims past the content"
    );

    // A second window replays the same erased actions from a script.
    let mut bus = mount(cx, 3, true);
    let id = bus.id.expect("registered");
    bus.vcx.update(|window, cx| {
        for (_, action) in &sent {
            assert!(ActionBus::dispatch(id, action, Source::Script, window, cx).is_ok());
        }
    });
    bus.vcx.run_until_parked();
    assert_eq!(bus.model().rows(), after_pointer.rows());
    assert_eq!(bus.model().selection, after_pointer.selection);
    assert_eq!(bus.model().playhead, after_pointer.playhead);
    assert_eq!(*bus.log.outcomes.borrow(), pointer_outcomes);
    assert_eq!(*bus.events.borrow(), pointer_events);
}

/// With a minimum time, a wheel pan past it reads the minimum straight away, before the
/// next frame renders (#147).
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn a_wheel_pan_is_clamped_to_the_minimum_time_before_the_next_frame(cx: &mut TestAppContext) {
    let mut m = mount(cx, 2, false);
    let body = m.body();
    let mut starts = Vec::new();
    for dx in [400.0, -400.0] {
        let store = m.store.clone();
        let view = m.view.clone();
        m.vcx.update(|_, cx| {
            store.dispatch_now(guiraffe_timeline::tracks_model::SetMinTime(Some(0.0)), cx);
            store.dispatch_now(
                SetView(ViewRange {
                    start: 0.0,
                    scale: 50.0,
                }),
                cx,
            );
        });
        m.vcx.run_until_parked();
        // Dispatch the wheel and read the axis in one update: no frame runs in between.
        let start = m.vcx.update(|window, cx| {
            window.dispatch_event(
                gpui_kit::PlatformInput::Scroll(gpui_kit::ScrollEvent {
                    position: body.center(),
                    delta: gpui_kit::ScrollDelta::Pixels(point(px(dx), px(0.0))),
                    ..Default::default()
                }),
                cx,
            );
            view.read(cx).axis().start
        });
        starts.push(start);
        m.vcx.run_until_parked();
    }
    assert!(
        starts.iter().all(|&start| start >= 0.0),
        "the axis never reads below the minimum: {starts:?}"
    );
    assert!(
        starts.contains(&0.0) && starts.iter().any(|&start| start > 0.0),
        "one pan goes past the minimum and stops at it, the other moves away: {starts:?}"
    );
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn unregistered_pointer_input_applies_the_same_actions(cx: &mut TestAppContext) {
    let mut m = mount(cx, 2, false);
    let p = m.at(2.0, 20.0);
    m.click(p);
    let (a, b) = (m.at(2.0, 20.0), m.at(2.5, 20.0));
    m.drag(a, b);
    assert!(
        m.log.actions.borrow().is_empty(),
        "nothing registered, nothing on the bus"
    );
    assert_eq!(
        m.model().clip(ClipRef::new(100, 1)).map(|c| c.start),
        Some(1.5)
    );
    assert_eq!(m.events.borrow().len(), 2);
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn host_writes_never_echo_and_zoom_survives_track_updates(cx: &mut TestAppContext) {
    let mut m = mount(cx, 3, true);
    let view = ViewRange {
        start: 2.0,
        scale: 0.004,
    };
    m.vcx.update(|_, cx| {
        m.store.dispatch(SetView(view), cx);
        m.store.dispatch(SetPlayhead(3.0), cx);
        m.store
            .dispatch(SetSelection(Some(ClipRef::new(101, 2))), cx);
        m.store.dispatch(SetFollow(Follow::Page), cx);
    });
    m.vcx.run_until_parked();
    // Outside project change: new clips on track 2, everything else equal.
    let rows: Vec<_> = m
        .model()
        .rows()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let t = if i == 2 {
                track(&[(4.0, 1.0)])
            } else {
                r.track.clone()
            };
            TrackUpdate::new(r.id, r.name.clone(), t)
        })
        .collect();
    let before = m.model();
    m.vcx.update(|_, cx| m.store.dispatch(SetTracks(rows), cx));
    m.vcx.run_until_parked();
    let after = m.model();
    assert!(
        m.log.actions.borrow().is_empty(),
        "host writes are not actions"
    );
    assert!(
        m.events.borrow().is_empty(),
        "host writes emit no TracksEvent"
    );
    assert_eq!(after.view, Some(view), "zoom and scroll survive");
    assert_eq!(after.playhead, 3.0);
    assert_eq!(after.selection, before.selection);
    assert!(
        std::sync::Arc::ptr_eq(&after.rows()[0], &before.rows()[0]),
        "unchanged rows kept"
    );
    assert_eq!(after.rows()[1].height, 120.0);
    let axis = m.vcx.update(|_, cx| *m.view.read(cx).axis());
    assert_eq!((axis.start, axis.scale), (2.0, 0.004));
    // Follow mode turns the page for a host playhead write, still without an echo.
    m.vcx.update(|_, cx| m.store.dispatch(SetPlayhead(9.0), cx));
    m.vcx.run_until_parked();
    assert_eq!(m.model().view.map(|v| v.start), Some(9.0));
    assert!(m.events.borrow().is_empty());
    // A user seek is an action with its source.
    let id = m.id.expect("registered");
    m.vcx.update(|window, cx| {
        ActionBus::dispatch(
            id,
            &TracksAction::Seek(1.0).to_action(),
            Source::Shortcut,
            window,
            cx,
        )
        .ok();
    });
    assert_eq!(m.log.actions.borrow()[0].0, Source::Shortcut);
    assert_eq!(*m.events.borrow(), [TracksEvent::Seek(1.0)]);
}

/// Paints one bar per clip and counts calls.
struct Bars(Rc<RefCell<Vec<(usize, u32, bool)>>>);
impl ClipPainter for Bars {
    fn paint(&self, clip: &ClipPaint<'_>, window: &mut Window, _: &App) {
        self.0
            .borrow_mut()
            .push((clip.track, clip.clip.id, clip.selected));
        let b = clip.bounds;
        let dx = (clip.axis.x(clip.clip.start + 0.25) - clip.axis.x(clip.clip.start)) as f32;
        window.paint_quad(gpui_kit::fill(
            Bounds::new(
                point(b.origin.x + px(dx), b.origin.y + px(4.)),
                size(px(6.), (b.size.height - px(8.)).max(px(1.))),
            ),
            gpui_kit::rgb(0x22_88_cc),
        ));
    }
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn the_painter_runs_for_visible_clips_only(cx: &mut TestAppContext) {
    let m = mount(cx, 8, true);
    let calls = Rc::new(RefCell::new(Vec::new()));
    m.vcx.update(|_, cx| {
        m.view.update(cx, |v, cx| {
            v.select_many([ClipRef::new(100, 1), ClipRef::new(101, 2)], cx);
            v.set_clip_painter(Some(Rc::new(Bars(calls.clone()))), cx)
        })
    });
    m.vcx.run_until_parked();
    let painted: std::collections::BTreeSet<_> = calls.borrow().iter().map(|c| c.0).collect();
    let visible = m.vcx.update(|_, cx| {
        let v = m.view.read(cx);
        let h = 236.0;
        v.layout().rows_in(v.scroll_y(), v.scroll_y() + h)
    });
    let marked: std::collections::BTreeSet<_> = calls
        .borrow()
        .iter()
        .filter(|c| c.2)
        .map(|c| (c.0, c.1))
        .collect();
    assert_eq!(marked, [(0, 1), (1, 2)].into());
    assert!(!painted.is_empty());
    assert!(
        painted.iter().all(|row| visible.contains(row)),
        "{painted:?} vs {visible:?}"
    );
    assert!(
        !painted.contains(&7),
        "the last row is scrolled out of view"
    );
}

#[test]
fn tracks_selection_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(gpui_kit::component::init);
        let img = s
            .render(size(px(640.), px(240.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let model = TracksModel::new(vec![
                    TrackRow::new(1, "Drums", track(&[(0.5, 2.0), (3.0, 1.5)])),
                    TrackRow::new(2, "Keys", track(&[(1.0, 4.0)])).with_height(90.0),
                    TrackRow::new(3, "Bass", track(&[(0.0, 1.5), (2.0, 2.0)])).with_height(30.0),
                ]);
                let store = tracks_store(model, 10, cx);
                store.dispatch_now(
                    guiraffe_timeline::tracks_model::SetSelectionMany(
                        [ClipRef::new(2, 1), ClipRef::new(1, 2)].into(),
                    ),
                    cx,
                );
                store.dispatch_now(SetPlayhead(2.25), cx);
                let view = TracksView::with_store(store, TimeFormat::Seconds, 6.0, cx);
                view.update(cx, |v, cx| {
                    v.set_clip_painter(Some(Rc::new(Bars(Rc::default()))), cx);
                    v.set_header(
                        HeaderMode::Custom(
                            90.0,
                            Rc::new(|row, _, _| {
                                div()
                                    .size_full()
                                    .px(px(6.))
                                    .flex()
                                    .items_center()
                                    .when(row.selected, |d| {
                                        d.font_weight(gpui_kit::FontWeight::BOLD)
                                    })
                                    .child(row.row.name.clone())
                                    .into_any_element()
                            }),
                        ),
                        cx,
                    );
                });
                cx.new(|_| Root(view))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "tracks-selection/light"
        } else {
            "tracks-selection/dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn header_modes_size_the_column(cx: &mut TestAppContext) {
    let mut m = mount(cx, 2, false);
    assert_eq!(
        m.body().origin.x,
        px(guiraffe_timeline::tracks_view::HEADER_W)
    );
    m.vcx.update(|_, cx| {
        m.view
            .update(cx, |v, cx| v.set_header(HeaderMode::Hidden, cx))
    });
    m.vcx.run_until_parked();
    assert_eq!(
        m.body().origin.x,
        px(0.0),
        "hidden: the body starts at the left edge"
    );
    let custom = HeaderMode::Custom(60.0, Rc::new(|_, _, _| div().into_any_element()));
    m.vcx
        .update(|_, cx| m.view.update(cx, |v, cx| v.set_header(custom, cx)));
    m.vcx.run_until_parked();
    assert_eq!(m.body().origin.x, px(60.0));
}

/// Insert, remove and reorder tracks in the middle with host writes: heights, selection
/// and the rendered layout stay with each track's key, and nothing echoes.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn track_inserts_removes_and_moves_keep_state_with_the_key(cx: &mut TestAppContext) {
    let mut m = mount(cx, 4, true); // keys 100..=103, heights 40, 120, 30, 80
    let selected = ClipRef::new(102, 1);
    m.vcx
        .update(|_, cx| m.store.dispatch(SetSelection(Some(selected)), cx));
    m.vcx.run_until_parked();
    let rows = |m: &mut Mounted<'_>| -> Vec<TrackUpdate> {
        m.model()
            .rows()
            .iter()
            .map(|r| TrackUpdate::new(r.id, r.name.clone(), r.track.clone()))
            .collect()
    };
    let check =
        |m: &mut Mounted<'_>, keys: &[u64], heights: &[f32], selected_row: Option<usize>| {
            let model = m.model();
            let got: Vec<u64> = model.rows().iter().map(|r| r.id.0).collect();
            assert_eq!(got, keys);
            let got: Vec<f32> = model.rows().iter().map(|r| r.height).collect();
            assert_eq!(got, heights);
            let layout: Vec<f32> = m.vcx.update(|_, cx| {
                let l = m.view.read(cx).layout().clone();
                (0..l.len()).map(|i| l.height(i)).collect()
            });
            assert_eq!(layout, heights, "the rendered rows follow the keys");
            assert_eq!(model.selected_index().map(|s| s.0), selected_row);
            assert_eq!(
                m.vcx
                    .update(|_, cx| m.view.read(cx).selected(cx))
                    .map(|s| s.0),
                selected_row
            );
        };

    // Insert above the selection.
    let mut next = rows(&mut m);
    next.insert(1, TrackUpdate::new(150, "Inserted", track(&[(0.0, 9.0)])));
    m.vcx.update(|_, cx| m.store.dispatch(SetTracks(next), cx));
    m.vcx.run_until_parked();
    check(
        &mut m,
        &[100, 150, 101, 102, 103],
        &[40.0, 40.0, 120.0, 30.0, 80.0],
        Some(3),
    );
    assert_eq!(m.model().selection, Some(selected));

    // Remove a track above the selection.
    let mut next = rows(&mut m);
    next.retain(|u| u.id != TrackId(101));
    m.vcx.update(|_, cx| m.store.dispatch(SetTracks(next), cx));
    m.vcx.run_until_parked();
    check(
        &mut m,
        &[100, 150, 102, 103],
        &[40.0, 40.0, 30.0, 80.0],
        Some(2),
    );

    // Move the selected track to the top.
    let mut next = rows(&mut m);
    let moved = next.remove(2);
    next.insert(0, moved);
    m.vcx.update(|_, cx| m.store.dispatch(SetTracks(next), cx));
    m.vcx.run_until_parked();
    check(
        &mut m,
        &[102, 100, 150, 103],
        &[30.0, 40.0, 40.0, 80.0],
        Some(0),
    );

    // A click on the moved row still hits the selected track's clips by key.
    let p = m.at(2.0, 15.0);
    m.click(p);
    assert_eq!(m.model().selection, Some(selected), "same clip, no change");
    let p = m.at(2.0, 50.0);
    m.click(p);
    assert_eq!(m.model().selection, Some(ClipRef::new(100, 1)));
    assert_eq!(
        *m.events.borrow(),
        [TracksEvent::SelectionChanged {
            selected: Some(ClipRef::new(100, 1)),
            clips: [ClipRef::new(100, 1)].into(),
            source: Source::Pointer,
        }],
        "only the user's click emitted; the three host writes did not"
    );
    let names: Vec<_> = m
        .log
        .actions
        .borrow()
        .iter()
        .map(|(_, a)| a.name.to_string())
        .collect();
    assert_eq!(names, ["select"]);
}

/// A lock belongs to its clip's key, so it follows the track when tracks move, and the row
/// that takes the old position stays editable.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn clip_locks_follow_the_track_key(cx: &mut TestAppContext) {
    let mut m = mount(cx, 3, true); // keys 100, 101, 102
    let locked = ClipRef::new(101, 1);
    m.vcx
        .update(|_, cx| m.view.update(cx, |v, cx| v.set_locked(locked, true, cx)));
    // Move the locked track to the bottom: 100, 102, 101.
    let mut next: Vec<_> = m
        .model()
        .rows()
        .iter()
        .map(|r| TrackUpdate::new(r.id, r.name.clone(), r.track.clone()))
        .collect();
    let moved = next.remove(1);
    next.push(moved);
    m.vcx.update(|_, cx| m.store.dispatch(SetTracks(next), cx));
    m.vcx.run_until_parked();
    let id = m.id.expect("registered");
    let (moved_at, kept) = m.vcx.update(|window, cx| {
        let view = m.view.read(cx);
        let x = view.axis().x(2.0);
        let grabs = (0..3)
            .map(|row| view.grab(row, x, cx).is_some())
            .collect::<Vec<_>>();
        let refused = ActionBus::dispatch(
            id,
            &TracksAction::MoveClip {
                clip: locked,
                start: 1.5,
            }
            .to_action(),
            Source::Script,
            window,
            cx,
        );
        (grabs, refused)
    });
    assert_eq!(
        moved_at,
        [true, true, false],
        "only the moved track's clip is locked"
    );
    assert_eq!(
        kept,
        Err(ActionError::Disabled),
        "the bus refuses the locked clip too"
    );
    assert!(m.vcx.update(|_, cx| m.view.read(cx).is_locked(locked)));
    // The track now at the old position (102) still moves.
    let free = ClipRef::new(102, 1);
    m.vcx.update(|window, cx| {
        assert!(
            ActionBus::dispatch(
                id,
                &TracksAction::MoveClip {
                    clip: free,
                    start: 1.5
                }
                .to_action(),
                Source::Script,
                window,
                cx,
            )
            .is_ok()
        );
    });
    assert_eq!(m.model().clip(free).map(|c| c.start), Some(1.5));
}

#[gpui_kit::test]
fn context_menu_selects_clip_and_reports_window_position(cx: &mut TestAppContext) {
    let mut m = mount(cx, 4, true);
    let at = ClipRef::new(
        101,
        m.model().row(1).expect("second row").track.clips()[0].id,
    );
    let pointer = m.at(1.75, 92.);
    m.vcx
        .update(|_, cx| m.view.update(cx, |v, cx| v.set_focusable(Some(true), cx)));
    m.vcx
        .simulate_mouse_down(pointer, MouseButton::Right, Default::default());
    m.vcx
        .simulate_mouse_up(pointer, MouseButton::Right, Default::default());
    m.vcx.run_until_parked();
    assert_eq!(m.model().selection, Some(at));
    assert_eq!(
        *m.events.borrow(),
        [
            TracksEvent::SelectionChanged {
                selected: Some(at),
                clips: [at].into(),
                source: Source::Pointer
            },
            TracksEvent::ContextMenu {
                track: at.track,
                clip: at.clip,
                position: pointer,
                source: Source::Pointer
            },
        ]
    );
    let center = m.at(2., 100.);
    for key in ["menu", "shift-f10"] {
        m.events.borrow_mut().clear();
        m.vcx.simulate_keystrokes(key);
        assert_eq!(
            *m.events.borrow(),
            [TracksEvent::ContextMenu {
                track: at.track,
                clip: at.clip,
                position: center,
                source: Source::Keyboard,
            }]
        );
    }
    m.events.borrow_mut().clear();
    for empty in [m.at(4., 92.), m.at(1.75, 41.)] {
        m.vcx
            .simulate_mouse_down(empty, MouseButton::Right, Default::default());
        m.vcx
            .simulate_mouse_up(empty, MouseButton::Right, Default::default());
    }
    m.vcx.run_until_parked();
    assert!(m.events.borrow().is_empty());
    assert_eq!(m.model().selection, Some(at));
}

#[gpui_kit::test]
fn multi_selection_follows_pointer_marquee_host_and_context_menu(cx: &mut TestAppContext) {
    for registered in [false, true] {
        let mut m = mount(cx, 4, registered);
        let a = ClipRef::new(100, 1);
        let b = ClipRef::new(101, 1);
        let first = m.at(2., 15.);
        let second = m.at(2., 75.);
        m.click(first);
        let shift = gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        };
        m.vcx.simulate_click(second, shift);
        assert_eq!(m.model().selected_clips().collect::<Vec<_>>(), [b, a]);
        m.vcx
            .simulate_mouse_down(first, MouseButton::Right, Default::default());
        m.vcx
            .simulate_mouse_up(first, MouseButton::Right, Default::default());
        assert_eq!(m.model().selected_clips().collect::<Vec<_>>(), [b, a]);
        assert!(
            matches!(m.events.borrow().last(), Some(TracksEvent::ContextMenu {track,clip:1,..}) if *track==a.track)
        );
        m.vcx.simulate_click(
            first,
            gpui_kit::Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(m.model().selected_clips().collect::<Vec<_>>(), [b]);
        let before = m.events.borrow().len();
        m.vcx.update(|_, cx| {
            m.view
                .update(cx, |v, cx| v.select_many([a, a, ClipRef::new(999, 1)], cx))
        });
        assert_eq!(m.model().selected_clips().collect::<Vec<_>>(), [a]);
        assert_eq!(m.events.borrow().len(), before);
        let start = m.at(4., 125.);
        let end = m.at(1.5, 15.);
        m.drag(start, end);
        assert_eq!(m.model().selected_clips().collect::<Vec<_>>(), [a, b]);
        assert!(
            matches!(m.events.borrow().last(), Some(TracksEvent::SelectionChanged { clips,source:Source::Pointer,.. }) if clips.as_ref()==[a,b])
        );
        m.click(first);
        assert_eq!(m.model().selected_clips().collect::<Vec<_>>(), [a]);
    }
}
