//! `DopeSheet` authoring on the 0.3 design: a store-held document keyed by host ids, user
//! edits as bus actions, value and toggle channels, curve lanes and a keyboard-only flow.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement,
    Pixels, Point, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use guiraffe_core::{
    GuiraffeTheme, Scheme, apply,
    protocol::{Action, ActionBus, ActionError, ControlId, Group, Interceptor, Outcome, Source},
};
use guiraffe_state::Store;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use guiraffe_timeline::{
    Axis, Interp, TimeFormat,
    dope_sheet::{
        ChannelFlag, ChannelId, DopeAction, DopeChannel, DopeDoc, DopeEvent, DopeKey, DopeSheet,
        KeyId, KeyRef, NAME_W, ROW_H, RULER_H, ValueBounds, dope_store,
    },
    dope_store::{SetChannels, SetDopeSelection, SetPlayhead},
};

fn bounds() -> ValueBounds {
    ValueBounds::new(0.0, 10.0)
}

/// A value channel (10), a toggle channel (20, on from 1 to 3) and a marker channel (30).
fn doc() -> DopeDoc {
    DopeDoc::new(vec![
        DopeChannel::value(
            10,
            "Cutoff",
            vec![
                DopeKey::new(1, 0.0, 0.0, Interp::Linear),
                DopeKey::new(2, 1.0, 10.0, Interp::Constant),
                DopeKey::new(3, 2.0, 4.0, Interp::Linear),
            ],
            bounds(),
        ),
        DopeChannel::toggle(
            20,
            "Bypass",
            vec![(KeyId(4), 1.0, true), (KeyId(5), 3.0, false)],
        ),
        DopeChannel::keys(30, "Notes", vec![DopeKey::new(6, 0.5, 0.0, Interp::Linear)]),
    ])
}

struct Root(Entity<DopeSheet>);
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

#[derive(Default)]
struct Log {
    actions: RefCell<Vec<(Source, String)>>,
    outcomes: RefCell<Vec<Outcome>>,
    groups: RefCell<usize>,
}
struct Recorder(Rc<Log>);
impl Interceptor for Recorder {
    fn before(
        &self,
        _: ControlId,
        a: &Action,
        s: Source,
        _: &Window,
        _: &App,
    ) -> Result<(), ActionError> {
        self.0.actions.borrow_mut().push((s, a.name.to_string()));
        Ok(())
    }
    fn after(
        &self,
        _: ControlId,
        _: &Action,
        _: Source,
        r: &Result<Outcome, ActionError>,
        _: &Window,
        _: &App,
    ) {
        if let Ok(o) = r {
            self.0.outcomes.borrow_mut().push(o.clone());
        }
    }
    fn begin_group(&self, _: Group, _: &Window, _: &App) {
        *self.0.groups.borrow_mut() += 1;
    }
}

struct Mounted<'a> {
    sheet: Entity<DopeSheet>,
    store: Store<DopeDoc>,
    id: Option<ControlId>,
    log: Rc<Log>,
    events: Rc<RefCell<Vec<DopeEvent>>>,
    vcx: &'a mut VisualTestContext,
}

impl Mounted<'_> {
    fn body(&mut self) -> Bounds<Pixels> {
        self.vcx.debug_bounds("dope-body").expect("dope body")
    }
    /// Window point at time `t` in display row `row` (vertical middle, plus `dy`).
    fn at(&mut self, t: f64, row: usize, dy: f32) -> Point<Pixels> {
        let body = self.body();
        let x = self.vcx.update(|_, cx| self.sheet.read(cx).axis().x(t));
        body.origin + point(px(x as f32), px(row as f32 * ROW_H + ROW_H / 2. + dy))
    }
    fn doc(&mut self) -> DopeDoc {
        self.vcx
            .update(|_, cx| self.store.state(cx).as_ref().clone())
    }
    fn names(&self) -> Vec<String> {
        self.log
            .actions
            .borrow()
            .iter()
            .map(|(_, n)| n.clone())
            .collect()
    }
    fn click(&mut self, p: Point<Pixels>, modifiers: Modifiers, count: usize) {
        if count == 1 {
            self.vcx.simulate_click(p, modifiers);
            self.vcx.run_until_parked();
            return;
        }
        self.vcx.simulate_mouse_move(p, None, modifiers);
        self.vcx.simulate_event(gpui_kit::MouseDownEvent {
            position: p,
            button: MouseButton::Left,
            modifiers,
            click_count: count,
            first_mouse: false,
        });
        self.vcx.simulate_event(gpui_kit::MouseUpEvent {
            position: p,
            button: MouseButton::Left,
            modifiers,
            click_count: count,
        });
        self.vcx.run_until_parked();
    }
    fn drag(&mut self, a: Point<Pixels>, b: Point<Pixels>, modifiers: Modifiers) {
        self.vcx
            .simulate_mouse_down(a, MouseButton::Left, modifiers);
        let mid = point((a.x + b.x) / 2., (a.y + b.y) / 2.);
        self.vcx
            .simulate_mouse_move(mid, Some(MouseButton::Left), modifiers);
        self.vcx
            .simulate_mouse_move(b, Some(MouseButton::Left), modifiers);
        self.vcx.simulate_mouse_up(b, MouseButton::Left, modifiers);
        self.vcx.run_until_parked();
    }
}

fn mount(cx: &mut TestAppContext, registered: bool) -> Mounted<'_> {
    cx.update(|cx| {
        gpui_kit::component::init(cx);
        apply(Default::default(), cx);
        guiraffe_timeline::init(cx);
    });
    let log = Rc::new(Log::default());
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut parts = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let store = dope_store(doc(), 50, cx);
        // Fitted at the window's first width; 8 s keeps 0..3.5 s in view after the resize.
        let sheet = DopeSheet::with_store(store.clone(), TimeFormat::Frames(10.), 8.0, cx);
        sheet.update(cx, |s, cx| s.set_focusable(true, cx));
        let id = registered.then(|| DopeSheet::register(&sheet, "automation", window, cx));
        ActionBus::add_interceptor(Rc::new(Recorder(log.clone())), window, cx);
        let ev = events.clone();
        cx.subscribe(&sheet, move |_, _, e: &DopeEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach();
        parts = Some((sheet.clone(), store, id));
        Root(sheet)
    });
    vcx.simulate_resize(size(px(1000.), px(400.)));
    vcx.run_until_parked();
    let (sheet, store, id) = parts.expect("mounted");
    Mounted {
        sheet,
        store,
        id,
        log,
        events,
        vcx,
    }
}

fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Default::default()
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn each_pointer_edit_is_one_bus_action_and_one_event(cx: &mut TestAppContext) {
    let mut m = mount(cx, true);
    // Double-click an empty cell on the value row: insert, keeping the curve's value.
    let p = m.at(0.5, 0, 0.);
    m.click(p, Modifiers::default(), 2);
    let inserted = m
        .doc()
        .channel(ChannelId(10))
        .and_then(|c| c.curve.at_time(0.5).copied());
    assert_eq!(inserted.map(|k| k.key.v), Some(5.0));
    // Click the toggle row's bar: off from 2 (snapped) to its next switch.
    let p = m.at(2.0, 1, 0.);
    m.click(p, Modifiers::default(), 1);
    assert!(
        m.doc()
            .channel(ChannelId(20))
            .is_some_and(|c| c.curve.is_on(1.5) && !c.curve.is_on(2.5))
    );
    // Alt-drag key 3 (t = 2, v = 4) up half a row: value + 5 (the row spans the range).
    let (a, b) = (m.at(2.0, 0, 0.), m.at(2.0, 0, -ROW_H / 2.));
    m.drag(a, b, alt());
    assert_eq!(m.doc().key(KeyRef::new(10, 3)).map(|k| k.key.v), Some(9.0));
    // Retime key 3 by dragging it two frames later: one batch, one undo step.
    let (a, b) = (m.at(2.0, 0, 0.), m.at(2.2, 0, 0.));
    m.drag(a, b, Modifiers::default());
    assert_eq!(m.doc().key(KeyRef::new(10, 3)).map(|k| k.key.t), Some(2.2));
    // Right-click the first segment: the next interpolation.
    let p = m.at(0.25, 0, 0.);
    m.vcx.simulate_event(gpui_kit::MouseDownEvent {
        position: p,
        button: MouseButton::Right,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    m.vcx.simulate_event(gpui_kit::MouseUpEvent {
        position: p,
        button: MouseButton::Right,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    m.vcx.run_until_parked();
    assert_eq!(
        m.doc().key(KeyRef::new(10, 1)).map(|k| k.key.interp),
        Some(Interp::Bezier)
    );
    // Ctrl-drag up inside the segment from 0.5 to 1: bend it.
    let (a, b) = (m.at(0.75, 0, 0.), m.at(0.75, 0, -12.));
    m.drag(a, b, ctrl());
    let names = m.names();
    assert_eq!(
        names,
        [
            "insert_key",
            "toggle_span",
            "select",
            "set_key_value",
            "retime_keys",
            "set_segment_interp",
            "bend_segment",
        ],
        "every edit went through the bus"
    );
    assert!(
        m.log
            .actions
            .borrow()
            .iter()
            .all(|(s, _)| *s == Source::Pointer)
    );
    let events = m.events.borrow();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            DopeEvent::KeyInserted { .. } => "inserted",
            DopeEvent::SpanToggled { .. } => "toggled",
            DopeEvent::SelectionChanged => "selection",
            DopeEvent::KeyValueChanged { .. } => "value",
            DopeEvent::KeysRetimed { .. } => "retimed",
            DopeEvent::SegmentInterpChanged { .. } => "interp",
            DopeEvent::SegmentBent { .. } => "bent",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "inserted",
            "toggled",
            "selection",
            "value",
            "retimed",
            "interp",
            "bent"
        ]
    );
    assert_eq!(
        *m.log.groups.borrow(),
        3,
        "value, retime and bend drags each commit one batch"
    );
    drop(events);
    // Undo the retime in one step.
    m.vcx
        .update(|_, cx| m.store.dispatch(guiraffe_state::UndoEvent, cx));
    m.vcx.run_until_parked();
    m.vcx
        .update(|_, cx| m.store.dispatch(guiraffe_state::UndoEvent, cx));
    m.vcx.run_until_parked();
    m.vcx
        .update(|_, cx| m.store.dispatch(guiraffe_state::UndoEvent, cx));
    m.vcx.run_until_parked();
    assert_eq!(
        m.doc().key(KeyRef::new(10, 3)).map(|k| k.key.t),
        Some(2.0),
        "bend, interp and retime undone"
    );
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn bus_clients_drive_the_same_edits(cx: &mut TestAppContext) {
    let mut m = mount(cx, true);
    let id = m.id.expect("registered");
    let k3 = KeyRef::new(10, 3);
    m.vcx.update(|window, cx| {
        for action in [
            DopeAction::InsertKey {
                channel: ChannelId(10),
                t: 0.5,
                value: None,
            },
            DopeAction::SetKeyValue {
                key: k3,
                value: 9.0,
            },
            DopeAction::SetSegmentInterp {
                key: KeyRef::new(10, 1),
                interp: Interp::EaseOut,
            },
            DopeAction::ToggleSpan {
                channel: ChannelId(20),
                t: 2.0,
            },
            DopeAction::RetimeKeys {
                keys: vec![k3],
                delta: 0.2,
            },
        ] {
            let outcome = ActionBus::dispatch(id, &action.to_action(), Source::Script, window, cx);
            assert!(outcome.is_ok_and(|o| !o.is_empty()), "{action:?}");
        }
    });
    let doc = m.doc();
    assert_eq!(doc.key(k3).map(|k| (k.key.t, k.key.v)), Some((2.2, 9.0)));
    assert_eq!(
        doc.key(KeyRef::new(10, 1)).map(|k| k.key.interp),
        Some(Interp::EaseOut)
    );
    assert_eq!(m.events.borrow().len(), 5, "one event per edit");
    // A locked channel refuses edits from every source.
    m.vcx.update(|window, cx| {
        ActionBus::dispatch(
            id,
            &DopeAction::SetChannelFlag {
                channel: ChannelId(10),
                flag: ChannelFlag::Locked,
                on: true,
            }
            .to_action(),
            Source::Script,
            window,
            cx,
        )
        .ok();
        let refused = ActionBus::dispatch(
            id,
            &DopeAction::SetKeyValue {
                key: k3,
                value: 1.0,
            }
            .to_action(),
            Source::Script,
            window,
            cx,
        );
        assert!(
            refused.is_ok_and(|o| o.is_empty()),
            "a refused edit is a no-op outcome"
        );
    });
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn host_writes_never_echo_and_selection_follows_ids(cx: &mut TestAppContext) {
    let mut m = mount(cx, true);
    m.vcx.update(|_, cx| {
        m.store
            .dispatch(SetDopeSelection(vec![KeyRef::new(10, 3)]), cx);
        m.store.dispatch(SetPlayhead(1.5), cx);
    });
    m.vcx.run_until_parked();
    // Reorder and insert channels from the host: the selection stays on key 3 of channel 10.
    let mut channels: Vec<DopeChannel> = m.doc().channels().iter().map(|c| (**c).clone()).collect();
    channels.reverse();
    channels.insert(0, DopeChannel::keys(40, "New", vec![]));
    m.vcx
        .update(|_, cx| m.store.dispatch(SetChannels(channels), cx));
    m.vcx.run_until_parked();
    let ids: Vec<u64> = m.doc().channels().iter().map(|c| c.id.0).collect();
    assert_eq!(ids, [40, 30, 20, 10]);
    let (selected, playhead) = m.vcx.update(|_, cx| {
        let s = m.sheet.read(cx);
        (
            s.model().selected.iter().copied().collect::<Vec<_>>(),
            s.playhead(cx),
        )
    });
    assert_eq!(
        selected,
        [(3, 2.0f64.to_bits())],
        "the render cache follows the key to row 3"
    );
    assert_eq!(playhead, 1.5);
    assert!(
        m.log.actions.borrow().is_empty(),
        "host writes are not actions"
    );
    assert!(m.events.borrow().is_empty(), "host writes emit no events");
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn keyboard_only_flow(cx: &mut TestAppContext) {
    let mut m = mount(cx, true);
    m.vcx.update(|window, cx| {
        let handle = gpui_kit::Focusable::focus_handle(m.sheet.read(cx), cx);
        handle.focus(window, cx);
    });
    // Row cursor to the value row (already 0), step the playhead to 0.5, insert there.
    m.vcx.simulate_keystrokes(". . . . . k");
    let inserted = m
        .doc()
        .channel(ChannelId(10))
        .and_then(|c| c.curve.at_time(0.5).copied());
    assert!(inserted.is_some(), "inserted at the playhead");
    // Select it with "]" from just before it, cycle its segment's interpolation, delete it.
    m.vcx.simulate_keystrokes(", [");
    let selected: Vec<KeyRef> = m.doc().selection.iter().copied().collect();
    assert_eq!(selected.len(), 1);
    let key = selected[0];
    assert_eq!(
        m.doc().key(key).map(|k| k.key.t),
        Some(0.0),
        "[ selects the previous key"
    );
    m.vcx.simulate_keystrokes("] i");
    let key = m.doc().selection.iter().copied().next().expect("selected");
    assert_eq!(
        m.doc().key(key).map(|k| k.key.interp),
        Some(Interp::Bezier),
        "linear -> bezier"
    );
    m.vcx.simulate_keystrokes("delete");
    assert!(m.doc().key(key).is_none());
    // Down to the toggle row: K toggles at the playhead.
    m.vcx.simulate_keystrokes("down k");
    assert_eq!(m.vcx.update(|_, cx| m.sheet.read(cx).cursor_row()), 1);
    let on = m
        .doc()
        .channel(ChannelId(20))
        .is_some_and(|c| c.curve.is_on(0.5));
    assert!(on, "toggled on at the playhead");
    let names = m.names();
    assert!(names.iter().all(|n| !n.is_empty()));
    assert!(
        m.log
            .actions
            .borrow()
            .iter()
            .all(|(s, _)| *s == Source::Keyboard)
    );
    assert_eq!(
        names.iter().filter(|n| *n == "insert_key").count(),
        1,
        "{names:?}"
    );
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn an_expanded_channel_is_a_curve_lane_over_the_same_keys(cx: &mut TestAppContext) {
    let mut m = mount(cx, false);
    m.vcx.update(|_, cx| {
        m.sheet
            .update(cx, |s, cx| s.set_expanded(ChannelId(10), true, cx))
    });
    m.vcx.run_until_parked();
    let lane = m
        .vcx
        .debug_bounds("curve-body")
        .expect("a curve lane under the value row");
    let body = m.body();
    assert!(
        lane.origin.y >= body.origin.y + px(ROW_H - 1.),
        "below its row"
    );
    assert!(lane.size.height > px(ROW_H * 2.), "taller than a row");
    // An edit in the curve lane commits one SetCurve on the same store slice.
    let before = m.events.borrow().len();
    let (store, sheet) = (m.store.clone(), m.sheet.clone());
    let lane_curve = m.vcx.update(|_, cx| {
        let doc = store.state(cx);
        let keys = doc
            .channel(ChannelId(10))
            .map(|c| c.curve.keys().len())
            .unwrap_or(0);
        let _ = sheet;
        keys
    });
    assert_eq!(lane_curve, 3);
    m.vcx.update(|_, cx| {
        m.sheet.update(cx, |s, cx| {
            let mut keys: Vec<guiraffe_timeline::Key> = s
                .doc(cx)
                .channel(ChannelId(10))
                .map(|c| c.curve.keys().iter().map(|k| k.key).collect())
                .unwrap_or_default();
            keys[2].v = 1.0;
            s.perform(
                DopeAction::SetCurve {
                    channel: ChannelId(10),
                    keys,
                },
                cx,
            )
            .ok();
        })
    });
    m.vcx.run_until_parked();
    assert_eq!(
        m.doc().key(KeyRef::new(10, 3)).map(|k| k.key.v),
        Some(1.0),
        "ids kept"
    );
    assert_eq!(m.events.borrow().len(), before + 1);
}

/// The drawn mini-curve sits where `value_at` says, sampled from rendered pixels.
#[test]
fn value_at_matches_the_drawn_segments() {
    let mut s = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .with_setup(gpui_kit::component::init);
    let width = 600.0;
    let length = 3.0;
    let mut bez = DopeKey::new(2, 1.0, 10.0, Interp::Bezier);
    bez.key.out_handle = (0.3, 0.0);
    let keys = vec![
        DopeKey::new(1, 0.0, 0.0, Interp::Linear),
        bez,
        {
            let mut k = DopeKey::new(3, 2.0, 2.0, Interp::EaseInOut);
            k.key.in_handle = (-0.3, 0.0);
            k
        },
        DopeKey::new(4, 3.0, 8.0, Interp::Linear),
    ];
    let channel = DopeChannel::value(1, "Curve", keys, bounds());
    let curve = channel.curve.clone();
    let img = s
        .render(size(px(width), px(80.)), move |_, cx| {
            apply(
                GuiraffeTheme {
                    scheme: Scheme::Light,
                    ..Default::default()
                },
                cx,
            );
            guiraffe_timeline::init(cx);
            let store = dope_store(DopeDoc::new(vec![channel]), 10, cx);
            let sheet = DopeSheet::with_store(store, TimeFormat::Seconds, length, cx);
            cx.new(|_| Root(sheet))
        })
        .unwrap_or_else(|e| panic!("{e:#}"));
    // The sheet fits 0..length with a 3% margin into the body width.
    let mut axis = Axis::new(0.0, length, f64::from(width - NAME_W));
    axis.fit(0.0, length, 0.03);
    let scale = img.width() as f64 / f64::from(width);
    let ink = |p: &image::Rgba<u8>| p[0] > 180 && p[1] > 120 && p[2] < 110;
    for t in [0.3, 0.6, 1.3, 1.6, 2.4, 2.7] {
        let v = curve.value_at(t);
        let x = ((f64::from(NAME_W) + axis.x(t)) * scale).round() as u32;
        let y = (f64::from(RULER_H) + 4.0 + (1.0 - v / 10.0) * f64::from(ROW_H - 8.0)) * scale;
        let hit = (0..img.height())
            .filter(|row| ink(img.get_pixel(x, *row)))
            .map(f64::from);
        let nearest = hit.map(|row| (row - y).abs()).fold(f64::INFINITY, f64::min);
        assert!(
            nearest <= 2.5 * scale,
            "t={t}: drawn {nearest} device px from value_at"
        );
    }
}

#[test]
fn constant_linear_and_bezier_segments_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(gpui_kit::component::init);
        let img = s
            .render(size(px(640.), px(150.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                guiraffe_timeline::init(cx);
                let mut bez = DopeKey::new(4, 2.0, 2.0, Interp::Bezier);
                bez.key.out_handle = (0.4, 6.0);
                let doc = DopeDoc::new(vec![
                    DopeChannel::value(
                        1,
                        "Segments",
                        vec![
                            DopeKey::new(1, 0.0, 2.0, Interp::Constant),
                            DopeKey::new(2, 1.0, 8.0, Interp::Linear),
                            bez,
                            DopeKey::new(5, 3.0, 9.0, Interp::Linear),
                        ],
                        bounds(),
                    ),
                    DopeChannel::toggle(
                        2,
                        "Bypass",
                        vec![(KeyId(6), 0.5, true), (KeyId(7), 2.5, false)],
                    ),
                    DopeChannel::keys(
                        3,
                        "Markers",
                        vec![DopeKey::new(8, 1.5, 0.0, Interp::Linear)],
                    ),
                ]);
                let store = dope_store(doc, 10, cx);
                store.dispatch_now(SetDopeSelection(vec![KeyRef::new(1, 2)]), cx);
                let sheet = DopeSheet::with_store(store, TimeFormat::Seconds, 3.0, cx);
                cx.new(|_| Root(sheet))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "dope-segments/light"
        } else {
            "dope-segments/dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
