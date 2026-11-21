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

### Gantt rendering storage

The view retains one schedule keyed by task ID, one ID-to-row index, and one
shared display label per task. Forward and reverse dependency indexes each
store N+1 row offsets and E row numbers, where N is task count and E is
dependency count. The schedule plan also holds N row numbers in topological
order. Drag and resize replace the schedule without rebuilding labels,
dependency indexes or topological order. No scroll history or zoom buckets
are retained. Dependency paint collects up to 32 projected edges on the
stack before drawing a stroke path and an arrowhead mesh.

A frame visits the intersecting rows plus their incident edges. Task bars
outside the horizontal interval are culled and narrow bars retain a
two-pixel marker. Dependency arrows require a visible endpoint. A graph
with very high degree on a visible row can still require work proportional
to its incident edges. Visible accessibility items and lock controls
remain GPUI elements. Plot command generation and arrow path construction
still allocate, so the warm repaint allocation gate remains open.


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
