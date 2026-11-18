#![allow(clippy::single_range_in_vec_init, clippy::let_and_return)]
//! GPUI interaction tests: scrolling, selection, keyboard, resize, sort, edit, focus policy,
//! accessibility and render isolation. Run on both backends (`--features gpui-fast`).

use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui_kit::{
    Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render,
    Role, ScrollDelta, ScrollEvent, Styled, TestAppContext, TouchPhase, VisualTestContext, Window,
    div, point, px, size,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_datagrid::{
    CellPos, CellRef, ColumnDef, DataGrid, EditError, FnSource, GridEvent, GridHandle, GridOptions,
    GridSource, Pin, SelectionMode, SortDir, SortKey, VecSource,
};
use guiraffe_test::render::RenderCounter;

struct Host {
    grid: GridHandle,
    renders: RenderCounter,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        div()
            .debug_selector(|| "grid".into())
            .size_full()
            .child(DataGrid::new(&self.grid))
    }
}

fn columns(n: usize) -> Vec<ColumnDef> {
    (0..n)
        .map(|c| ColumnDef::new(format!("c{c}"), format!("Col {c}"), c).width(100.))
        .collect()
}

/// `cell(r, c) = r * 100 + c`, so every cell label is unique.
fn million() -> impl GridSource {
    FnSource::new(1_000_000, |r, c| CellRef::Int((r * 100 + c) as i64))
}

type Events = Rc<RefCell<Vec<GridEvent>>>;

fn mount(
    cx: &mut TestAppContext,
    make: impl FnOnce(&mut gpui_kit::App) -> GridHandle,
) -> (GridHandle, RenderCounter, Events, &mut VisualTestContext) {
    let renders = RenderCounter::new();
    let r = renders.clone();
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
        Host { grid, renders: r }
    });
    vcx.simulate_resize(size(px(1000.), px(600.)));
    vcx.run_until_parked();
    let grid = handle.unwrap_or_else(|| panic!("grid not created"));
    (grid, renders, events, vcx)
}

/// Window position of the center of (view row, display col); header when `row` is None.
fn at(grid: &GridHandle, row: Option<usize>, col: usize) -> Point<gpui_kit::Pixels> {
    let b = grid.bounds();
    let h = grid.row_height();
    let (sx, sy) = grid.scroll_offset();
    let l = grid.layout();
    let x = l.screen_x(col, sx, f32::from(b.size.width)).unwrap_or(0.)
        + l.display(col).map_or(0., |(_, p)| p.width / 2.);
    let y = match row {
        None => h / 2.,
        Some(r) => h + (r as f64 * f64::from(h) - sy) as f32 + h / 2.,
    };
    b.origin + point(px(x), px(y))
}

fn click(vcx: &mut VisualTestContext, p: Point<gpui_kit::Pixels>, m: Modifiers) {
    vcx.simulate_click(p, m);
    vcx.run_until_parked();
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

#[gpui_kit::test]
fn scrolling_ten_million_rows_shows_the_right_rows(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| {
        GridHandle::new(
            FnSource::new(10_000_000, |r, c| CellRef::Int((r * 100 + c) as i64)),
            columns(50),
            cx,
        )
    });
    assert_eq!(grid.view_row_count(), 10_000_000);
    let stats = grid.stats();
    assert!(stats.pool < 40, "pool is O(viewport): {}", stats.pool);
    assert!(
        stats.visible_cells <= 12,
        "only visible columns built: {}",
        stats.visible_cells
    );
    vcx.update(|w, _| assert!(w.try_find_by_label("0").is_some()));

    vcx.update(|_, cx| grid.scroll_to_row(9_999_999, cx));
    vcx.run_until_parked();
    let vis = grid.visible_rows();
    assert_eq!(vis.end, 10_000_000);
    vcx.update(|w, _| {
        assert!(w.try_find_by_label("999999900").is_some(), "last row shown");
        assert!(w.try_find_by_label("0").is_none(), "first row recycled");
    });

    // Wheel: three rows up, f64-precise near the end of a 360M px document.
    let (_, before) = grid.scroll_offset();
    let h = f64::from(grid.row_height());
    let pos = at(&grid, Some(9_999_990), 1);
    vcx.simulate_event(ScrollEvent {
        position: pos,
        delta: ScrollDelta::Pixels(point(px(0.), px((3. * h) as f32))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    vcx.run_until_parked();
    let (_, after) = grid.scroll_offset();
    assert!(
        (before - after - 3. * h).abs() < 1e-6,
        "{before} -> {after}"
    );

    // Horizontal: columns past the viewport appear, pool unchanged.
    vcx.update(|_, cx| grid.set_scroll(4_000., after, cx));
    vcx.run_until_parked();
    let first = grid.visible_rows().start;
    let label = format!("{}", first * 100 + 40);
    vcx.update(|w, _| assert!(w.try_find_by_label(&label).is_some(), "{label} visible"));
}

#[gpui_kit::test]
fn materialised_million_rows_keep_viewport_sized_slots(cx: &mut TestAppContext) {
    let rows = (0..1_000_000_i64).collect();
    let source = VecSource::new(rows).column(|&value| CellRef::Int(value));
    let (grid, _, _, vcx) = mount(cx, |cx| GridHandle::new(source, columns(1), cx));
    for row in [0, 500_000, 999_999] {
        vcx.update(|_, cx| grid.scroll_to_row(row, cx));
        vcx.run_until_parked();
        assert!(grid.visible_rows().contains(&row));
        assert!(grid.stats().pool < 40);
        vcx.update(|window, _| {
            assert!(window.try_find_by_label(&row.to_string()).is_some());
        });
    }
}

#[gpui_kit::test]
fn click_shift_click_and_ctrl_click_select_rows(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| {
        let g = GridHandle::new(million(), columns(5), cx);
        g.set_options(
            GridOptions {
                selection: SelectionMode::MultiRow,
                ..Default::default()
            },
            cx,
        );
        g
    });
    click(vcx, at(&grid, Some(2), 1), Modifiers::default());
    click(vcx, at(&grid, Some(5), 0), shift());
    assert_eq!(grid.selection().rows.ranges(), &[2..6][..]);
    click(
        vcx,
        at(&grid, Some(4), 0),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(grid.selection().rows.ranges(), &[2..4, 5..6][..]);
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        GridEvent::SelectionChanged {
            selected_rows: 3,
            ..
        }
    )));
    vcx.update(|w, cx| {
        w.render_frame(cx);
        let rows = w.find_all_by_role(Role::Row);
        let selected = rows.iter().filter(|r| r.selected() == Some(true)).count();
        assert_eq!(selected, 3);
        assert!(
            !grid.focus_handle().is_focused(w),
            "a click never takes focus by default"
        );
    });
}

#[gpui_kit::test]
fn keyboard_navigation_select_all_and_copy(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| {
        let g = GridHandle::new(million(), columns(4), cx);
        g.set_options(
            GridOptions {
                selection: SelectionMode::Range,
                focusable: Some(true),
                ..Default::default()
            },
            cx,
        );
        g
    });
    click(vcx, at(&grid, Some(1), 1), Modifiers::default());
    vcx.update(|w, _| assert!(grid.focus_handle().is_focused(w), "opted-in grid focuses"));
    vcx.simulate_keystrokes("down right shift-down shift-right");
    let s = grid.selection();
    assert_eq!(s.rect(), Some((2..4, 2..4)));
    vcx.simulate_keystrokes("ctrl-c");
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()),
            Some("202\t203\n302\t303".to_string())
        );
    });
    vcx.simulate_keystrokes("pagedown");
    let page = grid.selection().cursor.map(|c| c.row).unwrap_or(0);
    assert!(page > 10, "page down moved a page: {page}");
    assert!(grid.visible_rows().contains(&page));
    vcx.simulate_keystrokes("ctrl-end");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(999_999, 3)));
    assert!(grid.visible_rows().contains(&999_999), "scrolled into view");
    vcx.simulate_keystrokes("home");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(999_999, 0)));
    vcx.simulate_keystrokes("ctrl-home");
    assert_eq!(grid.selection().cursor, Some(CellPos::new(0, 0)));
    vcx.simulate_keystrokes("ctrl-a");
    assert_eq!(grid.selection().rect(), Some((0..1_000_000, 0..4)));
}

#[gpui_kit::test]
fn dragging_a_header_edge_resizes_and_header_drag_reorders(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| GridHandle::new(million(), columns(4), cx));
    let b = grid.bounds();
    let h = grid.row_height();
    let edge = b.origin + point(px(100.), px(h / 2.));
    vcx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        edge + point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        edge + point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    vcx.run_until_parked();
    assert_eq!(grid.columns()[0].width, 160.);
    assert!(events.borrow().contains(&GridEvent::ColumnResized {
        column: 0,
        width: 160.
    }));
    assert!(grid.sort().is_empty(), "a resize is not a sort click");

    // Drag column 0's header onto the right half of column 2.
    let from = at(&grid, None, 0);
    let to = at(&grid, None, 2) + point(px(30.), px(0.));
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(grid.column_order(), vec![1, 2, 0, 3]);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, GridEvent::ColumnReordered { column: 0, .. }))
    );
}

fn people() -> VecSource<(String, i64)> {
    VecSource::new(vec![
        ("Carol".to_string(), 30),
        ("alice".to_string(), 30),
        ("Bob".to_string(), 25),
        ("Dave".to_string(), 41),
    ])
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

fn people_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef::new("name", "Name", 0).editable(true),
        ColumnDef::new("age", "Age", 1).editable(true),
    ]
}

#[gpui_kit::test]
fn header_click_sorts_shift_click_adds_keys(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| GridHandle::new(people(), people_columns(), cx));
    click(vcx, at(&grid, None, 1), Modifiers::default());
    assert_eq!(
        grid.sort(),
        vec![SortKey {
            column: 1,
            dir: SortDir::Asc
        }]
    );
    let order: Vec<_> = (0..4).filter_map(|r| grid.source_row(r)).collect();
    assert_eq!(order, [2, 0, 1, 3], "stable: Carol before alice at 30");
    click(vcx, at(&grid, None, 0), shift());
    assert_eq!(grid.sort().len(), 2);
    let order: Vec<_> = (0..4).filter_map(|r| grid.source_row(r)).collect();
    assert_eq!(order, [2, 0, 1, 3], "byte order: 'C' < 'a'");
    click(vcx, at(&grid, None, 1), Modifiers::default());
    assert_eq!(
        grid.sort().len(),
        1,
        "a plain click replaces the multi-sort"
    );
    click(vcx, at(&grid, None, 1), Modifiers::default());
    assert_eq!(grid.sort()[0].dir, SortDir::Desc);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, GridEvent::SortChanged(_)))
    );
    vcx.update(|w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find_all_by_role(Role::ColumnHeader).len(), 2);
    });
}

#[gpui_kit::test]
fn large_sort_runs_in_the_background(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| {
        let g = GridHandle::new(
            FnSource::new(200_000, |r, _| CellRef::Int(((r * 7919) % 200_000) as i64)),
            columns(1),
            cx,
        );
        g
    });
    vcx.update(|_, cx| {
        grid.set_sort(
            vec![SortKey {
                column: 0,
                dir: SortDir::Desc,
            }],
            cx,
        )
    });
    assert!(grid.is_busy(), "big data sorts off the UI thread");
    vcx.run_until_parked();
    assert!(!grid.is_busy());
    let first = grid.source_row(0).unwrap_or(usize::MAX);
    assert_eq!((first * 7919) % 200_000, 199_999);
    // A superseded job is dropped.
    vcx.update(|_, cx| {
        grid.set_sort(
            vec![SortKey {
                column: 0,
                dir: SortDir::Asc,
            }],
            cx,
        );
        grid.set_sort(vec![], cx);
    });
    vcx.run_until_parked();
    assert_eq!(grid.source_row(5), Some(5));
}

#[gpui_kit::test]
fn edit_commit_cancel_and_veto(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| GridHandle::new(people(), people_columns(), cx));
    // Double-click opens the Kit Input.
    let cell = at(&grid, Some(2), 1);
    vcx.simulate_event(gpui_kit::MouseDownEvent {
        button: MouseButton::Left,
        position: cell,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    vcx.simulate_event(gpui_kit::MouseUpEvent {
        button: MouseButton::Left,
        position: cell,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    vcx.run_until_parked();
    assert!(grid.is_editing());
    let input = grid.editor().unwrap_or_else(|| panic!("editor"));
    vcx.update(|w, cx| {
        assert_eq!(input.read(cx).value().as_ref(), "25");
        input.update(cx, |i, cx| i.set_value("abc", w, cx));
    });
    vcx.simulate_keystrokes("enter");
    assert!(grid.is_editing(), "the source vetoed a non-number");
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, GridEvent::EditRejected { row: 2, .. }))
    );
    vcx.update(|w, cx| input.update(cx, |i, cx| i.set_value("26", w, cx)));
    vcx.simulate_keystrokes("enter");
    assert!(!grid.is_editing());
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        GridEvent::CellEdited { row: 2, column: 1, value, .. } if value.as_ref() == "26"
    )));
    vcx.update(|w, _| assert!(w.try_find_by_label("26").is_some()));

    // Escape cancels.
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 0, w, cx)));
    vcx.run_until_parked();
    let input = grid.editor().unwrap_or_else(|| panic!("editor"));
    vcx.update(|w, cx| input.update(cx, |i, cx| i.set_value("Zed", w, cx)));
    vcx.simulate_keystrokes("escape");
    assert!(!grid.is_editing());
    vcx.update(|w, _| {
        assert!(w.try_find_by_label("Carol").is_some());
        assert!(w.try_find_by_label("Zed").is_none());
    });

    // Validation hook vetoes.
    grid.set_validator(|r| {
        if r.value.is_empty() {
            Err("required".into())
        } else {
            Ok(())
        }
    });
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 0, w, cx)));
    let input = grid.editor().unwrap_or_else(|| panic!("editor"));
    vcx.update(|w, cx| input.update(cx, |i, cx| i.set_value("", w, cx)));
    vcx.simulate_keystrokes("enter");
    assert!(grid.is_editing());
    vcx.simulate_keystrokes("escape");
}

#[gpui_kit::test]
fn enter_activates_non_editable_rows(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| {
        let g = GridHandle::new(million(), columns(3), cx);
        g.set_options(
            GridOptions {
                focusable: Some(true),
                ..Default::default()
            },
            cx,
        );
        g
    });
    click(vcx, at(&grid, Some(7), 0), Modifiers::default());
    vcx.simulate_keystrokes("enter");
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, GridEvent::RowActivated { row: 7, .. }))
    );
}

#[gpui_kit::test]
fn render_isolation(cx: &mut TestAppContext) {
    let (grid, host, _, vcx) = mount(cx, |cx| {
        let g = GridHandle::new(people(), people_columns(), cx);
        g.set_options(
            GridOptions {
                selection: SelectionMode::MultiRow,
                ..Default::default()
            },
            cx,
        );
        g
    });
    let renders = |vcx: &mut VisualTestContext| -> Vec<u64> {
        vcx.update(|_, cx| {
            (0..4)
                .map(|r| grid.row_renders(r, cx).unwrap_or(0))
                .collect()
        })
    };
    let host_before = host.get();
    let before = renders(vcx);
    vcx.update(|_, cx| grid.select(CellPos::new(1, 0), false, false, cx));
    vcx.run_until_parked();
    let after = renders(vcx);
    let changed: Vec<usize> = (0..4).filter(|&i| after[i] != before[i]).collect();
    if gpui_kit::GPUI_FAST {
        assert_eq!(changed, [1], "selecting row 1 re-rendered rows {changed:?}");
        assert_eq!(host.get(), host_before, "selection re-rendered the host");
    } else {
        assert!(changed.contains(&1));
    }

    // An edited cell (data change signalled by row range) re-renders only that row.
    // (Committing through the editor also moves keyboard focus, and a focus change makes
    // GPUI refresh the whole window on both backends.)
    let before = renders(vcx);
    let host_before = host.get();
    let grid_before = grid.stats().grid_renders;
    let ok = grid.update_source(|s| s.set_cell(3, 0, "Erin"));
    assert_eq!(ok, Some(Ok(())));
    vcx.update(|_, cx| grid.rows_changed(3..4, cx));
    vcx.run_until_parked();
    let after = renders(vcx);
    let changed: Vec<usize> = (0..4).filter(|&i| after[i] != before[i]).collect();
    if gpui_kit::GPUI_FAST {
        assert_eq!(changed, [3], "edit re-rendered rows {changed:?}");
        assert_eq!(host.get(), host_before, "edit re-rendered the host");
        assert_eq!(
            grid.stats().grid_renders,
            grid_before,
            "edit re-rendered the grid shell"
        );
    } else {
        assert!(changed.contains(&3));
    }
    vcx.update(|w, _| assert!(w.try_find_by_label("Erin").is_some()));
}

#[gpui_kit::test]
fn scrolling_is_o_viewport_and_never_renders_the_host(cx: &mut TestAppContext) {
    let (grid, host, _, vcx) = mount(cx, |cx| GridHandle::new(million(), columns(8), cx));
    let pool = grid.stats().pool as u64;
    let h = f64::from(grid.row_height());
    for step in 1..20 {
        let host_before = host.get();
        let before = grid.stats();
        vcx.update(|_, cx| grid.set_scroll(0., step as f64 * h * 1.5 + 500_000. * h, cx));
        vcx.run_until_parked();
        let after = grid.stats();
        assert_eq!(after.grid_renders - before.grid_renders, 1);
        // Rows move, so their views are redrawn; the work is bounded by the pool, never
        // by the 1M rows.
        assert!(after.row_renders - before.row_renders <= pool);
        assert!(after.visible_cells <= 10);
        if gpui_kit::GPUI_FAST {
            assert_eq!(host.get(), host_before, "scrolling re-rendered the host");
        }
    }
    // No-op scroll does nothing.
    let before = grid.stats();
    let (x, y) = grid.scroll_offset();
    vcx.update(|_, cx| grid.set_scroll(x, y, cx));
    vcx.run_until_parked();
    assert_eq!(grid.stats(), before);
}

#[gpui_kit::test]
fn focus_policy_and_stable_layout(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| GridHandle::new(million(), columns(3), cx));
    click(vcx, at(&grid, Some(0), 0), Modifiers::default());
    vcx.update(|w, _| assert!(!grid.focus_handle().is_focused(w)));
    let base = guiraffe_test::layout::capture(vcx, &["grid"]);
    vcx.update(|w, cx| grid.focus(w, cx));
    vcx.simulate_keystrokes("down");
    guiraffe_test::layout::assert_same_bounds(vcx, &["grid"], &base, "focused");
}

#[gpui_kit::test]
fn accessibility_roles_and_indices(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| {
        let mut cols = columns(4);
        cols[3].pin = Pin::Left;
        GridHandle::new(million(), cols, cx)
    });
    vcx.update(|_, cx| grid.select(CellPos::new(0, 0), false, false, cx));
    vcx.run_until_parked();
    vcx.update(|w, cx| {
        w.render_frame(cx);
        let grids = w.find_all_by_role(Role::Grid);
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].label(), Some("Data grid"));
        assert_eq!(w.find_all_by_role(Role::ColumnHeader).len(), 4);
        let cell = w.find_by_label("3");
        assert_eq!(cell.role(), Some(Role::GridCell));
        assert_eq!(cell.selected(), Some(true));
        let cell = w.find_by_label("101");
        assert_eq!(cell.selected(), Some(false));
    });
    // The pinned column is displayed first.
    assert_eq!(grid.layout().display(0).map(|(_, p)| p.def), Some(3));
}

#[gpui_kit::test]
fn shared_source_reports_busy_on_edit(cx: &mut TestAppContext) {
    let src: Arc<dyn GridSource> = Arc::new(people());
    let keep = src.clone();
    let (grid, _, events, vcx) = mount(cx, move |cx| {
        GridHandle::from_arc(src, people_columns(), cx)
    });
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 1, w, cx)));
    vcx.simulate_keystrokes("enter");
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, GridEvent::EditRejected { .. }))
    );
    drop(keep);
}

// ---------------------------------------------------------------- review fixes

fn has(events: &Events, f: impl Fn(&GridEvent) -> bool) -> bool {
    events.borrow().iter().any(f)
}

fn multi_people(cx: &mut gpui_kit::App) -> GridHandle {
    let g = GridHandle::new(people(), people_columns(), cx);
    g.set_options(
        GridOptions {
            selection: SelectionMode::MultiRow,
            focusable: Some(true),
            ..Default::default()
        },
        cx,
    );
    g
}

#[gpui_kit::test]
fn selection_follows_source_rows_through_sort_and_filter(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, multi_people);
    // View row 0 is Carol (source 0).
    vcx.update(|_, cx| grid.select(CellPos::new(0, 1), false, false, cx));
    events.borrow_mut().clear();
    vcx.update(|_, cx| {
        grid.set_sort(
            vec![SortKey {
                column: 1,
                dir: SortDir::Asc,
            }],
            cx,
        )
    });
    vcx.run_until_parked();
    // Ages 30,30,25,41 sort to [Bob, Carol, alice, Dave]: Carol is now view row 1.
    assert_eq!(grid.selection().rows.ranges(), &[1..2][..]);
    assert_eq!(grid.selection().cursor, Some(CellPos::new(1, 1)));
    assert!(has(
        &events,
        |e| matches!(e, GridEvent::SelectionChanged { cursor: Some(c), .. } if c.row == 1)
    ));
    events.borrow_mut().clear();
    // Filtering Carol out drops her from the selection, with an event.
    vcx.update(|_, cx| {
        grid.set_filter(
            0,
            Some(Arc::new(|c| !matches!(c, CellRef::Text("Carol")))),
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(grid.selection().rows.is_empty());
    assert!(has(&events, |e| matches!(
        e,
        GridEvent::SelectionChanged {
            selected_rows: 0,
            ..
        }
    )));
}

#[gpui_kit::test]
fn open_edit_survives_a_sort(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, multi_people);
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 0, w, cx)));
    let input = grid.editor().unwrap_or_else(|| panic!("editor"));
    vcx.update(|w, cx| input.update(cx, |i, cx| i.set_value("Caroline", w, cx)));
    vcx.update(|_, cx| {
        grid.set_sort(
            vec![SortKey {
                column: 1,
                dir: SortDir::Asc,
            }],
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(grid.is_editing(), "the sort discarded the open edit");
    assert_eq!(grid.editing_cell(), Some(CellPos::new(1, 0)));
    vcx.simulate_keystrokes("enter");
    assert!(has(&events, |e| matches!(e,
        GridEvent::CellEdited { row: 0, column: 0, value, .. } if value.as_ref() == "Caroline")));
    vcx.update(|w, _| assert!(grid.focus_handle().is_focused(w), "focus back on the grid"));
}

#[gpui_kit::test]
fn edit_on_a_filtered_out_row_is_cancelled_and_focus_returns(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, multi_people);
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 0, w, cx)));
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        grid.set_filter(
            0,
            Some(Arc::new(|c| !matches!(c, CellRef::Text("Carol")))),
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(!grid.is_editing());
    assert!(has(&events, |e| matches!(
        e,
        GridEvent::EditCancelled { row: 0, column: 0 }
    )));
    vcx.update(|w, _| {
        assert!(
            grid.focus_handle().is_focused(w),
            "focus left on a dropped Input"
        )
    });
    // Keys reach the grid again.
    vcx.simulate_keystrokes("down");
    assert!(grid.selection().cursor.is_some());
}

#[gpui_kit::test]
fn commit_during_a_background_sort_is_queued_not_rejected(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| {
        let src = VecSource::new((0..120_000i64).collect::<Vec<_>>()).editable_column(
            |v: &i64| CellRef::Int(*v),
            |v, t| {
                *v = t.parse().map_err(|_| EditError::Invalid(t.to_string()))?;
                Ok(())
            },
        );
        GridHandle::new(src, vec![ColumnDef::new("v", "V", 0).editable(true)], cx)
    });
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 0, w, cx)));
    let input = grid.editor().unwrap_or_else(|| panic!("editor"));
    vcx.update(|w, cx| input.update(cx, |i, cx| i.set_value("-5", w, cx)));
    vcx.update(|w, cx| {
        grid.set_sort(
            vec![SortKey {
                column: 0,
                dir: SortDir::Desc,
            }],
            cx,
        );
        assert!(grid.is_busy());
        assert!(
            grid.commit_edit(w, cx),
            "commit accepted while the sort runs"
        );
    });
    assert!(!has(&events, |e| matches!(
        e,
        GridEvent::EditRejected { .. }
    )));
    vcx.run_until_parked();
    assert!(has(&events, |e| matches!(e,
        GridEvent::CellEdited { row: 0, value, .. } if value.as_ref() == "-5")));
    assert!(!has(&events, |e| matches!(
        e,
        GridEvent::EditRejected { .. }
    )));
}

#[gpui_kit::test]
fn editor_follows_its_column_and_closes_when_hidden(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, multi_people);
    // Edit Age (definition 1, display 1).
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 1, w, cx)));
    vcx.run_until_parked();
    vcx.update(|_, cx| grid.move_column(1, Some(0), cx));
    vcx.run_until_parked();
    assert_eq!(grid.editing_cell(), Some(CellPos::new(0, 0)));
    // The editor is drawn in the Age cell, now display column 0, not in display column 1.
    let col0 = at(&grid, Some(0), 0);
    vcx.update(|w, cx| {
        w.render_frame(cx);
        let ed = w.find_by_label("Edit Age");
        assert!(
            ed.bounds().contains(&col0),
            "editor at {:?}, Age cell at {col0:?}",
            ed.bounds()
        );
    });
    vcx.update(|_, cx| grid.set_column_visible(1, false, cx));
    vcx.run_until_parked();
    assert!(!grid.is_editing());
    assert!(has(&events, |e| matches!(
        e,
        GridEvent::EditCancelled { row: 0, column: 1 }
    )));
}

#[gpui_kit::test]
fn a_new_edit_cancels_the_open_one_and_rerenders_its_row(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, multi_people);
    vcx.update(|w, cx| assert!(grid.begin_edit(0, 0, w, cx)));
    vcx.run_until_parked();
    let before = vcx.update(|_, cx| grid.row_renders(0, cx)).unwrap_or(0);
    vcx.update(|w, cx| assert!(grid.begin_edit(2, 0, w, cx)));
    vcx.run_until_parked();
    let after = vcx.update(|_, cx| grid.row_renders(0, cx)).unwrap_or(0);
    assert!(after > before, "the old editor's row was not re-rendered");
    assert!(has(&events, |e| matches!(
        e,
        GridEvent::EditCancelled { row: 0, column: 0 }
    )));
    assert_eq!(grid.editing_cell(), Some(CellPos::new(2, 0)));
}

#[gpui_kit::test]
fn recycled_slots_show_their_new_rows(cx: &mut TestAppContext) {
    // Scrolling by exactly `pool` rows puts row r + pool in row r's slot at the *same*
    // bounds, so gpui-fast would replay the old row unless the slot is notified.
    let (grid, _, _, vcx) = mount(cx, |cx| GridHandle::new(million(), columns(3), cx));
    let pool = grid.stats().pool;
    let h = f64::from(grid.row_height());
    let old: Vec<_> = grid
        .visible_rows()
        .filter_map(|r| grid.row_view_id(r))
        .collect();
    vcx.update(|_, cx| grid.set_scroll(0., (pool * 10) as f64 * h, cx));
    vcx.run_until_parked();
    let before = grid.stats();
    vcx.update(|_, cx| grid.set_scroll(0., (pool * 11) as f64 * h, cx));
    vcx.run_until_parked();
    let rows = grid.visible_rows();
    assert!(grid.stats().row_renders - before.row_renders <= pool as u64);
    vcx.update(|w, _| {
        for r in rows.clone() {
            let label = format!("{}", r * 100);
            assert!(
                w.try_find_by_label(&label).is_some(),
                "row {r} not shown in its slot"
            );
            let stale = format!("{}", (r - pool) * 100);
            assert!(
                w.try_find_by_label(&stale).is_none(),
                "row {} still shown",
                r - pool
            );
        }
    });
    // Same pool of entities, reused (no new views were created).
    assert_eq!(grid.stats().pool, pool);
    assert!(rows.clone().all(|r| grid.row_view_id(r).is_some()));
    assert!(!old.is_empty());
}

#[gpui_kit::test]
fn copy_reports_truncation_and_runs_big_copies_off_thread(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| {
        let g = GridHandle::new(million(), columns(3), cx);
        g.set_options(
            GridOptions {
                selection: SelectionMode::Range,
                ..Default::default()
            },
            cx,
        );
        g
    });
    // Small: inline.
    vcx.update(|_, cx| {
        grid.select(CellPos::new(0, 0), false, false, cx);
        grid.select(CellPos::new(1, 1), true, false, cx);
        assert_eq!(grid.copy(cx).as_deref(), Some("0\t1\n100\t101"));
    });
    assert!(has(&events, |e| matches!(
        e,
        GridEvent::Copied {
            cells: 4,
            truncated: false
        }
    )));
    // 3M cells: background, truncated at MAX_COPY_CELLS.
    vcx.update(|_, cx| {
        grid.select(CellPos::new(0, 0), false, false, cx);
        grid.select(CellPos::new(999_999, 2), true, false, cx);
        assert_eq!(grid.copy(cx), None, "large copy is deferred");
    });
    vcx.run_until_parked();
    assert!(has(&events, |e| matches!(
        e,
        GridEvent::Copied {
            truncated: true,
            ..
        }
    )));
    vcx.update(|_, cx| {
        let text = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        let rows = text.lines().count();
        assert_eq!(rows * 3, guiraffe_datagrid::tsv::MAX_COPY_CELLS - 2);
    });
}

#[gpui_kit::test]
fn header_drop_across_bands_is_ignored(cx: &mut TestAppContext) {
    let (grid, _, events, vcx) = mount(cx, |cx| {
        let mut cols = columns(4);
        cols[0].pin = Pin::Left;
        GridHandle::new(million(), cols, cx)
    });
    let from = at(&grid, None, 0);
    let to = at(&grid, None, 2) + point(px(30.), px(0.));
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(grid.column_order(), vec![0, 1, 2, 3]);
    assert!(!has(&events, |e| matches!(
        e,
        GridEvent::ColumnReordered { .. }
    )));
    // Within the center band it still works.
    let from = at(&grid, None, 1);
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(grid.column_order(), vec![0, 2, 1, 3]);
}

#[gpui_kit::test]
fn rows_expose_their_aria_row_index(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| GridHandle::new(million(), columns(2), cx));
    assert_eq!(
        guiraffe_datagrid::aria_row_index(0),
        2,
        "row 1 is the header"
    );
    vcx.update(|_, cx| grid.scroll_to_row(500_000, cx));
    vcx.run_until_parked();
    vcx.update(|w, cx| {
        w.render_frame(cx);
        let rows = w.find_all_by_role(Role::Row);
        // One unlabelled header row plus data rows labelled with their 1-based index, in
        // order (rows are listed by position; overscan rows sit above the header).
        let labels: Vec<_> = rows.iter().filter_map(|r| r.label()).collect();
        assert_eq!(rows.len(), labels.len() + 1);
        let first = grid.visible_rows().start;
        for (i, l) in labels.iter().enumerate() {
            assert_eq!(*l, format!("Row {}", first + i + 1));
            assert_eq!(guiraffe_datagrid::aria_row_index(first + i), first + i + 2);
        }
        assert!(rows.len() > 10);
    });
}

/// The horizontal scrollbar is driven by real pointer input at the thumb's
/// painted position, so a thumb outside the grid's hit area or under another
/// element fails here.
#[gpui_kit::test]
fn dragging_the_horizontal_thumb_scrolls_the_center_band(cx: &mut TestAppContext) {
    let (grid, _, _, vcx) = mount(cx, |cx| {
        let mut cols = columns(30);
        cols[0].pin = Pin::Left;
        GridHandle::new(million(), cols, cx)
    });
    vcx.run_until_parked();
    let thumb = vcx
        .debug_bounds("datagrid-hscroll-thumb")
        .unwrap_or_else(|| panic!("the horizontal thumb is painted"));
    let lw = grid.layout().left_width;
    let left = grid.bounds().origin.x + px(lw);
    assert!(thumb.left() >= left, "the thumb lives in the center band");
    assert_eq!(grid.scroll_offset().0, 0.);

    let width = f32::from(grid.bounds().size.width);
    let vp = f64::from(grid.layout().center_viewport(width));
    let content = grid.layout().center_width;
    let track = vp - f64::from(f32::from(thumb.size.width));
    let per_px = (content - vp) / track;

    let start = thumb.center();
    let to = start + point(px(50.), px(0.));
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let (sx, sy) = grid.scroll_offset();
    assert!(
        (sx - 50. * per_px).abs() < 1.,
        "a 50px thumb drag scrolls {} px, got {sx}",
        50. * per_px
    );
    assert_eq!(sy, 0., "a horizontal drag never scrolls rows");
    assert!(
        grid.selection().cursor.is_none(),
        "a thumb drag selects nothing"
    );

    // A press on the track right of the thumb pages one viewport.
    let thumb = vcx
        .debug_bounds("datagrid-hscroll-thumb")
        .unwrap_or_else(|| panic!("the horizontal thumb is painted"));
    let track_at = point(thumb.right() + px(20.), thumb.center().y);
    click(vcx, track_at, Modifiers::default());
    assert!(
        (grid.scroll_offset().0 - (sx + vp)).abs() < 0.5,
        "a track press pages one viewport: {} -> {}",
        sx,
        grid.scroll_offset().0
    );
    // And left of it pages back.
    let thumb = vcx
        .debug_bounds("datagrid-hscroll-thumb")
        .unwrap_or_else(|| panic!("the horizontal thumb is painted"));
    click(
        vcx,
        point(left + px(2.), thumb.center().y),
        Modifiers::default(),
    );
    assert!((grid.scroll_offset().0 - sx).abs() < 0.5);
}
