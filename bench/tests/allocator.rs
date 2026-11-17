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
fn initial_hot_paths_allocate_nothing() -> anyhow::Result<()> {
    for suite in guiraffe_bench::suites::registered()
        .into_iter()
        .filter(|s| !s.name.starts_with("flow-") && !s.name.starts_with("data-ui-"))
    {
        let (_, counts) = count_allocations(suite.run)?;
        assert_eq!(counts.allocations, 0, "{} allocates", suite.name);
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
