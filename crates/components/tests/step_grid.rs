//! StepGrid: pointer, keyboard and bus edits, one undo step per drag, no echo for host
//! writes, and snapshots in both themes.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point,
    Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use guiraffe_components::step_grid::{
    LevelGesture, RowId, RulerLabel, SetColumns, SetGhosts, SetRows, StepAction, StepCell,
    StepChange, StepDoc, StepFlag, StepGrid, StepGridEvent, StepGridOptions, StepRow, step_store,
};
use guiraffe_core::{
    ColorName, GuiraffeTheme, Scheme, apply,
    protocol::{ActionBus, Source},
};
use guiraffe_state::Store;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Root(Entity<StepGrid>);
impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(12.))
            .child(self.0.clone())
    }
}

fn doc() -> StepDoc {
    StepDoc::new(
        16,
        4,
        vec![
            StepRow::new(10, "Kick", 16)
                .with_color(ColorName::Red)
                .with_cell(0, StepCell::on())
                .with_cell(4, StepCell::on().with_level(0.6))
                .with_cell(8, StepCell::on().flagged())
                .with_cell(12, StepCell::on().with_ratchet(3)),
            StepRow::new(20, "Snare", 16)
                .with_color(ColorName::Blue)
                .with_cell(4, StepCell::on())
                .with_cell(12, StepCell::on().with_probability(0.5)),
            StepRow::new(30, "Hat", 6)
                .with_color(ColorName::Yellow)
                .with_cell(0, StepCell::on().with_level(0.4))
                .with_cell(2, StepCell::on()),
        ],
    )
}

struct Mounted<'a> {
    grid: Entity<StepGrid>,
    store: Store<StepDoc>,
    events: Rc<RefCell<Vec<StepGridEvent>>>,
    vcx: &'a mut VisualTestContext,
}

impl Mounted<'_> {
    fn cell(&mut self, row: usize, column: usize, dy: f32) -> Point<Pixels> {
        let b = self.vcx.debug_bounds("step-cells").expect("cells");
        let g = self.vcx.update(|_, cx| self.grid.read(cx).geometry());
        b.origin + point(px(g.x(column) + 11.), px(g.y(row) + 11. + dy))
    }
    fn doc(&mut self) -> StepDoc {
        self.vcx
            .update(|_, cx| self.store.state(cx).as_ref().clone())
    }
    fn on(&mut self, row: u64, column: usize) -> bool {
        self.doc()
            .row(RowId(row))
            .is_some_and(|r| r.cell(column).on)
    }
    fn changes(&self) -> Vec<StepChange> {
        self.events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                StepGridEvent::Changed(c) => Some(*c),
                _ => None,
            })
            .collect()
    }
}

fn mount(
    cx: &mut TestAppContext,
    register: bool,
) -> (Mounted<'_>, Option<guiraffe_core::protocol::ControlId>) {
    mount_with(cx, register, StepGridOptions::default())
}

fn mount_with(
    cx: &mut TestAppContext,
    register: bool,
    options: StepGridOptions,
) -> (Mounted<'_>, Option<guiraffe_core::protocol::ControlId>) {
    cx.update(guiraffe_components::init);
    let mut parts = None;
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let store = step_store(doc(), 50, cx);
        let grid = StepGrid::new(store.clone(), options, cx);
        let id = register.then(|| StepGrid::register(&grid, "drums", window, cx));
        cx.subscribe(&grid, move |_, _, e: &StepGridEvent, _| {
            ev.borrow_mut().push(*e)
        })
        .detach();
        parts = Some((grid.clone(), store, id));
        Root(grid)
    });
    vcx.simulate_resize(size(px(800.), px(300.)));
    vcx.run_until_parked();
    let (grid, store, id) = parts.expect("mounted");
    (
        Mounted {
            grid,
            store,
            events,
            vcx,
        },
        id,
    )
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn click_toggles_and_a_drag_paints_one_undo_step(cx: &mut TestAppContext) {
    let (mut m, _) = mount(cx, true);
    let p = m.cell(0, 1, 0.);
    m.vcx.simulate_click(p, Modifiers::default());
    m.vcx.run_until_parked();
    assert!(m.on(10, 1), "a click toggles");
    // Drag along the snare row from column 5 to 9: paints them on (5 was off).
    let (a, b) = (m.cell(1, 5, 0.), m.cell(1, 9, 0.));
    m.vcx
        .simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
    let mid = m.cell(1, 7, 0.);
    m.vcx
        .simulate_mouse_move(mid, Some(MouseButton::Left), Modifiers::default());
    m.vcx
        .simulate_mouse_move(b, Some(MouseButton::Left), Modifiers::default());
    assert!(!m.on(20, 7), "a preview until release");
    m.vcx
        .simulate_mouse_up(b, MouseButton::Left, Modifiers::default());
    m.vcx.run_until_parked();
    assert!((5..=9).all(|c| m.on(20, c)));
    assert_eq!(
        m.changes(),
        [
            StepChange::CellToggled {
                row: RowId(10),
                column: 1,
                on: true
            },
            StepChange::CellsPainted {
                row: RowId(20),
                from: 5,
                to: 9,
                on: true
            },
        ]
    );
    m.vcx
        .update(|_, cx| m.store.dispatch(guiraffe_state::UndoEvent, cx));
    m.vcx.run_until_parked();
    assert!((5..=9).all(|c| !m.on(20, c)), "the drag undoes in one step");
    assert!(m.on(10, 1));
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn alt_drag_sets_the_level_and_right_click_inspects(cx: &mut TestAppContext) {
    let (mut m, _) = mount(cx, false);
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let (a, b) = (m.cell(0, 2, 0.), m.cell(0, 2, -11.));
    m.vcx.simulate_mouse_down(a, MouseButton::Left, alt);
    m.vcx.simulate_mouse_move(b, Some(MouseButton::Left), alt);
    m.vcx.simulate_mouse_up(b, MouseButton::Left, alt);
    m.vcx.run_until_parked();
    let cell = m
        .doc()
        .row(RowId(10))
        .map(|r| r.cell(2))
        .unwrap_or_default();
    assert!(cell.on && (cell.level - 0.5).abs() < 0.01, "{cell:?}");
    let p = m.cell(2, 3, 0.);
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
    assert!(m.events.borrow().iter().any(|e| matches!(
        e,
        StepGridEvent::Inspect {
            row: RowId(30),
            column: 3,
            ..
        }
    )));
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn keyboard_moves_toggles_and_inspects(cx: &mut TestAppContext) {
    let (mut m, _) = mount(cx, true);
    let p = m.cell(0, 0, 0.);
    m.vcx.simulate_click(p, Modifiers::default()); // focuses (and toggles cell 0 off)
    m.vcx.run_until_parked();
    m.vcx.simulate_keystrokes("down right right space");
    m.vcx.run_until_parked();
    assert_eq!(m.vcx.update(|_, cx| m.grid.read(cx).cursor()), (1, 2));
    assert!(m.on(20, 2), "space toggles the cursor's cell");
    m.vcx.simulate_keystrokes("enter");
    assert!(m.events.borrow().iter().any(|e| matches!(
        e,
        StepGridEvent::Inspect {
            row: RowId(20),
            column: 2,
            ..
        }
    )));
}

#[gpui_kit::test]
fn bus_actions_and_host_writes(cx: &mut TestAppContext) {
    let (mut m, id) = mount(cx, true);
    let id = id.expect("registered");
    m.vcx.update(|window, cx| {
        for a in [
            StepAction::ToggleCell {
                row: RowId(30),
                column: 1,
            },
            StepAction::SetRowLength {
                row: RowId(30),
                length: 8,
            },
            StepAction::PaintCells {
                row: RowId(10),
                from: 13,
                to: 15,
                on: true,
            },
            StepAction::SetCellLevel {
                row: RowId(20),
                column: 0,
                level: 0.25,
            },
        ] {
            let outcome = ActionBus::dispatch(id, &a.to_action(), Source::Script, window, cx);
            assert!(outcome.is_ok_and(|o| !o.is_empty()), "{a:?}");
        }
    });
    assert_eq!(m.changes().len(), 4);
    assert_eq!(m.doc().row(RowId(30)).map(|r| r.length), Some(8));
    // A host write reorders rows: no events, and state follows the ids.
    let before = m.changes().len();
    let mut rows: Vec<StepRow> = m.doc().rows().iter().map(|r| (**r).clone()).collect();
    rows.reverse();
    m.vcx.update(|_, cx| m.store.dispatch(SetRows(rows), cx));
    m.vcx.run_until_parked();
    assert_eq!(m.doc().rows()[0].id, RowId(30));
    assert!(m.on(30, 1));
    assert_eq!(m.changes().len(), before, "host writes never echo");
}

#[test]
fn step_grid_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(guiraffe_components::init);
        let img = s
            .render(size(px(560.), px(140.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let store = step_store(doc(), 10, cx);
                let grid = StepGrid::new(store, StepGridOptions::default(), cx);
                let feed = grid.read(cx).playhead();
                feed.set(Some(5));
                cx.new(|_| Root(grid))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "step-grid-light"
        } else {
            "step-grid-dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

/// The pointer and the bus give the same document for the same edits.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn bus_drive_matches_the_pointer(cx: &mut TestAppContext) {
    let pointer_rows = {
        let (mut m, _) = mount(cx, true);
        let p = m.cell(2, 1, 0.);
        m.vcx.simulate_click(p, Modifiers::default());
        let (a, b) = (m.cell(1, 6, 0.), m.cell(1, 10, 0.));
        m.vcx
            .simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
        m.vcx
            .simulate_mouse_move(b, Some(MouseButton::Left), Modifiers::default());
        m.vcx
            .simulate_mouse_up(b, MouseButton::Left, Modifiers::default());
        m.vcx.run_until_parked();
        let changes = m.changes();
        (m.doc(), changes)
    };
    let (mut m, id) = mount(cx, true);
    let id = id.expect("registered");
    m.vcx.update(|window, cx| {
        for change in &pointer_rows.1 {
            let action = match *change {
                StepChange::CellToggled { row, column, .. } => {
                    StepAction::ToggleCell { row, column }
                }
                StepChange::CellsPainted { row, from, to, on } => {
                    StepAction::PaintCells { row, from, to, on }
                }
                StepChange::CellLevelChanged { row, column, level } => {
                    StepAction::SetCellLevel { row, column, level }
                }
                StepChange::RowLengthChanged { row, length } => {
                    StepAction::SetRowLength { row, length }
                }
                StepChange::FlagCycled { row, column, .. } => StepAction::CycleFlag { row, column },
                StepChange::OffsetChanged {
                    row,
                    column,
                    offset,
                } => StepAction::SetOffset {
                    row,
                    column,
                    offset,
                },
                StepChange::FlamChanged { row, column, flam } => {
                    StepAction::SetFlam { row, column, flam }
                }
                StepChange::GhostsApplied { row } => StepAction::ApplyGhosts { row },
            };
            assert!(
                ActionBus::dispatch(id, &action.to_action(), Source::Script, window, cx).is_ok()
            );
        }
    });
    assert_eq!(pointer_rows.1.len(), 2);
    assert_eq!(m.doc(), pointer_rows.0, "same document");
    assert_eq!(m.changes(), pointer_rows.1, "same changes");
}

fn cell_of(m: &mut Mounted<'_>, row: u64, column: usize) -> StepCell {
    m.doc()
        .row(RowId(row))
        .map(|r| r.cell(column))
        .unwrap_or_default()
}

fn modifiers(f: impl FnOnce(&mut Modifiers)) -> Modifiers {
    let mut m = Modifiers::default();
    f(&mut m);
    m
}

/// Shift-click cycles emphasis, Ctrl-drag sets micro-timing; the bus gives the same result.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn modifier_gestures_by_pointer_and_bus(cx: &mut TestAppContext) {
    let pointer = {
        let (mut m, _) = mount(cx, true);
        let shift = modifiers(|m| m.shift = true);
        let p = m.cell(0, 0, 0.);
        m.vcx.simulate_click(p, shift);
        m.vcx.run_until_parked();
        assert_eq!(cell_of(&mut m, 10, 0).emphasis(), StepFlag::Accent);
        assert!(cell_of(&mut m, 10, 0).on, "shift-click does not toggle");
        let ctrl = modifiers(|m| m.control = true);
        let (a, b) = (m.cell(0, 4, 0.), m.cell(0, 4, 0.) + point(px(5.5), px(0.)));
        m.vcx.simulate_mouse_down(a, MouseButton::Left, ctrl);
        m.vcx.simulate_mouse_move(b, Some(MouseButton::Left), ctrl);
        m.vcx.simulate_mouse_up(b, MouseButton::Left, ctrl);
        m.vcx.run_until_parked();
        assert!((cell_of(&mut m, 10, 4).offset - 0.25).abs() < 1e-4);
        (m.doc(), m.changes())
    };
    assert_eq!(
        pointer.1,
        [
            StepChange::FlagCycled {
                row: RowId(10),
                column: 0,
                flag: StepFlag::Accent
            },
            StepChange::OffsetChanged {
                row: RowId(10),
                column: 4,
                offset: 0.25
            },
        ]
    );
    let (mut m, id) = mount(cx, true);
    let id = id.expect("registered");
    m.vcx.update(|window, cx| {
        for a in [
            StepAction::CycleFlag {
                row: RowId(10),
                column: 0,
            },
            StepAction::SetOffset {
                row: RowId(10),
                column: 4,
                offset: 0.25,
            },
        ] {
            assert!(ActionBus::dispatch(id, &a.to_action(), Source::Mcp, window, cx).is_ok());
        }
    });
    assert_eq!(m.doc(), pointer.0);
    assert_eq!(m.changes(), pointer.1);
}

/// The host hook sees every press first and can take it.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn the_press_hook_can_handle_a_press(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let options = StepGridOptions {
        on_press: Some(Rc::new(move |press, _, _| {
            log.borrow_mut()
                .push((press.row, press.column, press.modifiers.platform));
            press.modifiers.platform
        })),
        ..StepGridOptions::default()
    };
    let (mut m, _) = mount_with(cx, true, options);
    let tool = modifiers(|m| m.platform = true);
    let p = m.cell(1, 2, 0.);
    m.vcx.simulate_click(p, tool);
    m.vcx.run_until_parked();
    assert!(!m.on(20, 2), "the host took it");
    m.vcx.simulate_click(p, Modifiers::default());
    m.vcx.run_until_parked();
    assert!(m.on(20, 2), "not taken: the grid toggles");
    assert_eq!(
        *seen.borrow(),
        [(RowId(20), 2, true), (RowId(20), 2, false)]
    );
}

/// With `LevelGesture::Absolute` an Alt-click sets the level from the press height.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn absolute_level_is_set_by_the_press_height(cx: &mut TestAppContext) {
    let options = StepGridOptions {
        level_gesture: LevelGesture::Absolute,
        ..StepGridOptions::default()
    };
    let (mut m, _) = mount_with(cx, true, options);
    let alt = modifiers(|m| m.alt = true);
    // 5.5 px below the cell's top: three quarters up.
    let p = m.cell(1, 7, -5.5);
    m.vcx.simulate_click(p, alt);
    m.vcx.run_until_parked();
    let cell = cell_of(&mut m, 20, 7);
    assert!(cell.on && (cell.level - 0.75).abs() < 0.01, "{cell:?}");
}

/// A drag commits when the button comes up outside the cells, or when a move arrives
/// with the button already up.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn a_drag_released_outside_the_cells_commits(cx: &mut TestAppContext) {
    let (mut m, _) = mount(cx, true);
    let (a, b) = (m.cell(1, 5, 0.), m.cell(1, 8, 0.));
    let outside = m.cell(1, 8, 200.);
    m.vcx
        .simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
    m.vcx
        .simulate_mouse_move(b, Some(MouseButton::Left), Modifiers::default());
    m.vcx
        .simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::default());
    m.vcx
        .simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
    m.vcx.run_until_parked();
    assert!((5..=8).all(|c| m.on(20, c)), "committed on release outside");
    // The button comes up where the grid can't see it: the next move commits.
    let (a, b) = (m.cell(0, 13, 0.), m.cell(0, 15, 0.));
    m.vcx
        .simulate_mouse_down(a, MouseButton::Left, Modifiers::default());
    m.vcx
        .simulate_mouse_move(b, Some(MouseButton::Left), Modifiers::default());
    m.vcx.simulate_mouse_move(b, None, Modifiers::default());
    m.vcx.run_until_parked();
    assert!((13..=15).all(|c| m.on(10, c)));
    assert_eq!(m.changes().len(), 2);
}

/// Hover events, group gaps in hit-testing, and the cells' bounds.
#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn hover_group_gaps_and_cells_bounds(cx: &mut TestAppContext) {
    let options = StepGridOptions {
        group_gap: 8.0,
        ..StepGridOptions::default()
    };
    let (mut m, _) = mount_with(cx, true, options);
    let bounds = m.vcx.update(|_, cx| m.grid.read(cx).cells_bounds());
    assert_eq!(Some(bounds), m.vcx.debug_bounds("step-cells"));
    // Column 4 starts a new group: 8 px further right than without gaps.
    let g = m.vcx.update(|_, cx| m.grid.read(cx).geometry());
    assert_eq!(g.x(4), 4.0 * 24.0 + 8.0);
    let p = m.cell(1, 5, 0.);
    m.vcx.simulate_click(p, Modifiers::default());
    m.vcx.run_until_parked();
    assert!(
        m.on(20, 5),
        "the click lands on column 5 past the group gap"
    );
    let hovers = |m: &Mounted<'_>| -> Vec<Option<(RowId, usize)>> {
        m.events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                StepGridEvent::Hover(h) => Some(*h),
                _ => None,
            })
            .collect()
    };
    let before = hovers(&m).len();
    let over = m.cell(2, 9, 0.);
    m.vcx.simulate_mouse_move(over, None, Modifiers::default());
    m.vcx.simulate_mouse_move(over, None, Modifiers::default());
    let away = m.cell(2, 9, 300.);
    m.vcx.simulate_mouse_move(away, None, Modifiers::default());
    m.vcx.run_until_parked();
    assert_eq!(
        hovers(&m)[before..],
        [Some((RowId(30), 9)), None],
        "one event per change"
    );
}

/// Host writes: ghost cells and column changes round-trip without events; applying the
/// ghosts over the bus is one user edit.
#[gpui_kit::test]
fn ghosts_and_columns_by_store_and_bus(cx: &mut TestAppContext) {
    let (mut m, id) = mount(cx, true);
    let id = id.expect("registered");
    let draft = vec![
        StepCell::default(),
        StepCell::on(),
        StepCell::on().ghost_note(),
    ];
    m.vcx.update(|_, cx| {
        m.store.dispatch(
            SetGhosts {
                row: RowId(30),
                cells: Some(draft.clone()),
            },
            cx,
        );
        m.store.dispatch(
            SetColumns {
                columns: 32,
                group: 8,
            },
            cx,
        );
    });
    m.vcx.run_until_parked();
    let doc = m.doc();
    assert_eq!(doc.ghosts(RowId(30)), Some(draft.as_slice()));
    assert_eq!((doc.columns, doc.group), (32, 8));
    assert!(m.changes().is_empty(), "host writes never echo");
    m.vcx.update(|window, cx| {
        let apply = StepAction::ApplyGhosts { row: RowId(30) }.to_action();
        assert!(ActionBus::dispatch(id, &apply, Source::Mcp, window, cx).is_ok());
    });
    assert!(m.on(30, 1) && !m.on(30, 0) && cell_of(&mut m, 30, 2).soft);
    assert_eq!(m.doc().ghosts(RowId(30)), None);
    assert_eq!(m.changes(), [StepChange::GhostsApplied { row: RowId(30) }]);
    m.vcx.update(|_, cx| {
        m.store.dispatch(
            SetColumns {
                columns: 8,
                group: 4,
            },
            cx,
        )
    });
    m.vcx.run_until_parked();
    assert!(m.doc().rows().iter().all(|r| r.length <= 8));
}

/// A drum-editor grid: group gaps, bar and step ruler labels, offset ticks, flams, ghost
/// notes, a ghost layer, and a muted row while another is soloed.
#[test]
fn step_grid_drum_editor_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(guiraffe_components::init);
        let img = s
            .render(size(px(600.), px(170.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let mut snare = StepRow::new(20, "Snare", 16)
                    .with_color(ColorName::Blue)
                    .with_cell(4, StepCell::on().with_flam())
                    .with_cell(7, StepCell::on().ghost_note().with_level(0.5))
                    .with_cell(12, StepCell::on().with_offset(0.3));
                snare.soloed = true;
                let mut hat = StepRow::new(30, "Hat", 16)
                    .with_color(ColorName::Yellow)
                    .with_cell(2, StepCell::on())
                    .with_cell(6, StepCell::on());
                hat.muted = true;
                let doc = StepDoc::new(
                    16,
                    4,
                    vec![
                        StepRow::new(10, "Kick", 16)
                            .with_color(ColorName::Red)
                            .with_cell(0, StepCell::on().flagged())
                            .with_cell(5, StepCell::on().with_offset(-0.25))
                            .with_cell(8, StepCell::on()),
                        snare,
                        hat,
                    ],
                );
                let store = step_store(doc, 10, cx);
                store.dispatch(
                    SetGhosts {
                        row: RowId(10),
                        cells: Some(
                            (0..16)
                                .map(|c| {
                                    if c % 4 == 2 {
                                        StepCell::on()
                                    } else {
                                        StepCell::default()
                                    }
                                })
                                .collect(),
                        ),
                    },
                    cx,
                );
                let options = StepGridOptions {
                    group_gap: 6.0,
                    ruler_label: Some(Rc::new(|c| {
                        Some(if c % 4 == 0 {
                            RulerLabel {
                                text: (c / 4 + 1).to_string().into(),
                                strong: true,
                            }
                        } else {
                            RulerLabel {
                                text: (c % 4 + 1).to_string().into(),
                                strong: false,
                            }
                        })
                    })),
                    ..StepGridOptions::default()
                };
                let grid = StepGrid::new(store, options, cx);
                cx.new(|_| Root(grid))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "step-grid-drum-light"
        } else {
            "step-grid-drum-dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
