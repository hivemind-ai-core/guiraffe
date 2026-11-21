# TreeList and PropertyGrid performance

The `tree` scenario in `bench/examples/soak.rs` mounts roughly one million
visible rows with 100,000 expanded branches. The `property-grid` scenario
exercises 1,000,001 rows with editors and 100,000 expanded groups. Both drive scrolling, selection,
hover and expansion in headless GPUI. The TreeList baseline at `65e854a`
spent 92.45% of sampled self time in ID-map insertion.

The shared model stores visible rows in an implicit treap of blocks.
Each block contains up to 512 rows. Subtree counts support expected
logarithmic row lookup and rank updates. Expansion rewrites the changed
rows and boundary blocks without moving the trailing rows. Each entry
uses 32 bytes with a compact sibling position. Exceptional wide sibling
positions use one sparse entry per such row.

| Storage | Limit |
| --- | --- |
| Row index | A 1 KiB dense page per 256 IDs once a full row block contains at least 32 IDs in that page. Otherwise one sparse entry per indexed ID. Wide block positions also use the sparse map. Pages persist until source replacement, and the number of pages is bounded by the source ID distribution. |
| Blocks | Reusable arena slots up to the block high-water mark. Adjacent boundary blocks are merged when they fit. |
| Contiguous snapshot | One materialized public `rows()` slice, invalidated on a structural change. The view and warmed scroll probe use individual rows and never populate it. |

Cross-block replacement tests preserve ranks, full-size IDs and sibling
positions. The scaling test verifies equal ID rewrites for the same edit
at 10k and 1M rows. The production Callgrind bench exposes that warmed
expand/collapse pair at both sizes. A standalone scoped Callgrind probe
recorded 667,797 → 194,437 instructions at 10k and 9,121,495 → 200,984
at 1M. The measured 3.4% difference includes rank traversal and index
access. The tail expansion allocator probe fell from 172 MB to 36.7 MB
of requested bytes at one million children. The warmed scroll allocator
gate covers the individual-row access path.

## Mounted evidence

| Upstream headless | Quiet baseline at 65e854a | Worker short soak under build contention |
| --- | ---: | ---: |
| TreeList setup and first draw | 81 ms | 227 ms |
| TreeList p99 / max | 64.0 / 82.8 ms | 9.86–14.65 / 26.23 ms |
| TreeList steady RSS | 124 MiB | 110 MiB |
| PropertyGrid setup and first draw, about 110k rows | 11 ms | 44 ms |
| PropertyGrid p99 / max, about 110k rows | 6.1 / 9.3 ms | 8.93–9.29 / 15.50 ms |
| PropertyGrid setup and first draw, 1M rows | — | 258 ms |
| PropertyGrid p99 / max, 1M rows | — | 10.70–11.03 / 17.62 ms |
| PropertyGrid steady RSS, 1M rows | — | 110 MiB |

Separate full-scale 256-frame runs peaked at 124 MiB for both controls.
The final profiles attributed 12.31% of TreeList and 9.31% of PropertyGrid
sampled self time to replacing affected block index entries. The warm forced window redraw made 894
UI-thread allocation calls (357,995 bytes) for TreeList and 1,294 (576,768
bytes) for the legacy 110k PropertyGrid workload both before and after
the changes. Mounted repaint allocation, TreeList suffix-copy scaling,
memory and quiet-window latency were open at that checkpoint.


## Follow-up at one million rows

| Upstream headless, loaded host | d12674a | Block storage and dense pages |
| --- | ---: | ---: |
| TreeList peak RSS | 124 MiB | 63.7 MiB |
| PropertyGrid peak RSS | 124 MiB | 64.5 MiB |
| TreeList warmed expansion memcpy, sampled self | 85.48% | See warmed frame mix below |
| PropertyGrid warmed expansion memcpy, sampled self | 83.92% | See warmed frame mix below |

Both synthetic models hold one million `u64` IDs (7.63 MiB) with static
labels. PropertyGrid also holds four values, four defaults and three
choices, with 39 bytes of string contents and fixed handles. Their
common RSS budget rounds to 73.54 MiB without counting those fixed
records. The observed peaks are process maxima in 256-frame runs.

In follow-up samples filtered to the frame loop, memcpy fell to 7.49%
of TreeList and 3.73% of PropertyGrid self samples, now including GPUI
copies. Flexbox layout accounted for 8.65% and 12.50% of self samples.
TreeList and PropertyGrid share the same model edit costs. The mounted
UI still rebuilds the viewport's Kit controls for dirty frames. Cold
all-expanded tree mount, quiet-window latency and allocation-free
mounted repaint remain open.
