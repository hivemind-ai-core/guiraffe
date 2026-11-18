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
    use guiraffe_code::{Row, RowMap, Viewport};
    let document = code_document();
    static ROWS: std::sync::OnceLock<RowMap> = std::sync::OnceLock::new();
    let rows = ROWS.get_or_init(|| RowMap::new(document.line_count(), &[], &[]));
    for step in 0..32 {
        let viewport = Viewport {
            x: 0.,
            y: (step * 29_000 * 20) as f64,
        };
        for row in viewport.visible(rows.len(), 20., 800.) {
            if let Some(Row::Source(line)) = rows.row(row) {
                black_box(document.line(line));
                black_box(document.line_hash(line));
            }
        }
    }
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
    static GRAPH: std::sync::OnceLock<guiraffe_flow_core::Graph> = std::sync::OnceLock::new();
    GRAPH.get_or_init(|| build_flow_graph(5_000))
}

fn build_flow_graph(count: u64) -> guiraffe_flow_core::Graph {
    use guiraffe_flow_core::{Edge, Graph, Node, Point};
    let graph = Graph::from_parts(
        (0..count).map(|id| {
            let mut node = Node::new(
                id,
                "node",
                Point::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
            );
            if id == 1 {
                node.parent = Some(0);
            }
            node
        }),
        (0..count * 2).map(|id| {
            Edge::new(
                id,
                id % count,
                (id % count + if id < count { 1 } else { 100 }) % count,
            )
        }),
    );
    match graph {
        Ok(graph) => graph,
        Err(error) => panic!("flow benchmark fixture rejected: {error}"),
    }
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
    use guiraffe_datagrid::{GridSource, window};
    let (source, cols, layout) = grid_fixture();
    thread_local! {static TEXT: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(64));}
    TEXT.with(|text| {
        let mut text = text.borrow_mut();
        for step in 0..32usize {
            let y = (step * 31_250) as f64 * 36.;
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

fn production_flow_graph() -> &'static guiraffe_flow_core::Graph {
    static GRAPH: std::sync::OnceLock<guiraffe_flow_core::Graph> = std::sync::OnceLock::new();
    GRAPH.get_or_init(|| build_flow_graph(50_000))
}

pub fn flow_build_50k() {
    let scene = guiraffe_flow_core::Scene::new(black_box(production_flow_graph().clone()));
    assert!(black_box(scene).is_ok());
}

pub fn flow_pan_hit_50k() {
    use guiraffe_flow_core::{Point, Rect, Scene, Visible};
    static SCENE: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
    let scene = SCENE.get_or_init(|| match Scene::new(production_flow_graph().clone()) {
        Ok(scene) => scene,
        Err(error) => panic!("flow production fixture rejected: {error}"),
    });
    thread_local! {
        static VISIBLE: std::cell::RefCell<Visible> = Default::default();
        static HIT: std::cell::RefCell<Vec<u64>> = Default::default();
    }
    VISIBLE.with_borrow_mut(|visible| {
        HIT.with_borrow_mut(|hit| {
            for row in [0, 250, 490] {
                let y = row as f32 * 150.;
                scene.visible(black_box(Rect::new(0., y, 1920., 1080.)), visible);
                black_box(visible.nodes.len());
                black_box(visible.edges.len());
                black_box(scene.hit_node(Point::new(1., y + 1.), hit));
            }
        });
    });
}

pub fn timeline_box_select_100k() {
    use guiraffe_timeline::{Channel, DopeModel};
    thread_local! {
        static MODEL: std::cell::RefCell<DopeModel> = std::cell::RefCell::new(DopeModel::new(
            vec![Channel::new("Production keys", (0..100_000).map(f64::from).collect())],
        ));
    }
    MODEL.with_borrow_mut(|model| {
        for start in [0., 50_000., 99_900.] {
            model.box_select(0..=0, start, start + 99., false);
            assert_eq!(black_box(model.selected.len()), 100);
        }
    });
}

pub fn timeline_navigation_hit_100k() {
    use guiraffe_timeline::{Axis, ResizeEdges, Track, tracks_view::hit};
    static TRACK: std::sync::OnceLock<Track> = std::sync::OnceLock::new();
    let track = TRACK.get_or_init(|| {
        let mut track = Track::default();
        for i in 0..100_000 {
            assert!(track.insert(f64::from(i) * 2., 1.).is_some());
        }
        track
    });
    for start in [0., 100_000., 199_800.] {
        let mut axis = Axis::new(start, start + 100., 1200.);
        axis.pan_px(black_box(-24.));
        assert!(axis.zoom_at(black_box(600.), 2.));
        let x = axis.x(start + 50.5);
        assert!(black_box(hit(track, &axis, x, ResizeEdges::Both)).is_some());
    }
}

pub fn timeline_draw_bars_100k() {
    use guiraffe_timeline::BarCurve;
    thread_local! {
        static BARS: std::cell::RefCell<BarCurve> =
            std::cell::RefCell::new(BarCurve::new(100_000, -1., 1.));
    }
    BARS.with_borrow_mut(|bars| {
        black_box(bars.draw((0., 1.), (99_999., -1.)));
        black_box(bars.draw((99_999., 1.), (0., -1.)));
    });
}

fn production_line_spec() -> guiraffe_charts::ChartSpec {
    use guiraffe_charts::{Channels, ChartSpec, Layer, Mark};
    ChartSpec::new("Production line").layer(Layer::new(
        Mark::Line,
        (0..10_000_000_u32).collect::<Vec<_>>(),
        Channels::new(
            |index: &u32| f64::from(*index),
            |index| (f64::from(*index) * 0.002).sin(),
        ),
    ))
}

pub fn chart_prepare_line_10m() {
    black_box(production_line_spec().prepare());
}

pub fn chart_line_10m() {
    use guiraffe_charts::{Domain, PreparedChart};
    static LINE: std::sync::OnceLock<PreparedChart> = std::sync::OnceLock::new();
    let chart = LINE.get_or_init(|| production_line_spec().prepare());
    let hidden = Default::default();
    black_box(chart.scene(1200., 440., chart.x_domain(), &hidden));
    for start in [0., 5_000_000., 9_980_000.] {
        if let Some(domain) = Domain::new(start, start + 19_999.) {
            black_box(chart.scene(1200., 440., domain, &hidden));
        }
    }
}

pub fn chart_scatter_1m_100_series() {
    use guiraffe_charts::{Channels, ChartSpec, Layer, Mark, PreparedChart};
    static SCATTER: std::sync::OnceLock<PreparedChart> = std::sync::OnceLock::new();
    let chart = SCATTER.get_or_init(|| {
        ChartSpec::new("Production scatter")
            .layer(Layer::new(
                Mark::Point,
                (0..1_000_000_u32).collect::<Vec<_>>(),
                Channels::new(
                    |index: &u32| f64::from(*index),
                    |index| f64::from(index.wrapping_mul(2_654_435_761).rotate_left(11)),
                )
                .group(|index| index % 100),
            ))
            .prepare()
    });
    black_box(chart.scene(1200., 440., chart.x_domain(), &Default::default()));
}

/// The CLI and Criterion also include the large workloads. Callgrind uses the
/// smaller suite list to bound setup time and the instrumented memory footprint.
#[macro_export]
macro_rules! with_production_suites {
    ($callback:ident) => {
        $crate::with_suites!(
            $callback,
            (flow_build_50k, "flow-build-50k"),
            (flow_pan_hit_50k, "flow-pan-hit-50k"),
            (timeline_box_select_100k, "timeline-box-select-100k"),
            (timeline_draw_bars_100k, "timeline-draw-bars-100k"),
            (timeline_navigation_hit_100k, "timeline-navigation-hit-100k"),
            (chart_prepare_line_10m, "chart-prepare-line-10m"),
            (chart_line_10m, "chart-line-10m"),
            (chart_scatter_1m_100_series, "chart-scatter-1m-100-series"),
        );
    };
}

/// Shared workload list, extended by the wall-clock harnesses.
#[macro_export]
macro_rules! with_suites {
    ($callback:ident) => {
        $crate::with_suites!($callback,);
    };
    ($callback:ident, $(($function:ident, $name:literal),)*) => {
        $callback! {
            (core_variants, "core-variants"),
            $(($function, $name),)*
            (chart_line_million, "chart-line-1m"),
            (chart_scatter_hundred_thousand, "chart-scatter-100k"),
            (code_scroll_million,"code-scroll-1m"),
            (code_snapshot_arc_clone,"code-snapshot-arc-clone"),
            (datagrid_scroll_million, "datagrid-scroll-1m"),
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
crate::with_production_suites!(make_registry);
