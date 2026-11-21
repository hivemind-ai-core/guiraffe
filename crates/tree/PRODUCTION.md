# TreeList and PropertyGrid performance

The `tree` scenario in `bench/examples/soak.rs` mounts roughly one million
visible rows with 100,000 expanded branches. The `property-grid` scenario
exercises about 110,000 rows with editors. Both drive scrolling, selection,
hover and expansion in headless GPUI. The TreeList baseline at `65e854a`
spent 92.45% of sampled self time in ID-map insertion.

The shared model preserves its contiguous public row slice. Its ID map now
points to block-relative slots. Expansion and collapse rebuild slots in
the touched blocks and shift the starts of trailing blocks. Rows and
sibling positions still move within their vectors, so expansion close to
the start of the tree remains dependent on the length of the suffix.

Each block indexes up to 512 rows. Each live row retains one map entry, and
the two block tables maintain the start position and reusable slot for each
block. The tables and free list reuse capacity up to their high-water mark.
The view already virtualizes row elements to the viewport. Hidden expansion
and requests to keep an existing visible expansion state skip notification.

The scaling regression checks that opening the same branch writes 513 ID
slots at both 2,048 and one million rows and preserves the trailing IDs.
The notification test checks hidden, repeated and visible expansion.
Callgrind measured 143.56 million to 4.64 million instructions for the
same local expansion near the start of a synthetic million-row model.
The existing warm scroll allocator gate covers model traversal.

## Mounted evidence

| Upstream headless | Quiet baseline at 65e854a | Worker short soak under build contention |
| --- | ---: | ---: |
| TreeList setup and first draw | 81 ms | 525 ms |
| TreeList p99 / max | 64.0 / 82.8 ms | 17.88–28.16 / 77.76 ms |
| TreeList RSS | 124 MiB | 125 MiB |
| PropertyGrid setup and first draw | 11 ms | 60 ms |
| PropertyGrid p99 / max | 6.1 / 9.3 ms | 15.86–24.27 / 39.67 ms |
| PropertyGrid RSS | 29 MiB | 30 MiB |

Separate 256-frame runs peaked at 140 MiB for TreeList and 29 MiB for
PropertyGrid. The TreeList reprofile attributed 11.86% of sampled self time to replacing
affected block index entries. The warm forced window redraw still made
894 UI-thread allocation calls (357,995 bytes) for TreeList and 1,294
(576,768 bytes) for PropertyGrid under the mounted fixtures. The results leave the mounted repaint
allocation, TreeList suffix-copy scaling, memory and latency budgets open.
The short PropertyGrid workload has approximately 110,000 rows and leaves
the million-row PropertyGrid check open.
