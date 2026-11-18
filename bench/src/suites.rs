//! Add new crate workloads here, then list them in `registered`.
//! The same functions feed the CLI, criterion and instruction-count benches.
use crate::Suite;
use guiraffe_core::{
    ColorName, Scheme, Variant,
    variant::{ResolveCtx, resolve},
};
use guiraffe_windows::placement::{Display, PlacementRules, Rect, cascade, place};
use std::hint::black_box;

/// Resolve every variant in both schemes, including color contrast work.
pub fn core_variants() {
    for scheme in [Scheme::Light, Scheme::Dark] {
        let ctx = black_box(ResolveCtx::new(scheme));
        for variant in [
            Variant::Filled,
            Variant::Light,
            Variant::Outline,
            Variant::Subtle,
            Variant::Transparent,
            Variant::White,
            Variant::Default,
        ] {
            black_box(resolve(
                &ctx,
                black_box(ColorName::Blue),
                black_box(variant),
            ));
        }
    }
}

/// Restore and cascade a window against two displays, including disconnected UUID recovery.
pub fn windows_placement() {
    let displays = [
        Display {
            uuid: None,
            bounds: Rect::new(0., 0., 1920., 1080.),
        },
        Display {
            uuid: None,
            bounds: Rect::new(1920., 0., 2560., 1440.),
        },
    ];
    let saved = black_box(Rect::new(1800., -50., 1000., 800.));
    let bounds = place(
        Some((saved, Some("disconnected"))),
        black_box(&displays),
        &PlacementRules::default(),
    );
    black_box(cascade(
        bounds,
        black_box(7),
        displays.get(1).map(|display| &display.bounds),
    ));
}

// Immutable source/index setup is paid by warmup. These suites measure the code model,
// not native frame rate: actual visible-line shaping is asserted in code's GPUI tests.
fn code_document() -> &'static guiraffe_code::CodeDocument {
    static DOCUMENT: std::sync::OnceLock<guiraffe_code::CodeDocument> = std::sync::OnceLock::new();
    DOCUMENT.get_or_init(|| {
        guiraffe_code::CodeDocument::new(std::sync::Arc::<str>::from(
            "let value = compute(42);\n".repeat(999_999),
        ))
    })
}
/// 32 viewport jumps in a million-line file, reading only the visible lines and hashes.
pub fn code_scroll_million() {
    use guiraffe_code::RowMap;
    let document = code_document();
    static ROWS: std::sync::OnceLock<RowMap> = std::sync::OnceLock::new();
    let rows = ROWS.get_or_init(|| RowMap::new(document.line_count(), &[], &[]));
    code_scroll(document, rows, 29_000);
}

fn code_scroll(
    document: &guiraffe_code::CodeDocument,
    rows: &guiraffe_code::RowMap,
    stride: usize,
) {
    use guiraffe_code::{Row, Viewport};
    for step in 0..32 {
        let viewport = Viewport {
            x: 0.,
            y: (step * stride * 20) as f64,
        };
        for row in viewport.visible(rows.len(), 20., 800.) {
            if let Some(Row::Source(line)) = rows.row(row) {
                black_box(document.line(line));
                black_box(document.line_hash(line));
            }
        }
    }
}
fn production_code_source() -> std::sync::Arc<str> {
    static SOURCE: std::sync::OnceLock<std::sync::Arc<str>> = std::sync::OnceLock::new();
    SOURCE
        .get_or_init(|| "x\n".repeat(9_999_999).into())
        .clone()
}

pub fn code_index_ten_million() {
    black_box(guiraffe_code::CodeDocument::new(production_code_source()));
}

pub fn code_scroll_ten_million() {
    use guiraffe_code::{CodeDocument, RowMap};
    static FIXTURE: std::sync::OnceLock<(CodeDocument, RowMap)> = std::sync::OnceLock::new();
    let (document, rows) = FIXTURE.get_or_init(|| {
        let document = CodeDocument::new(production_code_source());
        let rows = RowMap::new(document.line_count(), &[], &[]);
        (document, rows)
    });
    code_scroll(document, rows, (rows.len() - 1) / 31);
}
/// 1,024 Arc clone/drop pairs. This does not measure worker publication or UI admission.
pub fn code_snapshot_arc_clone() {
    static DOCUMENT: std::sync::OnceLock<std::sync::Arc<guiraffe_code::CodeDocument>> =
        std::sync::OnceLock::new();
    let document = DOCUMENT.get_or_init(|| {
        std::sync::Arc::new(guiraffe_code::CodeDocument::new(
            std::sync::Arc::<str>::from("fn main() {}\n".repeat(999_999)),
        ))
    });
    for _ in 0..1024 {
        black_box(std::sync::Arc::clone(document));
    }
}

// Each flow invocation is described below. Fixture creation is paid only by
// warmup; the measured move/delete workloads restore their document each time.
fn flow_graph() -> &'static guiraffe_flow_core::Graph {
    use guiraffe_flow_core::{Edge, Graph, Node, Point};
    static GRAPH: std::sync::OnceLock<Graph> = std::sync::OnceLock::new();
    GRAPH.get_or_init(|| {
        let graph = Graph::from_parts(
            (0..5000).map(|id| {
                let mut n = Node::new(
                    id,
                    "node",
                    Point::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
                );
                if id == 1 {
                    n.parent = Some(0);
                }
                n
            }),
            (0..10000).map(|id| {
                Edge::new(
                    id,
                    id % 5000,
                    (id % 5000 + if id < 5000 { 1 } else { 100 }) % 5000,
                )
            }),
        );
        match graph {
            Ok(graph) => graph,
            Err(error) => panic!("flow benchmark fixture rejected: {error}"),
        }
    })
}
fn flow_editor(f: impl FnOnce(&mut guiraffe_flow_core::Editor)) {
    thread_local! {static EDITOR:std::cell::RefCell<Option<guiraffe_flow_core::Editor>>=const {std::cell::RefCell::new(None)};}
    EDITOR.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(
                match guiraffe_flow_core::Editor::new(flow_graph().clone()) {
                    Ok(mut e) => {
                        e.history_limit(1);
                        e
                    }
                    Err(error) => panic!("flow benchmark editor rejected: {error}"),
                },
            );
        }
        if let Some(editor) = slot.as_mut() {
            f(editor);
        } else {
            panic!("flow benchmark editor unavailable");
        }
    });
}
/// Build and drop one 5k-node/10k-edge scene, including validation/routes/BVHs.
pub fn flow_build() {
    let result = guiraffe_flow_core::Scene::new(black_box(flow_graph().clone()));
    assert!(result.is_ok());
    drop(black_box(result));
}
/// Twenty spatial queries plus cached-route access; reuse output buffers.
pub fn flow_query() {
    use guiraffe_flow_core::{Rect, Scene, Visible};
    static SCENE: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
    let scene = SCENE.get_or_init(|| match Scene::new(flow_graph().clone()) {
        Ok(s) => s,
        Err(error) => panic!("flow benchmark scene rejected: {error}"),
    });
    thread_local! {static VISIBLE:std::cell::RefCell<Visible>=std::cell::RefCell::new(Visible {nodes:Vec::with_capacity(5000),edges:Vec::with_capacity(10000),visited:0});}
    VISIBLE.with(|scratch| {
        let mut visible = scratch.borrow_mut();
        for i in 0..20 {
            scene.visible(
                black_box(Rect::new(i as f32 * 200., i as f32 * 100., 1920., 1080.)),
                &mut visible,
            );
            for id in &visible.edges {
                black_box(scene.paths().get(id));
            }
        }
    });
}
/// Move one child node, then undo; includes the edit/history/cache costs.
pub fn flow_move() {
    flow_editor(|editor| {
        let positions =
            std::collections::BTreeMap::from([(1, guiraffe_flow_core::Point::new(280., 20.))]);
        assert!(matches!(
            editor.move_nodes(black_box(&positions), None),
            Ok(true)
        ));
        assert!(matches!(editor.undo(), Ok(true)));
    });
}
/// Select/delete a child with incident edges, then undo; hierarchy depth cost included.
pub fn flow_delete_child() {
    flow_editor(|editor| {
        editor.select(&std::collections::BTreeSet::from([1]), false);
        assert!(matches!(editor.delete_selected(), Ok(true)));
        assert!(matches!(editor.undo(), Ok(true)));
    });
}

// Data grid: a computed 1M x 50 source (nothing stored), as the showcase uses.
type GridCellFn = fn(usize, usize) -> guiraffe_datagrid::CellRef<'static>;
type GridFixture = (
    guiraffe_datagrid::FnSource<GridCellFn>,
    Vec<guiraffe_datagrid::ColumnDef>,
    guiraffe_datagrid::ColumnLayout,
);
fn grid_cell(row: usize, col: usize) -> guiraffe_datagrid::CellRef<'static> {
    use guiraffe_datagrid::CellRef;
    const NAMES: [&str; 8] = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
    ];
    match col % 3 {
        0 => CellRef::Int(((row.wrapping_mul(2_654_435_761) ^ col) % 1_000_000) as i64),
        1 => CellRef::Number((row % 10_007) as f64 * 0.25 + col as f64),
        _ => CellRef::Text(NAMES[(row ^ col) % NAMES.len()]),
    }
}
fn grid_fixture() -> &'static GridFixture {
    static FIXTURE: std::sync::OnceLock<GridFixture> = std::sync::OnceLock::new();
    FIXTURE.get_or_init(|| {
        let cols: Vec<_> = (0..50)
            .map(|c| guiraffe_datagrid::ColumnDef::new("c", "c", c).width(110.))
            .collect();
        let order: Vec<usize> = (0..50).collect();
        let layout = guiraffe_datagrid::ColumnLayout::compute(&cols, &order);
        (
            guiraffe_datagrid::FnSource::new(1_000_000, grid_cell as GridCellFn),
            cols,
            layout,
        )
    })
}
/// 32 viewport jumps (vertical and horizontal) over 1M x 50: window math, visible-column
/// search and formatting every visible cell into a reused buffer (what a frame reads).
pub fn datagrid_scroll_million() {
    let (source, cols, layout) = grid_fixture();
    grid_scroll(source, cols, layout, 31_250);
}

pub fn datagrid_scroll_ten_million() {
    let (_, cols, layout) = grid_fixture();
    let source = guiraffe_datagrid::FnSource::new(10_000_000, grid_cell as GridCellFn);
    grid_scroll(&source, cols, layout, 322_580);
}

fn grid_scroll(
    source: &impl guiraffe_datagrid::GridSource,
    cols: &[guiraffe_datagrid::ColumnDef],
    layout: &guiraffe_datagrid::ColumnLayout,
    stride: usize,
) {
    use guiraffe_datagrid::window;
    thread_local! {static TEXT: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(64));}
    TEXT.with(|text| {
        let mut text = text.borrow_mut();
        for step in 0..32usize {
            let y = (step * stride) as f64 * 36.;
            let x = (step * 157) as f64;
            let rows = window::visible_rows(y, 36., 720., source.row_count(), 3);
            let visible = layout.visible_center(x, 1280.);
            for r in rows {
                for p in layout.center.get(visible.clone()).into_iter().flatten() {
                    if let Some(def) = cols.get(p.def) {
                        text.clear();
                        def.format(source.cell(r, def.source), &mut text);
                        black_box(text.len());
                    }
                }
            }
        }
    });
}
/// Stable sort of 1M computed rows on an integer column: a `u32` permutation, no row copies.
pub fn datagrid_sort_million() {
    use guiraffe_datagrid::order::{SortDir, SortSpec, compute_order};
    let (source, _, _) = grid_fixture();
    let spec = [SortSpec {
        source_col: 0,
        dir: SortDir::Asc,
        comparator: None,
    }];
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let order = compute_order(source, &[], &spec, &cancel);
    assert!(order.is_ok());
    black_box(order.ok());
}

/// Warm the native fixture outside criterion/Callgrind measurement.
pub fn prepare_data_ui() {
    crate::data_ui::instantiate();
    crate::data_ui::patch();
    crate::data_ui::splice();
    crate::data_ui::structural_splice();
    crate::data_ui::structural_splice();
    crate::data_ui::settle();
}
pub fn data_ui_instantiate() {
    crate::data_ui::instantiate();
}
pub fn data_ui_patch() {
    crate::data_ui::patch();
}
pub fn data_ui_splice() {
    crate::data_ui::splice();
}

pub fn data_ui_structural_splice() {
    crate::data_ui::structural_splice();
}

fn chart_model(
    mark: guiraffe_charts::Mark,
    count: usize,
    value: impl Fn(usize) -> f64,
) -> guiraffe_charts::PreparedChart {
    use guiraffe_charts::{Channels, ChartSpec, Layer};
    let rows: Vec<_> = (0..count).map(|i| (i as f64, value(i))).collect();
    ChartSpec::new("Benchmark data")
        .layer(Layer::new(
            mark,
            rows,
            Channels::new(|r: &(f64, f64)| r.0, |r| r.1),
        ))
        .prepare()
}

fn million_line() -> &'static guiraffe_charts::PreparedChart {
    static MODEL: std::sync::OnceLock<guiraffe_charts::PreparedChart> = std::sync::OnceLock::new();
    MODEL.get_or_init(|| {
        chart_model(guiraffe_charts::Mark::Line, 1_000_000, |i| {
            let x = i as f64;
            (x * 0.002).sin() + (x * 0.000_07).cos() * 2.
        })
    })
}

fn dense_scatter() -> &'static guiraffe_charts::PreparedChart {
    static MODEL: std::sync::OnceLock<guiraffe_charts::PreparedChart> = std::sync::OnceLock::new();
    MODEL.get_or_init(|| {
        chart_model(guiraffe_charts::Mark::Point, 100_000, |i| {
            (i as u32).wrapping_mul(2_654_435_761).rotate_left(11) as f64 / f64::from(u32::MAX)
        })
    })
}

pub fn prepare_chart_line() {
    black_box(million_line());
}

pub fn prepare_chart_scatter() {
    black_box(dense_scatter());
}

/// Compile a full and zoomed 1200px viewport over a prepared million-point series.
pub fn chart_line_million() {
    let chart = million_line();
    let hidden = Default::default();
    black_box(chart.scene(1200., 440., chart.x_domain(), &hidden));
    if let Some(domain) = guiraffe_charts::Domain::new(482_500., 502_500.) {
        black_box(chart.scene(1200., 440., domain, &hidden));
    }
}

/// Compile a full 1200px viewport over 100k points and bin its visible geometry.
pub fn chart_scatter_hundred_thousand() {
    let chart = dense_scatter();
    black_box(chart.scene(1200., 440., chart.x_domain(), &Default::default()));
}

/// Warm visible-row traversal over one million frames at overview and detail zoom.
pub fn flamegraph_million() {
    use guiraffe_flamegraph::{FlameLayout, Frame, FrameSource};
    use std::sync::{Arc, OnceLock};

    struct Profile;
    impl FrameSource for Profile {
        fn len(&self) -> usize {
            1_000_000
        }
        fn frame(&self, index: usize) -> Frame<'_> {
            Frame {
                id: index as u64,
                parent: None,
                self_value: 1.,
                total: 1.,
                label: "frame",
                package: "bench",
            }
        }
    }
    static LAYOUT: OnceLock<FlameLayout> = OnceLock::new();
    let layout = LAYOUT.get_or_init(|| {
        FlameLayout::new(Arc::new(Profile))
            .unwrap_or_else(|error| panic!("invalid flame benchmark: {error:?}"))
    });
    for domain in [layout.domain(), 490_000.0..510_000.0] {
        layout.visit_row(0, domain, 1200., None, |segment| {
            black_box(segment);
        });
    }
}

/// Shared registration table consumed by the CLI, criterion and iai-callgrind.
/// Add workloads here once; each tool expands its own registration from this list.
#[macro_export]
macro_rules! with_suites {
    ($callback:ident) => {
        $callback! {
            (core_variants, "core-variants"),
            (chart_line_million, "chart-line-1m"),
            (flamegraph_million, "flamegraph-1m"),
            (chart_scatter_hundred_thousand, "chart-scatter-100k"),
            (code_scroll_million,"code-scroll-1m"),
            (code_scroll_ten_million, "code-scroll-10m"),
            (code_index_ten_million, "code-index-10m"),
            (code_snapshot_arc_clone,"code-snapshot-arc-clone"),
            (datagrid_scroll_million, "datagrid-scroll-1m"),
            (datagrid_scroll_ten_million, "datagrid-scroll-10m"),
            (datagrid_sort_million, "datagrid-sort-1m"),
            (flow_build,"flow-build-5k"),
            (flow_query,"flow-query-5k"),
            (flow_move,"flow-move-5k"),
            (flow_delete_child,"flow-delete-child-5k"),
            (windows_placement, "windows-placement"),
            (data_ui_instantiate, "data-ui-instantiate"),
            (data_ui_patch, "data-ui-patch"),
            (data_ui_splice, "data-ui-splice"),
            (data_ui_structural_splice, "data-ui-structural-splice"),
        }
    };
}
macro_rules! make_registry {
    ($(($function:ident, $name:literal),)*) => {
        /// Registered headless workloads. Names are stable baseline filenames.
        pub fn registered() -> Vec<Suite> {
            vec![$(Suite { name: $name, run: $function }),*]
        }
    };
}
crate::with_suites!(make_registry);
