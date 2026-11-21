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
| TreeList warmed expansion memcpy, sampled self | 85.48% | 5.56% |
| PropertyGrid warmed expansion memcpy, sampled self | 83.92% | 2.91% |

Both synthetic models hold one million `u64` IDs (7.63 MiB) with static
labels. PropertyGrid also holds four values, four defaults and three
choices, with 39 bytes of string contents and fixed handles. Their
common RSS budget rounds to 73.54 MiB without counting those fixed
records. The observed peaks are process maxima in 256-frame runs.

In the follow-up frame-loop samples, which also include scrolling,
hovering and selection, memcpy was 7.49% of TreeList and 3.73% of
PropertyGrid self time. Flexbox layout was 8.65% and 12.50%.
On the loaded host, TreeList p99 was 6.75–9.56 ms and PropertyGrid
p99 was 14.68–17.39 ms in the two short-soak intervals. The largest
recorded frames were 31.49 ms and 23.21 ms, respectively.
TreeList and PropertyGrid share the same model edit costs. The mounted
UI still rebuilds the viewport's Kit controls for dirty frames. Cold
all-expanded tree mount, quiet-window latency and allocation-free
mounted repaint remain open.

Five forced whole-window redraws requested 879 allocations and 357,987
bytes each for TreeList (previously 894 and 357,995). The legacy 110k
PropertyGrid probe requested 1,279 allocations and 576,768 bytes each
(previously 1,294 and 576,768).

A 1,024-frame thread-CPU probe with 64 frames excluded for warm-up
measured TreeList at 2.07 ms p99 / 2.93 ms max and PropertyGrid at
4.19 ms p99 / 5.00 ms max. The corresponding loaded wall p99 was
29.49 ms and 31.72 ms. Quiet-window wall timing remains the acceptance
measurement.

## Inspector viewport options

| Storage | Bound |
| --- | --- |
| Variable row heights | One ID/height pair per configured override and one prefix entry per visible override. An active multiline editor can add one temporary entry. Window lookup searches the sparse prefix entries, then visits only the viewport. |
| Sticky header and indentation guides | One sticky ancestor and up to 45 guides per viewport row. No label or layout cache. |
| Search projection | At most one visible rank per matching row or ancestor on a match path. One cancellable task walks every loaded node depth-first, 512 nodes per step, holding one frame per tree level, the matching IDs and the collapsed branches on match paths. A new query opens those branches (deepest first, so each revealed subtree is spliced once) when the walk completes; a rescan after a hierarchy mutation keeps the user's collapse and lists the collapsed branch as the path. A new query or mutation drops the running task. Labels are matched case-insensitively without allocation. |
| Editors | One active input editor with at most four component fields, one colour picker, and one slider. Multiline editors grow to at most eight lines. |
| Split | One ratio and measured width per view. Each keyed instance uses one application-managed layout-store entry. |

The sparse geometry tests cover identical viewport output at 10k and 10M
rows and zero allocations for warm viewport traversal at both scales.
Auto-fit inspects the visible names. Revealing a match inside an extremely wide collapsed branch splices that branch synchronously, like a manual expand. Expand All stages loaded branches
and leaves unloaded branches closed. Inserting or dropping one
extremely wide branch still uses the underlying synchronous model splice.

An early upstream measurement used the default inspector on the loaded
development host. It compared the release baseline binary with the
lightweight-row implementation before the final editor changes.

| Probe | Baseline | Lightweight rows |
| --- | ---: | ---: |
| Samply samples over 1,024 steps, 100,001 rows | 2,184 | 1,471 |
| Largest sampled self path, unsymbolized | 17.63% | 15.57% |
| Flexbox layout, sampled self | 4.30% | 5.91% |
| Callgrind instructions, setup and 64 frames at 10,001 rows | 405,094,957 | 303,714,634 |
| Callgrind memcpy instructions | 90,887,281 | 58,064,349 |
| Callgrind main flexbox routine instructions | 26,819,333 | 17,600,400 |
| Callgrind calls to malloc, realloc and calloc | 149,183 | 113,174 |
| Short-soak mount, 100,001 rows | 96.3 ms | 14.8 ms |
| Short-soak p99, two intervals | 8.25–9.40 ms | 6.66–7.71 ms |
| Largest short-soak frame | 16.42 ms | 15.50 ms |
| Largest reported steady RSS | 22.9 MiB | 25.2 MiB |
| Peak RSS at 1,000,001 rows, 256 scripted frames | 61.5 MiB | 61.8 MiB |
| Loaded-host mount at 1,000,001 rows | 296 ms | 364 ms |
| Warm sparse viewport geometry allocations | — | 0 |

These runs did not activate the inspector toolbar or custom row styles.
The Callgrind allocator calls cover setup and all 64 scripted frames.
The geometry allocation check covers viewport traversal. Mounted
unchanged-repaint allocations and release acceptance at one million
rows still require separate end-to-end measurements.
