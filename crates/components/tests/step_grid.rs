//! StepGrid: pointer, keyboard and bus edits, one undo step per drag, no echo for host
//! writes, and snapshots in both themes.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point,
    Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use guiraffe_components::step_grid::{
    RowId, SetRows, StepAction, StepCell, StepChange, StepDoc, StepGrid, StepGridEvent,
    StepGridOptions, StepRow, step_store,
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
        let step = 22.0 + 2.0;
        b.origin
            + point(
                px(column as f32 * step + 11.),
                px(row as f32 * step + 11. + dy),
            )
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
    cx.update(guiraffe_components::init);
    let mut parts = None;
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let store = step_store(doc(), 50, cx);
        let grid = StepGrid::new(store.clone(), StepGridOptions::default(), cx);
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
