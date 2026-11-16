use anyhow::Result;
use guiraffe_bench::allocator::{CountingAllocator, count_allocations, count_process_allocations};
use std::sync::{Arc, Barrier};
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// Keep the process-wide gate in a separate executable with a single test.
#[test]
fn process_scope_observes_worker_and_recovers_after_errors_and_panics() -> Result<()> {
    let start = Arc::new(Barrier::new(2));
    let done = Arc::new(Barrier::new(2));
    let worker = {
        let start = Arc::clone(&start);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            start.wait();
            let counted = count_allocations(|| std::hint::black_box(Box::new([0_u8; 64])));
            done.wait();
            counted.map(|(_, counts)| counts)
        })
    };
    let (_, process) = count_process_allocations(|| {
        start.wait();
        done.wait();
    })?;
    let thread = worker
        .join()
        .map_err(|_| anyhow::anyhow!("worker panic"))??;
    assert_eq!(thread.allocations, 1);
    assert_eq!(thread.bytes, 64);
    assert!(process.allocations >= thread.allocations);
    assert!(process.bytes >= thread.bytes);
    count_process_allocations(|| assert!(count_process_allocations(|| {}).is_err()))?;
    let _ = std::panic::catch_unwind(|| {
        let _ = count_process_allocations(|| panic!("reset process scope"));
    });
    let (_, counts) = count_process_allocations(|| std::hint::black_box(Box::new(8_u64)))?;
    assert_eq!(counts.allocations, 1);
    assert_eq!(counts.bytes, 8);
    Ok(())
}
