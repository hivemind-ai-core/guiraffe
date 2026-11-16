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
//! their own scope. Bytes are cumulative requested bytes, not live or peak memory.
#![allow(unsafe_code)]
use anyhow::{Result, ensure};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

/// Successful allocations and requested bytes within a counting scope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub allocations: u64,
    pub bytes: u64,
}
thread_local! {
    static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) };
}
fn allocated(size: usize) {
    let _ = COUNTS.try_with(|cell| {
        if let Some(mut count) = cell.get() {
            count.allocations = count.allocations.saturating_add(1);
            count.bytes = count.bytes.saturating_add(size as u64);
            cell.set(Some(count));
        }
    });
}

/// System allocator with thread-local instrumentation; inert outside counting scopes.
pub struct CountingAllocator;
// SAFETY: Every allocation operation delegates unchanged to System. Instrumentation
// uses a const-initialized TLS Cell, never allocates, and tolerates TLS teardown.
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
