//! The cell cursor (selected and editing) looks the same and never doubles a gridline or
//! shifts the cell's text: goldens for both states, light and dark.
use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_datagrid::{
    CellPos, CellRef, ColumnDef, DataGrid, GridHandle, SelectionMode, VecSource,
};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Scene {
    grid: GridHandle,
    initial_row: Option<usize>,
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let initial_scroll = self.initial_row.take().map(|row| {
            let grid = self.grid.clone();
            gpui_kit::canvas(
                move |_, _, cx| {
                    cx.defer(move |cx| {
                        grid.set_scroll(0., row as f64 * f64::from(grid.row_height()), cx);
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .w(px(0.))
            .h(px(0.))
        });
        div()
            .relative()
            .size_full()
            .p(px(12.))
            .child(
                div()
                    .w(px(360.))
                    .h(px(180.))
                    .child(DataGrid::new(&self.grid)),
            )
            .children(initial_scroll)
    }
}

fn grid(cx: &mut gpui_kit::App) -> GridHandle {
    let rows: Vec<(String, i64)> = [
        ("Analyst", 3030),
        ("Admiral", 809),
        ("Mathematician", 8589),
        ("Pilot", 120),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b))
    .collect();
    let src = VecSource::new(rows)
        .editable_column(
            |p: &(String, i64)| CellRef::Text(&p.0),
            |p, t| {
                p.0 = t.to_string();
                Ok(())
            },
        )
        .editable_column(|p: &(String, i64)| CellRef::Int(p.1), |_, _| Ok(()));
    let g = GridHandle::new(
        src,
        vec![
            ColumnDef::new("role", "Role", 0).width(200.).editable(true),
            ColumnDef::new("n", "Count", 1).width(120.),
        ],
        cx,
    );
    let mut o = g.options();
    o.selection = SelectionMode::Cell;
    g.set_options(o, cx);
    g
}

#[test]
fn cell_cursor_selected_and_editing() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        for editing in [false, true] {
            let mut s = Snapshotter::new().unwrap_or_else(|e| panic!("{e}"));
            let img = s
                .render(size(px(384.), px(204.)), move |window, cx| {
                    apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                    let g = grid(cx);
                    g.select(
                        CellPos {
                            row: 1,
                            col: if editing { 0 } else { 1 },
                        },
                        false,
                        false,
                        cx,
                    );
                    if editing {
                        g.begin_edit(1, 0, window, cx);
                    }
                    cx.new(|_| Scene {
                        grid: g,
                        initial_row: None,
                    })
                })
                .unwrap_or_else(|e| panic!("{e:#}"));
            let name = format!(
                "cell-cursor/{}-{}",
                if editing { "editing" } else { "selected" },
                if scheme == Scheme::Light {
                    "light"
                } else {
                    "dark"
                }
            );
            assert_snapshot(&name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
        }
    }
}

#[test]
fn computed_grid_at_production_scale() -> Result<(), Box<dyn std::error::Error>> {
    use guiraffe_datagrid::FnSource;
    const ROWS: usize = 10_000_000;
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, theme) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
        let mut snapshot = Snapshotter::new()?.with_setup(move |cx| {
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        for (row, position) in [(0, "top"), (ROWS / 2, "middle"), (ROWS - 1, "end")] {
            let mut control = None;
            let image = snapshot.render(size(px(384.), px(204.)), |_, cx| {
                let source = FnSource::new(ROWS, |r, c| CellRef::Int((r * 100 + c) as i64));
                let columns = (0..50)
                    .map(|col| ColumnDef::new(format!("c{col}"), format!("Col {col}"), col))
                    .collect();
                let grid = GridHandle::new(source, columns, cx);
                grid.select(CellPos::new(row, 0), false, false, cx);
                control = Some(grid.clone());
                cx.new(|_| Scene {
                    grid,
                    initial_row: Some(row),
                })
            })?;
            assert!(control.is_some_and(|grid| grid.visible_rows().contains(&row)));
            assert_snapshot(&format!("scale/{position}-{theme}"), &image, &config)?;
        }
    }
    Ok(())
}
