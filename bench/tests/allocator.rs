use guiraffe_bench::allocator::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn counts_frame_allocations_and_resets() -> anyhow::Result<()> {
    let (value, counts) = count_allocations(|| std::hint::black_box(Box::new(42_u64)))?;
    assert_eq!(counts.allocations, 1);
    assert_eq!(counts.bytes, 8);
    assert_eq!(*value, 42);
    let (_, empty) = count_allocations(|| std::hint::black_box(12))?;
    assert_eq!(empty.allocations, 0);
    Ok(())
}
#[test]
fn reallocations_and_zeroed_allocations_count() -> anyhow::Result<()> {
    let (_, counts) = count_allocations(|| {
        // Preserve the small initial capacity so the second push exercises realloc.
        let mut values = std::hint::black_box(Vec::with_capacity(1));
        values.push(1_u64);
        values.push(2);
        std::hint::black_box(values)
    })?;
    assert_eq!(counts.allocations, 2);
    assert!(counts.bytes >= 24);
    let (_, counts) = count_allocations(|| std::hint::black_box(vec![0_u8; 512]))?;
    assert_eq!(counts.allocations, 1);
    assert_eq!(counts.bytes, 512);
    Ok(())
}
#[test]
fn rejects_nesting_and_recovers_from_panic() -> anyhow::Result<()> {
    let (_, counts) = count_allocations(|| assert!(count_allocations(|| {}).is_err()))?;
    assert!(counts.allocations > 0); // The nested-scope error itself allocates.
    let _ = std::panic::catch_unwind(|| {
        let _ = count_allocations(|| panic!("frame failed"));
    });
    assert!(count_allocations(|| {}).is_ok());
    Ok(())
}
#[test]
fn scopes_are_thread_local() -> anyhow::Result<()> {
    let worker =
        std::thread::spawn(|| count_allocations(|| std::hint::black_box(Box::new([0_u8; 64]))));
    let result = worker
        .join()
        .map_err(|_| anyhow::anyhow!("worker panicked"))??;
    assert_eq!(result.1.allocations, 1);
    assert_eq!(result.1.bytes, 64);
    Ok(())
}
#[test]
fn warmed_hot_paths_allocate_nothing() -> anyhow::Result<()> {
    for suite in guiraffe_bench::suites::registered()
        .into_iter()
        .filter(|s| {
            !s.name.starts_with("flow-")
                && !matches!(s.name, "timeline-box-select-100k" | "timeline-draw-bars-100k")
                && !s.name.starts_with("data-ui-")
                && !s.name.starts_with("warm-")
                && s.name != "datagrid-sort-1m"
                && s.name != "code-index-10m"
                // Compiling a chart scene for a new viewport builds its output geometry; it is
                // budgeted below (bounded by the viewport, independent of data size). Repaints
                // reuse the cached scene.
                && !s.name.starts_with("chart-")
                && !s.name.contains("-frame-")
        })
    {
        // Immutable code fixtures, like flow fixtures, are initialized during warmup.
        (suite.run)();
        let (_, counts) = count_allocations(suite.run)?;
        assert_eq!(counts.allocations, 0, "{} allocates", suite.name);
    }
    Ok(())
}

/// A warm repaint of an unchanged view may not grow: the same frame, drawn
/// again, allocates the same. The budget is what one frame of each family
/// costs today; a regression in a component's render path shows up here.
#[test]
fn warm_repaints_allocate_a_bounded_amount() -> anyhow::Result<()> {
    for suite in guiraffe_bench::suites::registered()
        .into_iter()
        .filter(|s| s.name.starts_with("warm-"))
    {
        (suite.run)(); // Mount and warm the fixture.
        (suite.run)();
        (suite.run)();
        let (_, counts) = count_allocations(suite.run)?;
        let (_, again) = count_allocations(suite.run)?;
        eprintln!("{}: {counts:?} then {again:?}", suite.name);
        assert_eq!(
            counts.allocations, again.allocations,
            "{} allocates more on every repaint",
            suite.name
        );
        // The budget is what one frame of the family costs today. It is the
        // crates' own render paths that are counted here, so a component that
        // starts allocating on every repaint shows up.
        let budget = match suite.name {
            "warm-components" => 400,
            "warm-inputs" => 500,
            "warm-layout" => 250,
            "warm-overlays" => 100,
            // A week of 41 events: toolbar, all-day lane, virtualized hours,
            // overlap lanes, business hours and the now-line (2,892 today).
            "warm-calendar" => 3_200,
            _ => 4_000,
        };
        assert!(
            counts.allocations <= budget,
            "{} allocates {} on a warm repaint, over its budget of {budget}",
            suite.name,
            counts.allocations
        );
    }
    Ok(())
}

#[test]
fn mounted_frame_allocations_do_not_grow_with_source_size() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{
        code_frame_10m, code_frame_100k, datagrid_frame_10k, datagrid_frame_10m,
    };
    let (grid_allocations, grid_bytes) = if gpui_kit::GPUI_FAST {
        (1_900, 420 * 1024)
    } else {
        (1_050, 440 * 1024)
    };
    for (name, small, large, allocation_budget, byte_budget) in [
        (
            "datagrid",
            datagrid_frame_10k as fn(),
            datagrid_frame_10m as fn(),
            grid_allocations,
            grid_bytes,
        ),
        (
            "code",
            code_frame_100k as fn(),
            code_frame_10m as fn(),
            160,
            192 * 1024,
        ),
    ] {
        small();
        large();
        let (_, small_counts) = count_allocations(small)?;
        let (_, large_counts) = count_allocations(large)?;
        eprintln!("{name}: small {small_counts:?}, 10M {large_counts:?}");
        assert_eq!(small_counts.allocations, large_counts.allocations, "{name}");
        assert!(
            large_counts.bytes <= small_counts.bytes + 4096,
            "{name} requested bytes grew with source size"
        );
        for counts in [small_counts, large_counts] {
            assert!(
                counts.allocations <= allocation_budget && counts.bytes <= byte_budget,
                "{name} mounted frame allocation budget exceeded: {counts:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn calendar_frame_allocations_do_not_grow_with_source_size() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{calendar_frame_10k, calendar_frame_100k};
    calendar_frame_10k();
    calendar_frame_100k();
    let (_, small) = count_allocations(calendar_frame_10k)?;
    let (_, large) = count_allocations(calendar_frame_100k)?;
    eprintln!("calendar: 10k {small:?}, 100k {large:?}");
    assert!(
        large.allocations <= small.allocations,
        "calendar frame allocations grew with source size: {small:?} vs {large:?}"
    );
    assert!(
        large.bytes <= small.bytes + 4096,
        "calendar frame bytes grew with source size: {small:?} vs {large:?}"
    );
    Ok(())
}

#[test]
fn warmed_flow_queries_reuse_storage() -> anyhow::Result<()> {
    guiraffe_bench::suites::flow_query();
    let (_, counts) = count_allocations(guiraffe_bench::suites::flow_query)?;
    assert_eq!(counts.allocations, 0);
    Ok(())
}

// Flow rendering

#[test]
fn flow_default_short_label_uses_only_the_element_allocation() -> anyhow::Result<()> {
    use gpui_kit::{AppContext, EmptyView, TestAppContext, TestDispatcher, WindowOptions};
    use guiraffe_flow::{
        DefaultNodeRenderer, NodeRenderer,
        core::{Node, Point},
    };

    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("flow_default_label"));
    let (window, _) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| EmptyView))
    })?;
    let node = Node::new(1, "Flow node", Point::default());
    let (_, counts) = window.update(&mut cx, |_, window, cx| {
        drop(DefaultNodeRenderer.render(&node, 1., window, cx));
        count_allocations(|| {
            std::hint::black_box(DefaultNodeRenderer.render(&node, 1., window, cx))
        })
    })??;
    assert_eq!(counts.allocations, 1, "{counts:?}");
    Ok(())
}

#[test]
fn production_flow_pan_and_hit_reuse_storage() -> anyhow::Result<()> {
    use guiraffe_bench::suites::flow_pan_hit_50k;
    flow_pan_hit_50k();
    let (_, counts) = count_allocations(flow_pan_hit_50k)?;
    assert_eq!(counts.allocations, 0);
    Ok(())
}

#[test]
fn production_timeline_edits_have_bounded_allocation_traffic() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{timeline_box_select_100k, timeline_draw_bars_100k};
    timeline_box_select_100k();
    timeline_draw_bars_100k();
    let (_, selection) = count_allocations(timeline_box_select_100k)?;
    let (_, bars) = count_allocations(timeline_draw_bars_100k)?;
    assert!(selection.allocations <= 64, "{selection:?}");
    assert!(selection.bytes <= 16 * 1024, "{selection:?}");
    assert_eq!(bars.allocations, 0);
    Ok(())
}

#[test]
fn data_ui_native_workloads_have_bounded_allocation_traffic() -> anyhow::Result<()> {
    guiraffe_bench::suites::prepare_data_ui();
    for suite in guiraffe_bench::suites::registered()
        .into_iter()
        .filter(|suite| suite.name.starts_with("data-ui-"))
    {
        (suite.run)(); // Initialize native platform, template, list and caches outside counting.
        (suite.run)();
        let (_, counts) = count_allocations(suite.run)?;
        eprintln!(
            "{}: {} allocations, {} requested bytes",
            suite.name, counts.allocations, counts.bytes
        );
        let (allocation_budget, byte_budget) = match suite.name {
            "data-ui-instantiate" => (24, 4096),
            "data-ui-patch" => (12, 2048),
            _ => (64, 131_072),
        };
        assert!(
            counts.allocations <= allocation_budget,
            "{} allocation regression: {counts:?}",
            suite.name
        );
        assert!(
            counts.bytes <= byte_budget,
            "{} copies the 10k list: {counts:?}",
            suite.name
        );
    }
    guiraffe_bench::suites::prepare_data_ui();
    Ok(())
}

#[test]
fn datagrid_sort_allocates_only_the_permutation() -> anyhow::Result<()> {
    guiraffe_bench::suites::datagrid_sort_million();
    let (_, counts) = count_allocations(guiraffe_bench::suites::datagrid_sort_million)?;
    eprintln!("datagrid-sort-1m: {counts:?}");
    // Keyed (CellRef, u32) pairs, std's stable-sort scratch and the final u32 permutation;
    // never a copy of row data.
    assert!(counts.allocations <= 8, "{counts:?}");
    assert!(counts.bytes <= 120_000_000, "{counts:?}");
    Ok(())
}

#[test]
fn code_document_index_allocations_are_bounded() -> anyhow::Result<()> {
    // Warm the shared source before measuring only index construction.
    guiraffe_bench::suites::code_index_ten_million();
    let (_, counts) = count_allocations(guiraffe_bench::suites::code_index_ten_million)?;
    assert_eq!(counts.allocations, 0, "{counts:?}");
    assert_eq!(counts.bytes, 0, "{counts:?}");
    Ok(())
}

#[test]
fn grid_permutation_and_inverse_stay_within_row_budget() -> anyhow::Result<()> {
    use guiraffe_datagrid::{
        CellRef, FnSource,
        order::{FilterSpec, RowOrder, compute_order},
    };
    use std::sync::{Arc, atomic::AtomicBool};
    let rows = 1_000_000;
    let source = FnSource::new(rows, |row, _| CellRef::Int(row as i64));
    let filter = FilterSpec {
        source_col: 0,
        pred: Arc::new(|_| true),
    };
    let order = compute_order(&source, &[filter], &[], &AtomicBool::new(false))
        .map_err(|_| anyhow::anyhow!("unexpected cancellation"))?;
    let RowOrder::Indices(indices) = &order else {
        anyhow::bail!("filtered row order must store a permutation");
    };
    let (inverse, counts) = count_allocations(|| order.inverse(rows))?;
    assert_eq!(inverse.view_row(rows - 1), Some(rows - 1));
    assert!(std::mem::size_of_val(indices.as_ref()) as u64 + counts.bytes <= 16 * rows as u64);
    Ok(())
}

#[test]
fn one_item_splice_shares_an_immutable_large_list_snapshot() -> anyhow::Result<()> {
    use guiraffe_ui::{PatchBuf, SlotId, SlotTable, Value};
    use std::sync::Arc;
    for len in [1_000, 100_000] {
        let mut table = SlotTable::with_defaults(&[Value::List(
            (0..len).map(|i| Value::from(i as i64)).collect(),
        )]);
        let snapshot = table.clone();
        let mut out = PatchBuf::default();
        out.splice(
            SlotId(0),
            len / 2..len / 2 + 1,
            Arc::from([Value::from(-1i64)]),
        );
        let (changed, counts) = count_allocations(|| table.apply(&mut out))?;
        assert_eq!(changed, 1);
        assert_eq!(
            snapshot.get(SlotId(0)).list_get(len / 2),
            Some(&Value::from((len / 2) as i64))
        );
        assert_eq!(
            table.get(SlotId(0)).list_get(len / 2),
            Some(&Value::from(-1i64))
        );
        assert!(
            counts.bytes < 131_072,
            "splice copied the {len}-item snapshot: {counts:?}"
        );
        eprintln!(
            "snapshot splice {len}: {} allocations / {} bytes",
            counts.allocations, counts.bytes
        );
    }
    Ok(())
}

#[test]
fn chart_scene_compile_allocations_are_bounded_by_the_viewport_not_the_data() -> anyhow::Result<()>
{
    // Warm both suites, then count one scene compile each.
    guiraffe_bench::suites::chart_line_million();
    guiraffe_bench::suites::chart_scatter_hundred_thousand();
    let (_, line) = count_allocations(guiraffe_bench::suites::chart_line_million)?;
    let (_, scatter) = count_allocations(guiraffe_bench::suites::chart_scatter_hundred_thousand)?;
    // A 1M-point line (two viewports) and a 100k scatter stay within a small fixed budget.
    const BUDGET: u64 = 1024;
    assert!(
        line.allocations <= BUDGET,
        "chart-line-1m allocates {} (> {BUDGET})",
        line.allocations
    );
    assert!(
        scatter.allocations <= BUDGET,
        "chart-scatter-100k allocates {} (> {BUDGET})",
        scatter.allocations
    );
    Ok(())
}

// The budget is zero allocations for warmed CPU layout traversal at overview and
// detail zoom. Native painting shapes at most one label per visible segment wider
// than 32 px. GPU scene storage and the text shaper are outside this counter.
#[test]
fn warmed_flamegraph_million_frame_traversal_allocates_nothing() -> anyhow::Result<()> {
    use guiraffe_bench::suites::flamegraph_million;
    flamegraph_million();
    let (_, counts) = count_allocations(flamegraph_million)?;
    assert_eq!(counts.allocations, 0);
    Ok(())
}

#[test]
fn large_timeline_bar_storage_is_one_value_per_bar() -> anyhow::Result<()> {
    const BARS: usize = 100_000;
    let (bars, storage) = count_allocations(|| guiraffe_timeline::BarCurve::new(BARS, -1., 1.))?;
    assert_eq!(bars.values().len(), BARS);
    // One shared allocation containing the vector header, plus the value buffer.
    assert!(storage.allocations <= 2, "{storage:?}");
    assert!(
        storage.bytes <= (BARS * size_of::<f64>() + 64) as u64,
        "{storage:?}"
    );
    Ok(())
}

#[test]
fn production_scatter_recompute_bounds_allocation_traffic() -> anyhow::Result<()> {
    use guiraffe_bench::suites::chart_scatter_1m_100_series;
    chart_scatter_1m_100_series();
    let (_, counts) = count_allocations(chart_scatter_1m_100_series)?;
    assert!(counts.allocations <= 1_024, "{counts:?}");
    assert!(counts.bytes <= 128 * 1024 * 1024, "{counts:?}");
    Ok(())
}

#[test]
fn flow_layout_allocation_traffic_scales_linearly() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{flow_layout_5k, flow_layout_50k};

    for (nodes, workload) in [(5_000, flow_layout_5k as fn()), (50_000, flow_layout_50k)] {
        workload();
        let (_, counts) = count_allocations(workload)?;
        eprintln!("flow layout {nodes}: {counts:?}");
        assert!(
            counts.allocations <= 15 * nodes,
            "allocation count: {counts:?}"
        );
        assert!(counts.bytes <= 2_500 * nodes, "requested bytes: {counts:?}");
    }
    Ok(())
}

#[test]
fn production_flame_and_tree_model_traversals_allocate_nothing() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{flamegraph_ten_million, tree_scroll_million};

    for suite in [flamegraph_ten_million as fn(), tree_scroll_million] {
        // Construct the immutable index/visible rows before entering the
        // allocator scope. This only counts the warmed model traversal.
        suite();
        let (_, counts) = count_allocations(suite)?;
        assert_eq!(counts.allocations, 0, "{counts:?}");
    }
    Ok(())
}

#[test]
fn tree_tail_expansion_uses_one_visible_row_buffer() -> anyhow::Result<()> {
    use guiraffe_tree::{Children, NodeId, TreeModel, TreeSource};

    struct Source(Vec<NodeId>);

    impl TreeSource for Source {
        fn roots(&self) -> &[NodeId] {
            &[0]
        }

        fn children(&self, id: NodeId) -> Children<'_> {
            Children::Ready(if id == 0 { &self.0 } else { &[] })
        }

        fn label(&self, _: NodeId) -> &str {
            "node"
        }
    }

    for count in [125_000_u64, 1_000_000] {
        let source = Source((1..=count).collect());
        let mut model = TreeModel::new(&source);
        let (_, counts) = count_allocations(|| model.set_expanded(0, true, &source))?;
        assert!(
            counts.bytes <= 185 * count,
            "tail expansion allocated a second set of rows: {counts:?}"
        );
        assert_eq!(model.row_index(count), Some(count as usize));
    }
    Ok(())
}

#[test]
fn ui_macro_matches_builder_allocations_for_a_thousand_nodes() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{ui_builder_tree, ui_macro_tree};

    for _ in 0..8 {
        ui_builder_tree();
        ui_macro_tree();
    }
    let (_, builder) = count_allocations(ui_builder_tree)?;
    let (_, declarative) = count_allocations(ui_macro_tree)?;
    assert_eq!(declarative, builder);
    Ok(())
}

// Charts

#[inline(never)]
fn draw_cached_chart(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    window.draw(cx).clear(cx);
}

#[gpui_kit::test]
fn warmed_chart_replays_cached_ink(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{Context, Entity, IntoElement, Render, StyleRefinement, Styled, Window, px};
    use guiraffe_charts::{Channels, ChartOptions, ChartSpec, ChartView, Layer, Mark};

    struct CachedChart(Entity<ChartView>);
    impl Render for CachedChart {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0
                .clone()
                .cached(StyleRefinement::default().w_full().h(px(290.)))
        }
    }

    cx.update(|cx| {
        gpui_kit::base::init(cx);
        gpui_kit::component::init(cx);
        guiraffe_core::apply(Default::default(), cx);
    });
    let (root, window) = cx.add_window_view(|_, cx| {
        let spec = ChartSpec::new("Cached scatter").layer(Layer::new(
            Mark::Point,
            (0..10_000).collect::<Vec<_>>(),
            Channels::new(|i| f64::from(*i), |i| (f64::from(*i) * 0.01).sin())
                .group(|i| (i % 100) as u32),
        ));
        let options = ChartOptions {
            legend: false,
            navigation: false,
            ..Default::default()
        };
        CachedChart(ChartView::new(spec.prepare(), options, cx))
    });
    window.run_until_parked();
    window.update(|window, cx| {
        for _ in 0..4 {
            window.draw(cx).clear(cx);
        }
        let chart = root.read(cx).0.clone();
        let paints = chart.read(cx).canvas_paint_count();
        let builds = chart.read(cx).scene_build_count();
        assert!(paints > 0 && builds > 0, "chart never mounted");
        let (_, counts) =
            count_allocations(|| draw_cached_chart(window, cx)).expect("count frame allocations");
        eprintln!("cached chart frame: {counts:?}");
        // GPUI duplicates the vertex buffers during cached scene replay.
        // Bound the full draw until the backend shares those buffers.
        let (allocations, bytes) = if gpui_kit::GPUI_FAST {
            (80, 24 * 1024)
        } else {
            (100, 96 * 1024)
        };
        assert!(counts.allocations <= allocations, "{counts:?}");
        assert!(counts.bytes <= bytes, "{counts:?}");
        assert_eq!(chart.read(cx).canvas_paint_count(), paints);
        assert_eq!(chart.read(cx).scene_build_count(), builds);
    });
}

// Mermaid

#[test]
fn mermaid_warm_repaint_allocation_traffic_is_independent_of_node_count() -> anyhow::Result<()> {
    use gpui_kit::{
        AppContext, Context, Entity, HeadlessAppContext, IntoElement, Render, Window, px, size,
    };
    use guiraffe_bench::allocator::Counts;
    use guiraffe_mermaid::{MermaidView, MermaidViewState, view::Status};
    use std::{fmt::Write as _, sync::Arc};

    const FRAMES: u64 = 8;

    struct Diagram(Entity<MermaidViewState>);

    impl Render for Diagram {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            MermaidView::new("allocation-diagram").state(&self.0)
        }
    }

    fn measure(nodes: usize) -> anyhow::Result<Counts> {
        let mut source = String::from("flowchart TD\n");
        for i in (0..nodes).step_by(2) {
            writeln!(source, "  n{i} --> n{}", i + 1)?;
        }
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut app =
            HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), || {
                Ok(None)
            });
        app.update(guiraffe_core::init);
        let window = app.open_window(size(px(1200.), px(800.)), |_, cx| {
            let diagram = MermaidView::create_state("allocation-diagram", cx);
            if nodes != 0 {
                diagram.update(cx, |state, cx| state.set_source(source.into(), false, cx));
            }
            cx.new(|_| Diagram(diagram))
        })?;
        app.run_until_parked();
        for _ in 0..FRAMES {
            app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
            app.run_until_parked();
        }
        app.update_window(window.into(), |host, window, cx| {
            let diagram = host
                .downcast::<Diagram>()
                .expect("Mermaid test host")
                .read(cx)
                .0
                .clone();
            let state = diagram.read(cx);
            if nodes == 0 {
                assert_eq!(state.status(), &Status::Empty);
            } else {
                assert_eq!(state.status(), &Status::Ready);
            }
            let image = state.image().map(|image| image.image.clone());
            let (_, counts) = count_allocations(|| {
                for _ in 0..FRAMES {
                    window.draw(cx).clear(cx);
                }
            })?;
            if let Some(image) = image {
                assert!(Arc::ptr_eq(
                    &image,
                    &diagram.read(cx).image().expect("unchanged diagram").image
                ));
            }
            Ok(counts)
        })?
    }

    let empty = measure(0)?;
    let small = measure(50)?;
    let large = measure(500)?;
    eprintln!(
        "Mermaid allocations over eight draws: empty {empty:?}, 50 nodes {small:?}, 500 nodes {large:?}"
    );
    assert_eq!(empty, small, "painting a diagram adds no warm allocations");
    assert_eq!(small, large);
    assert!(
        large.allocations <= FRAMES * 12,
        "warm redraw must reuse the Mermaid view: {large:?}"
    );
    Ok(())
}

#[test]
fn layered_code_viewport_queries_allocate_nothing_at_both_scales() -> anyhow::Result<()> {
    use guiraffe_code::{
        AnchorBias, CodeDocument, DecorationInput, DecorationStyle, Decorations, Position,
        SourceRange,
    };
    use std::sync::Arc;

    for count in [10_000, 1_000_000] {
        let document = Arc::new(CodeDocument::new("x\n".repeat(count)));
        let mut decorations = Decorations::new(document)?;
        let style = decorations.intern_style(DecorationStyle::Deprecated)?;
        decorations.update_layer(
            0,
            Default::default(),
            &[],
            (0..count).map(|line| DecorationInput {
                id: line as u64,
                range: SourceRange::new(Position::new(line, 0), Position::new(line, 1)),
                style,
                start_bias: AnchorBias::Left,
                end_bias: AnchorBias::Right,
            }),
        )?;
        let layer = decorations.layer(0).expect("published layer");
        let (visible, counts) = count_allocations(|| {
            let mut visible = 0;
            for line in 100..140 {
                layer.query((line * 2)..(line * 2 + 1), |mark| {
                    std::hint::black_box(mark);
                    visible += 1;
                });
            }
            visible
        })?;
        assert_eq!(visible, 40);
        assert_eq!(counts.allocations, 0, "{counts:?}");
        assert_eq!(counts.bytes, 0, "{counts:?}");
    }
    Ok(())
}

#[gpui_kit::test]
fn cached_annotated_code_viewer_reuses_its_render(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{
        App, AppContext, Context, Entity, IntoElement, Render, StyleRefinement, Styled, Window, px,
    };
    use guiraffe_code::{
        AnchorBias, CodeDocument, CodeHandle, CodeViewer, DecorationInput, DecorationStyle,
        Decorations, Position, Severity, SourceRange, UnderlinePattern,
    };
    use std::sync::Arc;

    struct Viewer(CodeHandle, Arc<Decorations>);
    impl Render for Viewer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            CodeViewer::new(&self.0)
        }
    }

    struct CachedRoot(Entity<Viewer>);
    impl Render for CachedRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0
                .clone()
                .cached(StyleRefinement::default().w(px(600.)).h(px(400.)))
        }
    }

    let (view, window) = cx.add_window_view(|_, cx| {
        let document = Arc::new(CodeDocument::new("reference\n".repeat(10_000)));
        let code = CodeHandle::new(1, document.clone(), cx);
        let mut decorations = Decorations::new(document).expect("compact document");
        let style = decorations
            .intern_style(DecorationStyle::Diagnostic {
                severity: Severity::Error,
                pattern: UnderlinePattern::Wavy,
                message: "Test diagnostic".into(),
            })
            .expect("interned style");
        decorations
            .update_layer(
                0,
                Default::default(),
                &[],
                [DecorationInput {
                    id: 1,
                    range: SourceRange::new(Position::new(0, 0), Position::new(0, 8)),
                    style,
                    start_bias: AnchorBias::Left,
                    end_bias: AnchorBias::Right,
                }],
            )
            .expect("prepared layer");
        CachedRoot(cx.new(|_| Viewer(code, Arc::new(decorations))))
    });
    window.run_until_parked();
    let (code, decorations) = window.update(|_, cx| {
        let viewer = view.read(cx).0.read(cx);
        (viewer.0.clone(), viewer.1.clone())
    });
    let warm = |window: &mut Window, cx: &mut App| {
        for _ in 0..8 {
            window.draw(cx).clear(cx);
        }
        let before = code.stats();
        let (_, counts) = count_allocations(|| {
            for _ in 0..8 {
                window.draw(cx).clear(cx);
            }
        })
        .expect("measured cached viewer draw");
        assert_eq!(code.stats(), before);
        counts
    };
    let plain = window.update(warm);
    window.update(|_, cx| assert!(code.set_decorations(Some(decorations), cx)));
    window.run_until_parked();
    let annotated = window.update(warm);
    eprintln!("eight warm draws: plain {plain:?}, annotated {annotated:?}");
    assert_eq!(plain, annotated);
}

#[gpui_kit::test]
fn unchanged_list_draw_allocations_are_independent_of_list_length(
    cx: &mut gpui_kit::TestAppContext,
) {
    use gpui_kit::{
        Context, IntoElement, ListAlignment, ListState, Render, Styled, Window, div, list, px,
    };
    use std::{cell::Cell, rc::Rc};

    struct ListFixture {
        list: ListState,
        rows_rendered: Rc<Cell<usize>>,
    }

    impl Render for ListFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rows_rendered = self.rows_rendered.clone();
            list(self.list.clone(), move |_, _, _| {
                rows_rendered.set(rows_rendered.get() + 1);
                div().w_full().h(px(20.)).into_any_element()
            })
            .w(px(320.))
            .h(px(200.))
        }
    }

    let mut measure = |length| {
        let rows_rendered = Rc::new(Cell::new(0));
        let (_, window) = cx.add_window_view(|_, _| ListFixture {
            list: ListState::new(length, ListAlignment::Top, px(40.))
                .with_uniform_item_height(px(20.)),
            rows_rendered: rows_rendered.clone(),
        });
        window.update(|window, cx| {
            for _ in 0..4 {
                window.refresh();
                window.draw(cx).clear(cx);
            }
            rows_rendered.set(0);
            let (_, counts) = count_allocations(|| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .expect("count unchanged list draw");
            (rows_rendered.get(), counts)
        })
    };

    let small = measure(10_000);
    let large = measure(100_000);
    eprintln!("plain list: 10,000 items {small:?}, 100,000 items {large:?}");
    assert!(small.0 > 0, "the warm draw must lay out visible rows");
    assert_eq!(small, large, "list length changed warm viewport work");
}

#[test]
fn registered_protocol_dispatch_and_repaint() -> anyhow::Result<()> {
    use guiraffe_bench::protocol::Fixture;
    let mut unused = Fixture::new(false)?;
    let mut registered = Fixture::new(true)?;
    let (plain, baseline) = count_allocations(|| unused.draw())?;
    plain?;
    let (frame, bound) = count_allocations(|| registered.draw())?;
    frame?;
    eprintln!("protocol repaint: unregistered {baseline:?}, registered {bound:?}");
    assert_eq!(bound.allocations, baseline.allocations);
    assert_eq!(bound.bytes, baseline.bytes);
    registered.dispatch(|call| call())?;
    let (_, counts) = registered.dispatch(|call| count_allocations(call))??;
    assert_eq!(counts.allocations, 0);
    registered.dispatch_batch(|call| call())?;
    let (_, batch) = registered.dispatch_batch(|call| count_allocations(call))??;
    assert_eq!(batch.allocations, 0, "a batch of eight dispatches inline");
    Ok(())
}

#[test]
fn adopted_controls_cost_the_same_bound_or_unbound() -> anyhow::Result<()> {
    use guiraffe_bench::protocol::Controls;
    let mut unbound = Controls::new(false)?;
    let mut bound = Controls::new(true)?;
    assert_eq!((unbound.registered()?, bound.registered()?), (0, 1000));
    let (plain, baseline) = count_allocations(|| unbound.draw())?;
    plain?;
    let (frame, registered) = count_allocations(|| bound.draw())?;
    frame?;
    eprintln!("adopted controls: unbound {baseline:?}, bound {registered:?}");
    assert_eq!(registered.allocations, baseline.allocations);
    assert_eq!(registered.bytes, baseline.bytes);
    Ok(())
}

// Undo history

#[derive(Clone, Default, PartialEq)]
struct Counter(u64);

#[derive(Debug)]
struct Bump;
impl guiraffe_state::Event<Counter> for Bump {
    fn handle(self, fx: &mut guiraffe_state::Fx<'_, Counter>) {
        // The in-place keyed write: no new state Arc, so only history could allocate.
        let next = fx.state().0 + 1;
        fx.set_keyed_field(1, next, |c| &c.0, |c| &mut c.0);
    }
}

#[test]
fn disabled_undo_history_allocates_nothing_on_the_push_path() -> anyhow::Result<()> {
    use guiraffe_state::{BoundedHistory, Coeffects, Engine, HistoryLimits};
    let cofx = Coeffects::default();
    for limits in [None, Some(HistoryLimits::DISABLED)] {
        let mut engine = Engine::new(Counter::default());
        if let Some(limits) = limits {
            engine.enable_undo(limits);
        }
        // Warm the queue, then time only processing (the event box is the dispatch's cost).
        engine.dispatch(Bump);
        engine.process(&cofx);
        engine.dispatch(Bump);
        let (_, counts) = count_allocations(|| {
            engine.begin_undo_group();
            let out = engine.process(&cofx);
            engine.end_undo_group();
            out.changed
        })?;
        assert_eq!(counts.allocations, 0, "{limits:?}: {counts:?}");
        assert!(!engine.can_undo());
    }
    let mut history = BoundedHistory::new(HistoryLimits::DISABLED);
    let (_, counts) = count_allocations(|| history.push(vec![0_u8; 0], 1))?;
    assert_eq!(counts.allocations, 0);
    Ok(())
}

#[test]
fn store_writes_outside_a_batch_allocate_nothing_extra() -> anyhow::Result<()> {
    use guiraffe_state::Store;
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app = gpui_kit::HeadlessAppContext::with_platform(
        text,
        std::sync::Arc::new(gpui_kit::assets::Assets),
        || Ok(None),
    );
    app.update(|cx| -> anyhow::Result<()> {
        let store = Store::new(Counter::default(), cx);
        // Warm the store; the batch check reads an absent global and then a present one.
        store.dispatch_now(Bump, cx);
        let (_, absent) = count_allocations(|| store.dispatch_now(Bump, cx))?;
        assert_eq!(absent.allocations, 0, "no batch global: {absent:?}");
        cx.default_global::<Marker>();
        let (_, counts) = count_allocations(|| store.dispatch_now(Bump, cx))?;
        assert_eq!(counts.allocations, 0, "{counts:?}");
        assert_eq!(store.state(cx).0, 3);
        Ok(())
    })
}

/// Any global, so the app's global map is non-empty like a real app's.
#[derive(Default)]
struct Marker;
impl gpui_kit::Global for Marker {}

// TracksView 0.3.1: painter, mixed heights and selection

struct Notes;
impl guiraffe_timeline::ClipPainter for Notes {
    fn paint(
        &self,
        clip: &guiraffe_timeline::ClipPaint<'_>,
        window: &mut gpui_kit::Window,
        _: &gpui_kit::App,
    ) {
        let b = clip.bounds;
        for k in 0..16 {
            window.paint_quad(gpui_kit::fill(
                gpui_kit::Bounds::new(
                    b.origin
                        + gpui_kit::point(
                            gpui_kit::px(k as f32 * 3.),
                            gpui_kit::px((k % 12) as f32),
                        ),
                    gpui_kit::size(gpui_kit::px(2.), gpui_kit::px(1.)),
                ),
                gpui_kit::rgb(0x33_66_cc),
            ));
        }
    }
}

struct TracksRoot(gpui_kit::Entity<guiraffe_timeline::TracksView>);
impl gpui_kit::Render for TracksRoot {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{ParentElement, Styled};
        gpui_kit::div().size_full().child(self.0.clone())
    }
}

/// Allocations of one repaint of a 64 x 200 TracksView after a notify.
fn tracks_repaint(painter: bool, mixed: bool) -> anyhow::Result<guiraffe_bench::allocator::Counts> {
    use gpui_kit::AppContext;
    use guiraffe_timeline::{
        TimeFormat, Track, TracksView,
        tracks_model::{ClipRef, SetSelection, TrackRow, TracksModel, tracks_store},
    };
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app = gpui_kit::HeadlessAppContext::with_platform(
        text,
        std::sync::Arc::new(gpui_kit::assets::Assets),
        || Ok(None),
    );
    let mut handle = None;
    let window = app.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        |_, cx| {
            guiraffe_timeline::init(cx);
            let rows = (0..64)
                .map(|t| {
                    let mut track = Track::default();
                    for k in 0..200 {
                        track.insert(k as f64 * 2. + (t % 4) as f64 * 0.25, 1.5);
                    }
                    // 30 + 50 per pair keeps the same rows in view as 40 + 40.
                    let h = if mixed { [30., 50.][t % 2] } else { 40. };
                    TrackRow::new(format!("Track {t}"), track).with_height(h)
                })
                .collect();
            let store = tracks_store(TracksModel::new(rows), 100, cx);
            store.dispatch_now(SetSelection(Some(ClipRef { track: 1, clip: 3 })), cx);
            let view = TracksView::with_store(store, TimeFormat::Seconds, 40., cx);
            if painter {
                view.update(cx, |v, cx| {
                    v.set_clip_painter(Some(std::rc::Rc::new(Notes)), cx)
                });
            }
            handle = Some(view.clone());
            cx.new(|_| TracksRoot(view))
        },
    )?;
    let view = handle.ok_or_else(|| anyhow::anyhow!("no view"))?;
    let frame = |app: &mut gpui_kit::HeadlessAppContext| {
        app.update_window(window.into(), |_, w, cx| {
            view.update(cx, |_, cx| cx.notify());
            w.draw(cx).clear(cx)
        })
    };
    for _ in 0..6 {
        frame(&mut app)?;
        app.run_until_parked();
    }
    let (result, counts) = count_allocations(|| frame(&mut app))?;
    result?;
    Ok(counts)
}

#[test]
fn tracks_painter_heights_and_selection_add_no_frame_allocations() -> anyhow::Result<()> {
    let plain = tracks_repaint(false, false)?;
    let painted = tracks_repaint(true, false)?;
    let mixed = tracks_repaint(true, true)?;
    eprintln!("tracks repaint: plain {plain:?}, painter {painted:?}, mixed heights {mixed:?}");
    assert_eq!(
        painted.allocations, plain.allocations,
        "the clip painter allocates nothing"
    );
    assert_eq!(
        mixed.allocations, plain.allocations,
        "variable heights allocate nothing"
    );
    Ok(())
}
