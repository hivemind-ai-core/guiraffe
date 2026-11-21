//! Opt-in per-thread allocation counting for repaint tests.
//!
//! Install once per test binary and count the warm repaint:
//! ```ignore
//! use guiraffe_test::alloc::{CountingAllocator, count_allocations};
//! #[global_allocator]
//! static ALLOCATOR: CountingAllocator = CountingAllocator;
//! ```
//! Counts allocations/reallocations on the calling thread only (the GPUI test
//! foreground thread); realloc adds the full new requested size. This is
//! allocation traffic, not live or peak memory. Kept in lockstep with
//! `bench/src/allocator.rs`, which owns the instruction-count gates.
#![allow(unsafe_code)]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

/// Successful allocations and cumulative requested bytes within a counting scope.
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

// SAFETY: Every operation delegates unchanged to System. Instrumentation uses a
// const-initialized TLS Cell, never allocates, and tolerates TLS teardown.
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

/// Count one frame/operation on this thread. Nested scopes and panic unwinding
/// both reset instrumentation, so a failed scope cannot poison later tests.
pub fn count_allocations<T>(operation: impl FnOnce() -> T) -> (T, Counts) {
    assert!(
        COUNTS.with(|cell| cell.get().is_none()),
        "allocation scopes cannot nest"
    );
    COUNTS.with(|cell| cell.set(Some(Counts::default())));
    let scope = Scope;
    let value = operation();
    let counts = COUNTS.with(|cell| cell.get().unwrap_or_default());
    drop(scope);
    (value, counts)
}
