//! Live-heap budget for the 50k-node / 100k-edge document of the soak probe
//! (`bench/examples/soak.rs flow`). A counting global allocator tracks net live
//! requested bytes, so these assertions measure retained storage rather than
//! allocation traffic. The binary holds a single test so no other test thread
//! allocates while the counter is sampled.
#![allow(unsafe_code)]
use guiraffe_flow_core::{Edge, Editor, Graph, LayoutCancellation, Node, Point};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

struct LiveBytes;
static LIVE: AtomicIsize = AtomicIsize::new(0);

// SAFETY: every method forwards to the system allocator with the caller's
// arguments and only adjusts a counter afterwards.
unsafe impl GlobalAlloc for LiveBytes {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            LIVE.fetch_add(
                new_size as isize - layout.size() as isize,
                Ordering::Relaxed,
            );
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: LiveBytes = LiveBytes;

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

const NODES: u64 = 50_000;

/// The `soak flow` fixture: two edges per node, grid positions, `Node {i}` labels.
fn soak_graph() -> Graph {
    Graph::from_parts(
        (0..NODES).map(|i| {
            Node::new(
                i,
                format!("Node {i}"),
                Point::new((i % 100) as f32 * 240., (i / 100) as f32 * 150.),
            )
        }),
        (0..NODES * 2).map(|i| {
            let offset = if i < NODES { 1 } else { 100 };
            Edge::new(i, i % NODES, ((i % NODES) + offset) % NODES)
        }),
    )
    .unwrap_or_else(|error| panic!("soak graph: {error}"))
}

#[test]
fn soak_document_storage_stays_within_byte_budget() {
    let start = live();
    let graph = soak_graph();
    let graph_bytes = live() - start;
    let editor = Editor::new_deferred(graph).unwrap_or_else(|error| panic!("editor: {error}"));
    assert!(
        editor
            .scene()
            .prepare_indexes(&LayoutCancellation::default())
            .unwrap_or_else(|error| panic!("indexes: {error:?}"))
    );
    let indexed_bytes = live() - start;
    assert!(!editor.scene().incident_edges(0).is_empty());
    let total_bytes = live() - start;
    eprintln!(
        "graph {graph_bytes} B ({:.1} B per node with its two edges), \
         + indexes {indexed_bytes} B, + incidence {total_bytes} B",
        graph_bytes as f64 / NODES as f64
    );
    // Before shared handle names and packed maps this document took 43.4 MB:
    // half-full 64-entry map nodes doubled each entry and every node and edge
    // owned copies of "in", "out", "default" and its handle list.
    assert!(
        graph_bytes <= 20_000_000,
        "50k-node document uses {graph_bytes} B, budget 20,000,000 B (400 B per node + 2 edges)"
    );
    // The immutable BVHs and incidence lists add about 6.6 MB.
    assert!(
        total_bytes <= 27_000_000,
        "indexed 50k-node document uses {total_bytes} B, budget 27,000,000 B"
    );
    drop(editor);
}
