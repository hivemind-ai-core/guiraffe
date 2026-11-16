//! Opt-in allocator for per-thread allocation assertions.
//!
//! Install once in a test or benchmark executable:
//! ```
//! use guiraffe_bench::allocator::{CountingAllocator, count_allocations};
//! #[global_allocator]
//! static ALLOCATOR: CountingAllocator = CountingAllocator;
//! # fn main() -> anyhow::Result<()> {
//! let (_, counts) = count_allocations(|| std::hint::black_box(Box::new(42)))?;
//! assert_eq!(counts.allocations, 1);
//! # Ok(()) }
//! ```
//! Counts allocations/reallocations on the calling thread only; spawned workers need
//! their own scope. `count_process_allocations` includes every thread, so run its
//! gates in an isolated executable without unrelated background work. Bytes sum each
//! successful allocation's requested size; realloc adds the FULL new requested size,
//! not its growth delta. This is allocation traffic, not live/peak memory.
#![allow(unsafe_code)]
use anyhow::{Result, ensure};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// Successful allocations and cumulative requested bytes within a counting scope.
/// Reallocations add one operation and their entire new requested size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub allocations: u64,
    pub bytes: u64,
}
thread_local! {
    static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) };
}
static PROCESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static PROCESS_COUNTS: Mutex<Option<Counts>> = Mutex::new(None);

fn allocated(size: usize) {
    let _ = COUNTS.try_with(|cell| {
        if let Some(mut count) = cell.get() {
            count.allocations = count.allocations.saturating_add(1);
            count.bytes = count.bytes.saturating_add(size as u64);
            cell.set(Some(count));
        }
    });
    if PROCESS_ACTIVE.load(Ordering::Acquire) {
        let mut counts = PROCESS_COUNTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(count) = counts.as_mut() {
            count.allocations = count.allocations.saturating_add(1);
            count.bytes = count.bytes.saturating_add(size as u64);
        }
    }
}

/// System allocator with thread-local instrumentation; inert outside counting scopes.
pub struct CountingAllocator;
// SAFETY: Every allocation operation delegates unchanged to System. Instrumentation
// uses a const-initialized TLS Cell and optional mutex counters, never allocates,
// and tolerates TLS teardown. User callbacks never hold the accounting mutex.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies the valid layout required by GlobalAlloc.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Same contract as System.alloc_zeroed; no pointer transformation.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller provides the matching System allocation and layout.
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: GlobalAlloc's realloc preconditions are forwarded to System.
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            allocated(size);
        }
        result
    }
}
struct Scope;
impl Drop for Scope {
    fn drop(&mut self) {
        COUNTS.with(|cell| cell.set(None));
    }
}

/// Count one frame/operation. Nested scopes are rejected and panic unwinding resets
/// instrumentation. The returned value is dropped outside the counting scope.
pub fn count_allocations<T>(operation: impl FnOnce() -> T) -> Result<(T, Counts)> {
    ensure!(
        COUNTS.with(|cell| cell.get().is_none()),
        "allocation scopes cannot nest"
    );
    COUNTS.with(|cell| cell.set(Some(Counts::default())));
    let scope = Scope;
    let value = operation();
    let counts = COUNTS.with(|cell| cell.get().unwrap_or_default());
    drop(scope);
    Ok((value, counts))
}

struct ProcessScope {
    armed: bool,
}
impl Drop for ProcessScope {
    fn drop(&mut self) {
        if self.armed {
            let mut counts = PROCESS_COUNTS
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            PROCESS_ACTIVE.store(false, Ordering::Release);
            *counts = None;
        }
    }
}

/// Count successful allocations from ALL threads during the operation. Only one
/// process scope may be active; overlap/nesting returns an error. User callbacks
/// never execute under the accounting mutex. Panic unwinding resets the scope.
///
/// Coordinate worker start/finish inside the callback and run gates without other
/// tests or background work in the same process. Thread-local scopes may coexist.
/// Allocation hooks serialize count updates; this instrumentation is for allocation
/// assertions, not wall-clock benchmarking.
pub fn count_process_allocations<T>(operation: impl FnOnce() -> T) -> Result<(T, Counts)> {
    let acquired = {
        let mut counts = PROCESS_COUNTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if counts.is_none() {
            *counts = Some(Counts::default());
            PROCESS_ACTIVE.store(true, Ordering::Release);
            true
        } else {
            false
        }
    };
    // Construct errors outside the mutex: anyhow's own allocation is instrumented.
    ensure!(acquired, "process allocation scopes cannot overlap or nest");
    let mut scope = ProcessScope { armed: true };
    let value = operation();
    let counts = {
        let mut counts = PROCESS_COUNTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        PROCESS_ACTIVE.store(false, Ordering::Release);
        counts.take().unwrap_or_default()
    };
    // Release ownership before allowing a new scope; Drop must not reset its counts.
    scope.armed = false;
    Ok((value, counts))
}
