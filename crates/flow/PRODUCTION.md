# Flow production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | 50k nodes / 100k edges exercised by `flow-build-50k`, `flow-pan-hit-50k` and the headless soak. The zoomed-out view uses coarse regions from the node BVH. [Coverage and primitive-count check](../flow-core/tests/review_2.rs). |
| §2 Latency | ⚠️ Open | Core build median 213.26 ms. Three viewport queries plus hits took 18.67 µs median. The tested headless camera path had significant tail latency. The [headless replay](../../bench/examples/soak.rs) exercises initial overview and detailed camera motion. Scene construction on the UI thread and native input-to-frame evidence remain open. |
| §3 Memory | ⚠️ Open | Zero warm allocations in the scale core pan/hit test. A default node payload fits 256 B including handles and type strings. The test excludes scene/index/map overhead and arbitrary host data. Dense minimaps now submit at most 8,100 vertices per layer. Native rendering allocates and caches meshes as nodes/edges enter view. |
| §4 Correctness and robustness | ⚠️ Open | Randomized editor operations match a reference undo/redo history, and BVH updates match a linear scan. [Property tests](../flow-core/tests/review_2.rs). Flow retains existing native selection, gesture and reduced-motion coverage. Parser fuzz, the complete input/viewport matrix and background-cancellation verification remain open. |
| §5 Long-session and stress | ⚠️ Open | The shared headless soak runner supports camera pan/zoom, 10-second RSS/PSS samples and a short CI mode. The full 30-minute run, native inspect/drive replay and editing workload remain open. |
| §6 Accessibility and UX | ⚠️ Open | Keyboard canvas commands exist. Audit handle navigation and announcement. Capture scale viewport positions on both themes and backends. |
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

The default minimap short soak before the density fix took 48.8 s from
fixture creation through first frame. Its second sample had 101.6 ms p99
and 134.9 ms maximum at 2.39 GiB RSS. Shared host contention makes the
absolute size of the tail unreliable. The same workload still needs a
controlled native run.

After the minimap density fix at `7312e37`, a loaded short run mounted in
29.9 s. Its second window had 51.7 ms p99, 69.1 ms maximum and
2,479,884 KiB RSS, 216 KiB above the warmed sample. The structural
vertex gate reproduces the minimap regression independently of timing.

The zoom regression measures arrow proportions and handle contact at 0.25×,
0.5×, 1×, 2× and 4×. The showcase includes inspected 0.5× and 2× Flow
snapshots on both GPUI backends. Arrowheads, handles, open-marker strokes,
animated markers and connection previews share the camera scale.


## Retained paint geometry

The edge mesh cache holds at most 8,192 route entries and 16 MiB of
allocated vertex capacity, plus the path and map-entry metadata. Each
entry shares its immutable route with the scene. A route replacement
invalidates that entry. Cached triangles use world coordinates and are
shared across pan and zoom. If admission hits either limit, the next camera
or canvas-size change drops cached entries outside the new visible set.
Oversize meshes and meshes that cannot be admitted draw transiently.

Grid and minimap geometry each retain one representation. The grid depends
on canvas size, spacing and background style. The dense minimap is
bounded to 8,100 vertices per layer. The edge submission batches reserve
capacity based on the previous visible frame and GPUI owns their submitted
vertices. The cache regression simulates 216,000 camera frames and checks
both retained mesh limits on every frame.
