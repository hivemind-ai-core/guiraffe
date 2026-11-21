# Charts production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | Generated one line with 10M points and one scatter plot with 1M points across 100 series. CPU line scene compilation is fast once prepared. Dense scatter and cold preparation remain outside their budgets. |
| §2 Latency | ⚠️ Open | 10M line preparation median 1.98 s. Full scene plus three zoomed widths took 0.360 ms. Full 1M-point/100-series scatter scene compilation took 47.45 ms. Large viewport scenes compile in cancellable background jobs. The [deferred source interaction test](tests/interactions.rs) checks completion and supersession. Full useful-frame and interaction budget verification remains open. |
| §3 Memory | ⚠️ Open | Prepared line buffers have a 256 B/point structural gate for sample, LTTB and line-break buffers at 100k. Allocator gates cover existing smaller scenes and full million-point scatter recompute. Chart row strings, other retained storage, native paint and 30-minute memory validation remain open. |
| §4 Correctness and robustness | ⚠️ Open | Randomized stream chunk append/snapshot behavior matches a flat vector, in addition to existing scale, sampling and stack properties. [Dense mark tests](src/scene.rs) cover representative validity and shared scatter draw budgets. [Stream tests](src/data.rs) cover bounded retention, snapshot isolation and stable keys. The complete rendering/interaction edge matrix, cancellation and fuzz verification remain open. |
| §5 Long-session and stress | ⚠️ Open | The shared headless soak runner has line/scatter interaction modes and samples frames and memory. The [chart-stream replay](../../bench/examples/soak.rs) drives a rolling chart at 10k points/s and verifies throughput. Native inspect/drive and full 30-minute results remain open. |
| §6 Accessibility and UX | ⚠️ Open | [Native interactions](tests/interactions.rs) cover chart controls and retention. Verify focus and screen-reader state under data updates, and capture scale snapshots across both themes/backends. |
| §7 Process | ⚠️ Open | Audit, regression checks and exploratory release measurements recorded. Automated end-to-end acceptance, quiet reference baselines and independent review remain open. |

Latency acceptance requires p99 ≤8 ms, maximum ≤16 ms, UI tasks ≤50 ms and cold mount ≤200 ms. Nightly replay must sample frame percentiles and RSS/PSS every 10 seconds and enforce the 2% warmed drift limit. Review the [baseline policy](../../bench/baselines/README.md) before recording reference results.

Scale workloads and checks are registered in [suites.rs](../../bench/src/suites.rs),
[allocator.rs](../../bench/tests/allocator.rs) and the [headless soak runner](../../bench/examples/soak.rs).

Exploratory release runs used rustc 1.98.0 on the AMD Ryzen 7 8745HS with
Radeon 780M graphics and the powersave governor. Ten samples used a minimum
one invocation per batch. Machine load was 17.37/16.14/13.28 before and
16.32/15.95/13.31 afterward. CPU workloads above were measured at `a6e7ede`,
except timeline clip navigation at `5ae9382`. The host was shared and loaded,
so these measurements do not qualify as reference baselines.

The 10M-line headless short replay mounted in 3.25 s including data
preparation. Its second sample had 33.0 ms p99, 41.8 ms maximum and
1,905,300 KiB RSS, 336 KiB higher than the warmed sample. Short mode
exercises the runner and supplies diagnostics without asserting absolute timing.

At 31.66/33.23/24.54 load, the 1M-point, 100-series scatter replay
took 79.8 s from fixture creation through first frame. Each short sampling
window completed one interaction, taking 26.7 s and 33.3 s respectively.
RSS rose from 495,784 KiB to 502,888 KiB. Those two windows are too short
and too sparse for a live memory or percentile acceptance claim.

## Scatter viewport budget

Scatter and bubble runs share device-pixel occupancy per facet. A sparse
viewport draws at most one mark center per pixel. At overview density, all
visible runs split a 2,048-candidate budget, and marks share a grid with cells
at least six device pixels wide and high, capped at 64 × 32 cells. The first
drawable candidate claims each cell. Picking still uses the original points.

Scene compilation uses a 2,048-byte occupancy array, up to 2,048 sparse
pixel keys, and one temporary visible range per scatter run. These buffers
are released when scene compilation finishes. The two cached scenes are
invalidated on size, scale, data or visibility changes.

On the shared host, the renderer-neutral scatter probe recorded the following
at `5f8fd2a` and `172e66c`. The instruction count covers three scene builds
and picks at 100k points. Allocation traffic, peak RSS, and timing are from
300 scene builds and picks at 1M points across 100 series.

| Metric | Before | After |
| --- | ---: | ---: |
| Callgrind instructions | 8,109,614 | 2,419,329 |
| Allocations per build and pick | 277.06 | 117.40 |
| Peak process RSS | 27.6 MiB | 27.9 MiB |
| Busy-host p99 | 20.46 ms | 13.93 ms |
| Samply frame samples across 3,500 iterations | 6,699 | 2,147 |
| Coordinate projection share of self samples | 31.9% | 18.5% |
| Scatter generation share of self samples | 31.8% | 27.9% |
| Visible-range search share of self samples | 19.0% | 15.7% |

The scaling test checks the 2,048 projection budget across 1 and 100 series
and across 10k through 10M source points. Finding visible ranges and exact
picking still have per-series work.

## Warm draw allocation attribution

The mounted allocator check in [allocator.rs](../../bench/tests/allocator.rs)
forces a draw of a warmed, unchanged 10k-point, 100-series scatter chart. It
asserts no additional scene builds or ink paints and bounds full-frame
allocation traffic. GPUI upstream allocated 263 times and requested 498,444
bytes. Valgrind DHAT attributes the draw as follows.

| Allocation site | Calls | Bytes |
| --- | ---: | ---: |
| GPUI `Scene::replay` and `Scene::insert_primitive` vertex clones | 248 | 465,792 |
| GPUI `Scene::finish` stable path sort | 1 | 20,832 |
| GPUI tab stops, element IDs, dispatch and arena housekeeping | 14 | 11,820 |
| Chart code | 0 | 0 |

Each replayed path is cloned once into the paint operation and again into the
draw list. Sharing immutable scaled vertices across those two records and
adjacent frames would remove the vertex copies. The full zero-allocation
criterion also requires reusable backend sort and window scratch storage.

Dense scatter batches now draw rounded quads inside one GPUI paint layer per
batch. The layer preserves mark order and uses one bounds-tree insertion for
the batch. In the same upstream warm draw, this reduced allocation traffic to
74 calls and 79,500 bytes. Remaining allocations are 58 axis-path vertex
copies (11,136 bytes), two stable-sort buffers (56,544 bytes), and 14 window
bookkeeping allocations (11,820 bytes). Chart code still allocates zero
bytes in the captured draw.

The native headless release probe at 1M points and 100 series reached its
first frame in 155 ms, published the full scene in 1,200 ms, and held 42.7 MiB
RSS across the two 10-second samples on a shared host. Its second sample
recorded p50 1.56 ms, p99 27.95 ms and maximum 36.82 ms. A prior run of the
same workload with octagon paths recorded p50 1.68 ms, p99 11.67 ms, and
maximum 21.06 ms under different contention. These runs do not settle the
quiet-host p99 or full-scene publication time.

The warmed mounted Samply trace filtered to `soak::sample` attributes 1.0%
of self samples to bounds-tree insertion with layered quads, compared with
1.6% with octagon paths. Visible-range search accounted for 21.4%, dense
scatter generation 11.5%, and coordinate projection 6.2%. A trial that
restricted the upper-bound search to a short suffix did not improve total
Callgrind instructions at either 100k or 1M points and was dropped.
