# Timeline production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | Generated 100k-key, 100k-clip and 100k-bar workloads. The dope-sheet native test executor also mounts a 100k-key document. Other family views still need scale rendering coverage. |
| §2 Latency | ⚠️ Open | Three pan/zoom/hit checks on 100k clips took 1.10 µs median. Three small box selections from 100k keys took 0.014 ms after range indexing and two 100k-bar strokes took 1.13 ms. The [100k-key replay](../../bench/examples/soak.rs) exercises the dense undecorated key culling and diamond batching paths. The mount and interaction budgets remain open. |
| §3 Memory | ⚠️ Open | Warm clip navigation/hit and standalone bar drawing allocate zero. Small box selection has an allocator gate. 100k standalone bars fit one f64 per value plus a shared vector header. Aggregate overhead, retained render state, warm GPUI allocation and leak limits remain open. |
| §4 Correctness and robustness | ⚠️ Open | Randomized clip insertion matches a reference scan. [Dope model property tests](src/dope_model.rs) check box selection and atomic retiming against set-based references. A stale-row regression covers host channel removal. A red/green test prevents non-finite clip ends on insertion. Native tests exercise existing gestures. Full model-oracle coverage, malformed/large-input matrices, and off-thread job validation remain open. |
| §5 Long-session and stress | ⚠️ Open | The shared headless soak runner drives dope-sheet scroll and drag with 100k keys, including Arabic, Japanese and emoji labels. The 30-minute run and native inspect/drive and family-wide replay remain open. |
| §6 Accessibility and UX | ⚠️ Open | [Keyboard](tests/keyboard.rs) and [snapshot](tests/snapshots.rs) coverage is at demo sizes. Verify announcements, focus under updates and scale snapshots in both themes/backends. |
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

At 36.98/24.61/17.36 load before the run, the short 100k-key dope-sheet
replay took 802 ms from fixture creation through first frame. Its second
10-second window reported a 1.12 s p99 over 20 scripted iterations. RSS
increased by 24 KiB from 100,952 KiB between the two samples. A scripted drag
iteration dispatches several events and can paint more than once.

Box-select median fell from 0.309 ms to 0.014 ms after `65d401a`.
The second run had 33.92/33.73/24.56 load and modified report Markdown only.
The runs were sequential on a shared host and were not a controlled A/B.

The showcase opts Gantt and Tracks into focus on item clicks and displays
keyboard instructions. Its native Gantt regression clicks a bar, verifies
a one-day move at the default knob setting, clicks the live week option
and verifies a seven-day move on the same view. The keyboard tests also
check that views without an explicit opt-in preserve pointer focus.

## Viewport rendering bounds

- The bar editor retains up to 16,384 pairs of `f64` extrema (256 KiB of
  entries) and one dirty index range. The first dense render and a column
  count change scan the model. A stroke recomputes only columns intersecting
  its changed range. One GPUI paint layer groups the dense view's quads,
  with at most one quad per column.
- Tracks overview uses the sorted, disjoint clip intervals already in the model
  and retains no overview geometry. Each visible lane emits at most one mark
  per logical pixel column, capped at 16,384 columns, with adjacent runs of the
  same color combined in one paint layer. The selected clip keeps its individual element.
  When assistive technology is active, individual visible clip elements expose
  their names, roles and selection state.
- The dope sheet retains one cached header view per visible row (including
  partially visible rows). The header cache reuses the same controls across
  pan, zoom and key edits. The key canvas uses four-pixel spacing for dense
  undecorated rows without selection. Decorated, summary and selected rows
  retain their individual marks. Plain channels store only the sorted key
  times, with no parallel decoration array. The key canvas traverses the
  visible rows at paint time using the current scroll offset and keeps no
  key-mark vector. GPUI receives owned path meshes. The pinned backends
  still invalidate the editor view when scrolling shifts the row headers.
- The curve editor indexes visible markers by time. When more than 512
  markers from one series intersect the viewport, consecutive unselected
  markers share a mesh. Selected markers and tangent handles remain
  individually painted. The editor retains no marker geometry between
  frames. The viewport test varies the number of offscreen keys and keeps
  the viewport fixed.

### Gantt rendering storage

The view retains one schedule keyed by task ID, one ID-to-row index, and one
shared display label per task. Forward and reverse dependency indexes each
store N+1 row offsets and E row numbers, where N is task count and E is
dependency count. The schedule plan also holds N row numbers in topological
order. Drag and resize replace the schedule without rebuilding labels,
dependency indexes or topological order. No scroll history or zoom buckets
are retained. Dependency paint collects up to 32 projected edges on the
stack before drawing a stroke path and an arrowhead mesh.

A frame visits the intersecting rows. Task bars outside the horizontal
interval are culled and narrow bars retain a two-pixel marker. Dependency
arrows require a visible endpoint. Adjacencies with at most 64 peers are
scanned directly. For larger adjacencies, row-ordered indexes locate the
visible range by binary search. Paint projects each distinct peer in that
range and one representative (the furthest row) for each offscreen direction.
This bounds projections by the viewport and leaves logarithmic index work
in the degree of a dense row. The LOD omits individual offscreen routes
when a dense row's offscreen edges overlap.

The view retains one header subtree containing the current viewport's
task names and lock controls. It reuses the subtree across horizontal
navigation, hover and selection, and replaces it when vertical position,
visible rows, tasks, schedule or lock state changes. Colors resolve from
the live GPUI theme. The accessibility label cache retains at most 128
shared strings in direct-mapped row slots. Schedule timing changes refresh
a slot lazily and task replacement clears all slots. GPUI retains its own
text layout inside the header. Visible accessibility items remain GPUI
elements. Plot commands and arrow tessellation still allocate.


The standalone scheduler check used the 100k-task soak fixture, optimized
Rust binaries and the pre-change revision `65e854a`. Both executables reported
the same sum of earliest finish times. Callgrind and Memcheck counts cover the
entire process, including fixture creation and output. Timing is the median
of five alternating runs on a shared machine. RSS is the child process peak
and excludes GPUI.

| Standalone scheduler measurement | Before | After |
| --- | ---: | ---: |
| Callgrind instructions | 259,177,812 | 190,809,476 |
| Heap allocation calls | 280,050 | 190,023 |
| Heap bytes allocated | 29,771,368 | 24,005,868 |
| Median schedule build | 183.6 ms | 99.0 ms |
| Median peak RSS | 33.3 MiB | 28.9 MiB |

A second standalone probe built the same schedule, changed the first task's
duration by four seconds, and solved again. Both versions reported the
same sum of earliest finish times after the edit. Total process instructions
fell from 395.9M to 224.7M. The second solve took a median 148.2 ms before
and 28.6 ms with the reused plan in five alternating runs on the shared
machine.


The mounted upstream probe at `ce4b8b1` ran the unchanged Gantt fixture and
interaction sequence extracted from `bench/examples/soak.rs`. Timeline code
and the probe were compiled at opt-level 3 and linked against the upstream
dev-build GPUI dependencies. The after measurements were taken on the
shared machine. The before measurements are the quiet release baseline
recorded in `plans/PERF-0.2.md`.

| 100k-task mounted Gantt | Before | After, indicative |
| --- | ---: | ---: |
| Setup and first frame | 26.7 s | 170.3 ms |
| Warm median frame | 6.7 s | 4.3 ms |
| Warm p99 frame | Unavailable | 13.6 ms |
| Warm maximum frame | Unavailable | 16.2 ms |
| Sampled RSS | 6.66 GiB | 49.4 MiB |

Five separate 500-frame replays of the same fixture produced busy-machine
p99s of 19.3 ms for pan, 15.0 ms for vertical scrolling, 15.2 ms for zoom,
13.1 ms for hover, and 24.6 ms for selection. Quiet release latency and the
30-minute drift limit remain open.

The self-time share for the profile's unsymbolized layout-copy hotspot
was 18.58% in the baseline and 15.21% in the 400-frame optimized mounted
probe. Flex layout was 3.83% and 2.66%, respectively. Bounds-tree insertion
was 5.96% in the latter capture. These are shares from different captures
and build profiles. Standalone scheduler instruction counts above provide
the like-for-like CPU comparison.

The same local mounted probe on gpui-fast reported 330.9 ms mount,
17.4 ms warmed p99 and 56.0 MiB sampled RSS on the shared machine.

An upstream probe used the bench counting allocator around one
`Window::refresh` and frame render after four warm-up frames. It measured
3,367 calls requesting 1,981,143 bytes at each of 10k, 100k and 10M tasks.
This includes the entire GPUI window refresh. The measured repaint traffic
stayed flat as the source grew, and the zero-allocation criterion remains
open.

Regression tests cover viewport row counts through 10M rows, pixel-accurate
coordinates near row 9M, horizontal interval culling, two-pixel milestones,
and dependency endpoint culling with each shared edge emitted once
([Gantt view tests](src/gantt_view.rs)). The scheduler tests check ordered
adjacency and reuse of a plan after timing edits ([scheduler tests](src/gantt.rs)).
A mounted interaction test scrolls a 10k-task chart to its last row and
selects the bar in the scrolled coordinate system ([interaction tests](tests/interactions.rs)).

The eight refreshed timeline and showcase goldens were inspected in both
themes on both GPUI backends. Pixel changes were confined to Gantt rendering
around its borders and dependency paths. Lock badges, critical-path colors
and coincident arrowheads remain visible.

`scripts/gate.sh` reported `GATE-PASS` at `c1322f5` with four build jobs
and separate targets for each backend. Both full workspace test stages,
the per-crate doctest commands, workspace Clippy with warnings denied,
and formatting checks passed.


### Gantt follow-up at `34c5531`

The baseline for this comparison is `9f6e90a`. Both binaries used opt-level 3
for the probe and timeline code and the same upstream dev GPUI dependencies.
Callgrind counts include the 10k-task fixture, mount and 64 scripted frames.
Samply sampled the 100k-task mounted replay for 1,000 iterations. Percentages
are shares of their respective captures. Timing was recorded on a shared
host and needs quiet release measurement.

| Measurement | Before | After |
| --- | ---: | ---: |
| Layout copy, Samply self share | 13.96% | 11.63% |
| Flex layout, Samply self share | 2.91% | 1.99% |
| Bounds-tree insertion, Samply self share | 7.42% | 9.65% |
| Sparse replay, Callgrind instructions | 274,323,270 | 175,827,681 |
| Forced warm whole-window refresh, allocation calls | 3,367 | 3,251 |
| Forced warm whole-window refresh, requested bytes | 1,981,143 | 1,987,211 |
| 10k-task fan-out, setup and first frame | 52.0 s | 68.1 ms |
| 100k-task sparse, setup and first frame | 748.4 ms | 455.6 ms |
| 100k-task sparse, warmed p99 | 33.5 ms | 29.2 ms |
| 100k-task sparse, sampled RSS | 47.2 MiB | 49.6 MiB |
| 100k-task sparse, 64-frame process peak RSS | 47.3 MiB | 46.3 MiB |

The 100k-task fan-out mounted in 165.1 ms, with a warmed p99 of 31.6 ms
and 50.3 MiB sampled RSS. The 100k-task chain mounted in 469.9 ms, with
a warmed p99 of 29.1 ms and 48.3 MiB sampled RSS. The benchmark harness
exposes these cases as `gantt-fan-out` and `gantt-chain`, with the same
`SOAK_SCALE` and `SOAK_FRAMES` controls as `gantt`.

The 10M-task sparse run also recorded 3,251 allocations and 1,987,211
requested bytes for the forced repaint. Setup at that scale took 52.1 s
on the busy host. The viewport test checks the same six projected peers
at degrees of 10k and 10M. The mounted header test checks reuse after pan
and invalidation after a lock change. A label test checks cache reuse,
timing invalidation, eviction and cache clearing. The zero-allocation
budget remains open. GPUI forced refresh bypasses retained view caching,
and GPUI's path submission consumes and rescales owned path vertices.
A trial bounded tessellated-path cache increased the sparse replay from
175.8M to 176.9M instructions and did not reduce forced-repaint allocations,
so it was discarded. Quiet latency for every interaction and memory over a 30-minute run
still need verification.

Five separate 500-iteration, 100k-task interaction probes measured p99s of
6.4 ms for pan, 18.9 ms for vertical scroll, 13.6 ms for zoom, 11.6 ms for
hover and 31.0 ms for selection. These shared-host replays used the same
interaction probes as the earlier run, with independently varying load.

The current code passed the full upstream and gpui-fast workspace test
batches, per-crate doctests, workspace Clippy with warnings denied,
formatting, and `cargo test -p guiraffe-bench`. The initial
`scripts/gate.sh` invocation started before the label test fixture correction
and still reported its original upstream failure. An exact rerun of the
upstream workspace test stage after the correction exited 0. The focused
Gantt showcase and timeline snapshots passed on both backends with no
golden changes.
