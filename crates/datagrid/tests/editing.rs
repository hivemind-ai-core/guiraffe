#![allow(clippy::single_range_in_vec_init)]
//! Record editing driven by real keystrokes and pointer events: type-to-edit, commit-and-move,
//! Esc, range extension (keys and drag autoscroll), paste tiling with rejections, cut, delete
//! and undo/redo. Run on both backends (`--features gpui-fast`).

use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::{
    ClipboardItem, Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement,
    Point, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use guiraffe_datagrid::{
    CellPos, CellRef, ColumnDef, DataGrid, EditError, EditOp, FnSource, GridEvent, GridHandle,
    GridOptions, SelectionMode, VecSource,
};

struct Host {
    grid: GridHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "grid".into())
            .size_full()
            .child(DataGrid::new(&self.grid))
    }
}

type Events = Rc<RefCell<Vec<GridEvent>>>;

fn mount(
    cx: &mut TestAppContext,
    make: impl FnOnce(&mut gpui_kit::App) -> GridHandle,
) -> (GridHandle, Events, &mut VisualTestContext) {
    let events: Events = Rc::default();
    let ev = events.clone();
    let mut handle = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let grid = make(cx);
        cx.subscribe(grid.events(), move |_, _, e: &GridEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach();
        handle = Some(grid.clone());
        Host { grid }
    });
    vcx.simulate_resize(size(px(1000.), px(600.)));
    vcx.run_until_parked();
    let grid = handle.unwrap_or_else(|| panic!("grid not created"));
    (grid, events, vcx)
}

fn at(grid: &GridHandle, row: usize, col: usize) -> Point<gpui_kit::Pixels> {
    let b = grid.bounds();
    let h = grid.row_height();
    let (sx, sy) = grid.scroll_offset();
    let l = grid.layout();
    let x = l.screen_x(col, sx, f32::from(b.size.width)).unwrap_or(0.)
        + l.display(col).map_or(0., |(_, p)| p.width / 2.);
    let y = h + (row as f64 * f64::from(h) - sy) as f32 + h / 2.;
    b.origin + point(px(x), px(y))
}

fn click(vcx: &mut VisualTestContext, p: Point<gpui_kit::Pixels>) {
    vcx.simulate_click(p, Modifiers::default());
    vcx.run_until_parked();
}

/// Six people; name is free text, age must parse as a number; a third read-only column.
fn people() -> VecSource<(String, i64)> {
    VecSource::new(
        ["Carol", "alice", "Bob", "Dave", "Eve", "Finn"]
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), 20 + i as i64))
            .collect(),
    )
    .editable_column(
        |p: &(String, i64)| CellRef::Text(&p.0),
        |p, t| {
            p.0 = t.to_string();
            Ok(())
        },
    )
    .editable_column(
        |p: &(String, i64)| CellRef::Int(p.1),
        |p, t| {
            p.1 = t
                .trim()
                .parse()
                .map_err(|_| EditError::Invalid(format!("{t:?} is not a number")))?;
            Ok(())
        },
    )
}

fn editable_grid(cx: &mut gpui_kit::App) -> GridHandle {
    let g = GridHandle::new(
        people(),
        vec![
            ColumnDef::new("name", "Name", 0).editable(true),
            ColumnDef::new("age", "Age", 1).editable(true),
            ColumnDef::new("ro", "Read-only", 1),
        ],
        cx,
    );
    g.set_options(
        GridOptions {
            selection: SelectionMode::Range,
            focusable: Some(true),
            ..Default::default()
        },
        cx,
    );
    g
}

/// Source cell text (source row, source column).
fn cell(grid: &GridHandle, row: usize, col: usize) -> String {
    grid.update_source(|s| {
        let mut out = String::new();
        s.cell(row, col).write_display(&mut out);
        out
    })
    .unwrap_or_else(|| panic!("source shared"))
}

fn column_text(grid: &GridHandle, col: usize) -> Vec<String> {
    (0..6).map(|r| cell(grid, r, col)).collect()
}

fn editor_value(grid: &GridHandle, vcx: &mut VisualTestContext) -> Option<String> {
    let input = grid.editor()?;
    Some(vcx.update(|_, cx| input.read(cx).value().to_string()))
}

#[gpui_kit::test]
fn type_to_edit_and_enter_moves(cx: &mut TestAppContext) {
    let (grid, events, vcx) = mount(cx, editable_grid);
    click(vcx, at(&grid, 0, 0));
    vcx.simulate_keystrokes("Z");
    assert!(grid.is_editing(), "a printable key opens the editor");
    assert_eq!(editor_value(&grid, vcx).as_deref(), Some("Z"), "replaces");
    vcx.simulate_keystrokes("e d");
    assert_eq!(editor_value(&grid, vcx).as_deref(), Some("Zed"));
    vcx.simulate_keystrokes("enter");
    assert!(!grid.is_editing());
    assert_eq!(cell(&grid, 0, 0), "Zed");
    assert_eq!(
        grid.selection().cursor,
        Some(CellPos::new(1, 0)),
        "moved down"
    );
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        GridEvent::CellEdited { row: 0, column: 0, value, .. } if value.as_ref() == "Zed"
    )));

    // F2 edits in place with the caret at the end; typing appends.
    vcx.simulate_keystrokes("f2");
    assert_eq!(editor_value(&grid, vcx).as_deref(), Some("alice"));
    let input = grid.editor().unwrap_or_else(|| panic!("editor"));
    vcx.update(|_, cx| assert_eq!(input.read(cx).cursor(), 5, "caret at end"));
    vcx.simulate_keystrokes("s");
    // Shift+Enter commits and moves up.
    vcx.simulate_keystrokes("shift-enter");
    assert_eq!(cell(&grid, 1, 0), "alices");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(0, 0)));

    // Enter on a cell edits in place too.
    vcx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&grid, vcx).as_deref(), Some("Zed"));
    vcx.simulate_keystrokes("escape");
    // Ctrl-chords never start an edit.
    vcx.simulate_keystrokes("ctrl-q");
    assert!(!grid.is_editing());
}

#[gpui_kit::test]
fn tab_wraps_and_escape_restores(cx: &mut TestAppContext) {
    let (grid, events, vcx) = mount(cx, editable_grid);
    click(vcx, at(&grid, 0, 2));
    // Tab moves (not editing) and wraps at the row end.
    vcx.simulate_keystrokes("tab");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(1, 0)), "wrapped");
    vcx.simulate_keystrokes("shift-tab");
    assert_eq!(
        grid.selection().cursor,
        Some(CellPos::new(0, 2)),
        "wrapped back"
    );
    vcx.simulate_keystrokes("shift-tab");
    // Editing: Tab commits and moves right.
    vcx.simulate_keystrokes("5 tab");
    assert_eq!(cell(&grid, 0, 1), "5");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(0, 2)));
    // Shift+Tab from the first column wraps to the previous row's last column.
    click(vcx, at(&grid, 2, 0));
    vcx.simulate_keystrokes("Q shift-tab");
    assert_eq!(cell(&grid, 2, 0), "Q");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(1, 2)));
    // A vetoed commit stays put.
    click(vcx, at(&grid, 3, 1));
    vcx.simulate_keystrokes("x tab");
    assert!(grid.is_editing(), "age 'x' rejected: editor stays open");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(3, 1)));
    // Esc cancels and restores the old value.
    vcx.simulate_keystrokes("escape");
    assert!(!grid.is_editing());
    assert_eq!(cell(&grid, 3, 1), "23");
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, GridEvent::EditCancelled { row: 3, column: 1 }))
    );
}

#[gpui_kit::test]
fn arrows_commit_at_the_text_edge(cx: &mut TestAppContext) {
    let (grid, _, vcx) = mount(cx, editable_grid);
    click(vcx, at(&grid, 0, 0));
    vcx.simulate_keystrokes("f2 left");
    assert!(grid.is_editing(), "Left inside the text moves the caret");
    vcx.simulate_keystrokes("right");
    assert!(grid.is_editing(), "caret back at the end");
    vcx.simulate_keystrokes("! right");
    assert!(!grid.is_editing(), "Right at the end commits");
    assert_eq!(cell(&grid, 0, 0), "Carol!");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(0, 1)));
    vcx.simulate_keystrokes("9 down");
    assert_eq!(cell(&grid, 0, 1), "9");
    assert_eq!(
        grid.selection().cursor,
        Some(CellPos::new(1, 1)),
        "Down commits"
    );
    vcx.simulate_keystrokes("f2 home left");
    assert!(!grid.is_editing(), "Left at the start commits");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(1, 0)));
    vcx.simulate_keystrokes("f2 up");
    assert_eq!(
        grid.selection().cursor,
        Some(CellPos::new(0, 0)),
        "Up commits"
    );
}

fn million(cx: &mut gpui_kit::App) -> GridHandle {
    let g = GridHandle::new(
        FnSource::new(1_000_000, |r, c| CellRef::Int((r * 100 + c) as i64)),
        (0..30)
            .map(|c| ColumnDef::new(format!("c{c}"), format!("Col {c}"), c).width(100.))
            .collect(),
        cx,
    );
    g.set_options(
        GridOptions {
            selection: SelectionMode::Range,
            focusable: Some(true),
            ..Default::default()
        },
        cx,
    );
    g
}

#[gpui_kit::test]
fn shift_arrows_shift_click_and_drag_autoscroll(cx: &mut TestAppContext) {
    let (grid, _, vcx) = mount(cx, million);
    click(vcx, at(&grid, 1, 1));
    vcx.simulate_keystrokes("shift-down shift-down shift-right");
    assert_eq!(grid.selection().rect(), Some((1..4, 1..3)));
    vcx.simulate_click(
        at(&grid, 5, 4),
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    vcx.run_until_parked();
    assert_eq!(grid.selection().rect(), Some((1..6, 1..5)), "Shift+click");

    // Drag from (2, 0) to the bottom edge: the range extends and the grid scrolls.
    let from = at(&grid, 2, 0);
    let b = grid.bounds();
    let bottom = b.origin + point(px(50.), b.size.height - px(2.));
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(bottom, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let (_, y1) = grid.scroll_offset();
    assert!(y1 > 0., "a move into the edge zone scrolls");
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    let (_, y2) = grid.scroll_offset();
    assert!(y2 > y1, "holding at the edge keeps scrolling: {y1} -> {y2}");
    // Past the grid (outside its bounds) still extends.
    let below = bottom + point(px(0.), px(40.));
    vcx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    vcx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let (_, y3) = grid.scroll_offset();
    let (rows, cols) = grid.selection().rect().unwrap_or((0..0, 0..0));
    assert_eq!((rows.start, cols), (2, 0..1));
    let last = grid.visible_rows().end;
    assert!(
        rows.end > 20 && rows.end + 5 >= last,
        "range follows: {rows:?} {last}"
    );
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    assert_eq!(grid.scroll_offset().1, y3, "released: autoscroll stops");
}

fn set_clipboard(vcx: &mut VisualTestContext, text: &str) {
    vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())));
}

fn select(grid: &GridHandle, vcx: &mut VisualTestContext, a: (usize, usize), b: (usize, usize)) {
    vcx.update(|_, cx| {
        grid.select(CellPos::new(a.0, a.1), false, false, cx);
        grid.select(CellPos::new(b.0, b.1), true, false, cx);
    });
}

#[gpui_kit::test]
fn paste_tiles_rejects_one_and_undoes_as_one_step(cx: &mut TestAppContext) {
    let (grid, events, vcx) = mount(cx, editable_grid);
    grid.set_validator(|r| {
        if r.row == 3 && r.column == 1 {
            Err("row 3 age is locked".into())
        } else {
            Ok(())
        }
    });
    click(vcx, at(&grid, 0, 0));
    let names = column_text(&grid, 0);
    let ages = column_text(&grid, 1);
    // A 2×2 block divides the 4×2 selection: tiled.
    select(&grid, vcx, (0, 0), (3, 1));
    set_clipboard(vcx, "A\t1\nB\t2\n");
    vcx.simulate_keystrokes("ctrl-v");
    assert_eq!(column_text(&grid, 0)[..4], ["A", "B", "A", "B"]);
    assert_eq!(
        column_text(&grid, 1)[..4],
        ["1", "2", "1", "23"],
        "row 3 vetoed"
    );
    assert_eq!(column_text(&grid, 0)[4..], names[4..], "outside untouched");
    let ev = events.borrow().clone();
    let rejections: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            GridEvent::CellsRejected {
                op,
                rejected,
                applied,
                first,
            } => Some((*op, *rejected, *applied, first.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(rejections.len(), 1, "one summary event");
    let (op, rejected, applied, first) = &rejections[0];
    assert_eq!((*op, *rejected, *applied), (EditOp::Paste, 1, 7));
    assert_eq!((first[0].row, first[0].column), (Some(3), Some(1)));
    assert_eq!(first[0].reason.as_ref(), "row 3 age is locked");
    assert!(ev.contains(&GridEvent::CellsChanged {
        op: EditOp::Paste,
        cells: 7,
        truncated: false
    }));

    // Undo restores the whole paste in one step; redo re-applies it.
    vcx.simulate_keystrokes("ctrl-z");
    assert_eq!(column_text(&grid, 0), names);
    assert_eq!(column_text(&grid, 1), ages);
    assert!(!grid.can_undo());
    vcx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(column_text(&grid, 0)[..4], ["A", "B", "A", "B"]);
    vcx.simulate_keystrokes("ctrl-z ctrl-y");
    assert_eq!(column_text(&grid, 1)[..4], ["1", "2", "1", "23"]);
    vcx.simulate_keystrokes("ctrl-z");

    // Not dividing: pasted once at the top-left; clipped to editable columns and rows.
    select(&grid, vcx, (4, 0), (4, 0));
    set_clipboard(vcx, "X\t7\t99\nY\t8\t99\nZ\t9\t99\nW\t1\t1");
    vcx.simulate_keystrokes("ctrl-v");
    assert_eq!(column_text(&grid, 0)[4..], ["X", "Y"], "clipped to rows");
    assert_eq!(
        column_text(&grid, 1)[4..],
        ["7", "8"],
        "read-only col skipped"
    );
    assert_eq!(
        grid.selection().rect(),
        Some((4..6, 0..3)),
        "pasted area selected"
    );
    // A single value fills the whole range.
    select(&grid, vcx, (0, 1), (2, 1));
    set_clipboard(vcx, "50");
    vcx.simulate_keystrokes("ctrl-v");
    assert_eq!(column_text(&grid, 1)[..3], ["50", "50", "50"]);
}

#[gpui_kit::test]
fn cut_and_delete_clear_editable_cells(cx: &mut TestAppContext) {
    let (grid, events, vcx) = mount(cx, editable_grid);
    click(vcx, at(&grid, 0, 0));
    vcx.simulate_keystrokes("shift-down");
    vcx.simulate_keystrokes("ctrl-x");
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("Carol\nalice")
        );
    });
    assert_eq!(column_text(&grid, 0)[..3], ["", "", "Bob"]);
    // Delete over name + age + read-only: names clear, empty ages are vetoed by the source,
    // the read-only column is skipped.
    select(&grid, vcx, (2, 0), (3, 2));
    vcx.simulate_keystrokes("delete");
    assert_eq!(column_text(&grid, 0)[2..4], ["", ""]);
    assert_eq!(column_text(&grid, 1)[2..4], ["22", "23"]);
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        GridEvent::CellsRejected {
            op: EditOp::Clear,
            rejected: 2,
            applied: 2,
            ..
        }
    )));
    vcx.simulate_keystrokes("backspace");
    // Undo the clear, then the cut: each is one step.
    vcx.simulate_keystrokes("ctrl-z");
    assert_eq!(column_text(&grid, 0)[..4], ["", "", "Bob", "Dave"]);
    vcx.simulate_keystrokes("ctrl-z");
    assert_eq!(column_text(&grid, 0)[..2], ["Carol", "alice"]);
    // An edit is one undo step too.
    click(vcx, at(&grid, 5, 0));
    vcx.simulate_keystrokes("N enter ctrl-z");
    assert_eq!(cell(&grid, 5, 0), "Finn");
}

#[gpui_kit::test]
fn keys_need_focus_opt_in(cx: &mut TestAppContext) {
    let (grid, _, vcx) = mount(cx, |cx| {
        let g = editable_grid(cx);
        let mut o = g.options();
        o.focusable = None;
        g.set_options(o, cx);
        g
    });
    click(vcx, at(&grid, 0, 0));
    vcx.simulate_keystrokes("Z delete");
    assert!(!grid.is_editing(), "an unfocused grid ignores keys");
    assert_eq!(cell(&grid, 0, 0), "Carol");
}
