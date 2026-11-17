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

struct Scene(GridHandle);
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(12.))
            .child(div().w(px(360.)).h(px(180.)).child(DataGrid::new(&self.0)))
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
                    cx.new(|_| Scene(g))
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
