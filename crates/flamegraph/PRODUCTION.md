# FlameGraph performance

The `flamegraph` scenario in `bench/examples/soak.rs` mounts the real canvas
with 10 million frames and a stack depth of 500. It dispatches wheel zoom,
drag pan and hover in a 1280 × 800 headless GPUI window. The mount figure
includes constructing the layout before opening the window.

The 0.2 baseline at `65e854a` spent 80.37% of sampled self time in GPUI
`BoundsTree::insert` and 6.44% inserting frame locations. This branch
coalesces touching same-style aggregate backgrounds within each visible
row. Standard frame paints and labels keep their geometry and order.
Exact hit testing, highlighting, and the public segment iterator retain
the source frame boundaries.

## Storage bounds

| Storage | Limit |
| --- | --- |
| Label text cache | 128 direct-mapped entries per graph, 256 bytes per entry. Total payload at most 32 KiB plus fixed handles and allocator overhead. Short strings use GPUI's inline representation. Longer strings use the existing uncached path. GPUI owns the shaped line cache. |
| Layout index | Eight bytes per frame with sequential IDs and 32-bit row and offset. Arbitrary IDs and wider locations use a hash table with one entry per frame. |
| Aggregate paint | One pending segment per active row, held on the stack. |
| Search | One offset per matching frame plus one vector per depth for the active query. |

The scale regression checks one aggregate paint at both 10,000 and
10 million adjacent subpixel frames and verifies the last exact hit.
The ID regression checks the sparse fallback and duplicate rejection.
The label regression checks backing-storage reuse, collision replacement
and the byte cutoff. The model traversal allocator gate is in
`bench/tests/allocator.rs`.

A standalone optimized model comparison of 20,000 stacks × 500 frames
reduced peak RSS from 652 MiB to 329 MiB. At 36 rows × 1,200 pixels,
background paints fell from 28,800 to 36. Callgrind recorded 7.29 million
and 7.53 million model traversal instructions respectively, excluding the
GPUI paint calls. The geometry construction still scales with source
size, and merging still visits the intermediate buckets.

For a continuous row, the overview LOD emits its clipped coverage directly
when the widest frame projects to at most 0.25 logical pixels and at least
one pixel of the row is visible. Gapped rows, search highlighting and wider
frames use the normal bucket traversal. Callgrind recorded 1,385 instructions
and 36 paint dispatches for the same 400-pixel viewport at both 1 million and
10 million frames. The upstream 640 × 380 logical pixel overview captures
before and after this shortcut were pixel-identical.

The source-based constructor `FlameGraph::from_source` validates and renders
a labelled preview of up to 4,096 frames, then validates the full layout on
the background executor. Dropping the graph cancels that work at the next
1,024-frame checkpoint. The preview uses a bounded layout and shares the
source with the full layout. The prepared `FlameGraph::new` path remains
available for callers that load or build the layout themselves.

## Mounted evidence

| 10M upstream headless | Quiet baseline at 65e854a | Worker short soak under build contention |
| --- | ---: | ---: |
| Setup and first full draw | 1.47 s | 2.07 s |
| p99 interaction | 343 ms | 1.76–2.63 ms |
| Max interaction | 343 ms | 14.49 ms |
| Resident memory | 729 MiB | 345 MiB |
| BoundsTree insertion, sampled self time | 80.37% | 0.69% |

The short soak uses the prepared full-layout API. A separate 10M headless
test of the deferred constructor reached a labelled preview at 150 ms
with a warm window and 478 ms including a cold window. Full publication
took over two seconds under contention. Forced warm redraws before the
constant-work LOD still allocated 224 times on the UI thread. That
measurement includes a full GPUI window refresh and should not be treated as
an allocation-free repaint result. The 30-minute drift and cold/full-load
budgets remain unverified or outside target.
