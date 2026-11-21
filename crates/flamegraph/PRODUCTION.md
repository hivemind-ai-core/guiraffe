# FlameGraph performance

The `flamegraph` scenario in `bench/examples/soak.rs` mounts the real canvas
with 10 million frames and a stack depth of 500. It dispatches wheel zoom,
drag pan and hover in a 1280 × 800 headless GPUI window. The soak reports first useful preview and full-layout publication separately.
The preview is drawn before draining the deterministic test executor.

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
| Layout index | Four bytes per sequential ID, plus a sparse entry for locations beyond the packed range. Arbitrary IDs use one hash entry per frame. |
| Row spans | Blocks contain up to 1,024 spans. An exact affine block stores one span, index and coordinate strides, and a length. A block that fails exact reconstruction stores up to 1,024 explicit spans, each 24 bytes. |
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
| Steady resident memory | 729 MiB | 345 MiB |
| BoundsTree insertion, sampled self time | 80.37% | 0.69% |

The short soak uses the prepared full-layout API. A separate 10M headless
test of the deferred constructor reached a labelled preview at 150 ms
with a warm window and 478 ms including a cold window. Full publication
took over two seconds under contention. A separate 256-frame run peaked at 345 MiB. Five forced warm redraws each
made 224 allocation calls before and after the changes. Requested bytes
per redraw dropped from 6,710,465 to 86,633. The measurement includes a full
GPUI window refresh. The 30-minute drift and cold/full-load budgets remain
unverified or outside target.


## Follow-up at 10 million frames

The showcase and the mounted soak use `FlameGraph::from_source` so a
labelled prefix is available while the full build runs. Callers with an
already validated index can continue passing it to `FlameGraph::new`.
The index does not duplicate source labels or stack records.

| Upstream headless, loaded host | d12674a path | Deferred and compact layout |
| --- | ---: | ---: |
| First frame in the soak | 1,748 ms | 12–41 ms, preview pending |
| Full layout and draw | 1,748 ms | 1,277–1,367 ms |
| Second short-soak interval p99 / max | 1.76–2.63 / 14.49 ms in earlier runs | 3.81 / 20.82 ms |
| Peak RSS | 345 MiB | 59.2 MiB |

The synthetic source stores one `usize` and 46 bytes of static label and
package text. On a 64-bit build the source payload is 54 bytes before
the shared allocation header, making the peak budget approximately
64 MiB. The compact fixture uses sequential IDs and exact linear spans
in each row. Irregular profiles can store explicit spans and sparse
locations, so the measured peak applies to this workload.

Before the change, the eager build was 76.60% inclusive in the 256-frame
samply trace, with span vector reallocation at 11.76% inclusive.
The follow-up FlameLayout-filtered trace showed build at 36.60% self,
row insertion at 32.36% and location insertion at 12.20%, with no
span vector growth among the leading costs. The regression tests cover
mixed explicit and linear blocks, packed-location overflow, and equal
overview dispatch at 10k, 1M and 10M spans. Mounted allocation-free repaint
and 30-minute drift remain open.

The five forced whole-window redraws each retained the prior count of
224 allocations and 86,633 requested bytes.

A real headless window drew the pending preview in 123 ms with the
window renderer warmed. Starting a cold renderer and window in the
same timed interval took 637 ms. A 1,024-frame thread-CPU probe with
64 frames excluded for warm-up measured 1.70 ms p99 and 2.15 ms max
on the UI thread. The loaded wall p99 in that probe was 13.09 ms.
