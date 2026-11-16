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
    for suite in guiraffe_bench::suites::registered() {
        let (_, counts) = count_allocations(suite.run)?;
        assert_eq!(counts.allocations, 0, "{} allocates", suite.name);
    }
    Ok(())
}
