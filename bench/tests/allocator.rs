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

#[test]
fn mounted_frame_allocations_do_not_grow_with_source_size() -> anyhow::Result<()> {
    use guiraffe_bench::suites::{
        code_frame_10m, code_frame_100k, datagrid_frame_10m, datagrid_frame_100k,
    };
    for (name, small, large, allocation_budget, byte_budget) in [
        (
            "datagrid",
            datagrid_frame_100k as fn(),
            datagrid_frame_10m as fn(),
            10_300,
            2_750 * 1024,
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
        eprintln!("{name}: 100k {small_counts:?}, 10M {large_counts:?}");
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
fn warmed_flow_queries_reuse_storage() -> anyhow::Result<()> {
    guiraffe_bench::suites::flow_query();
    let (_, counts) = count_allocations(guiraffe_bench::suites::flow_query)?;
    assert_eq!(counts.allocations, 0);
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
