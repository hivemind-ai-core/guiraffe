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
position and UV data in exact-sized arrays, plus mesh and map-entry metadata. Each
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


At 0.05× zoom and below, graphs with at least 4,096 nodes use the coarse
overview. Between 0.05× and 0.10×, the overview activates when a viewport
query finds more than 1,024 nodes. The BVH stops counting at the cutoff
before the renderer collects visible IDs or tessellates edges. Sparse
views, active gestures and accessibility rendering use detailed geometry.


For mounted graphs with at least 4,096 nodes or edges, Flow validates the document
during setup and computes routes as paint needs them. The first frame queries
nearby nodes and edges. The first zoomed-out preview uses at most 4,096
occupied cells. A cancellable background job builds immutable node and edge
BVHs. Once indexed, incident-edge lookups can query the edge BVH at the
affected node and verify the endpoint IDs. The edge BVH uses endpoint and control
bounds, so indexed queries include viewport-crossing edges whose endpoints
lie outside the screen. The renderer tests exact route bounds before
submitting triangles.

The deferred route cache uses edge ID and half-octave zoom buckets. It keeps
up to 1,024 entries and 1 MiB of sampled points and compact vertex buffers.
Eviction uses the last frame that accessed each entry. A document change
clears cached routes and cancels stale background work. The immutable
edge BVH stores bounds and IDs without the eager per-edge route table or
the mutable index's ID-to-leaf map.


At each zoom bucket, Flow simplifies deferred stroke geometry within an
eighth of a screen pixel. Picking, labels and arrowheads use exact route
points. On gpui-fast, large viewports with the default node renderer build
the visible cards inline to reduce retained view state. Custom renderers and
accessibility use the normal node entity path.

The viewport uses a sized GPUI cached view boundary. Unchanged incremental
window draws reuse the previous paint ranges without invoking Flow render
callbacks. Camera, gesture and document notifications invalidate the viewport.
GPUI's scene replay can allocate transient buffers. Forced window refreshes
rebuild the viewport, and accessibility rendering takes the normal entity path.

## Warm camera and hover replay

Set `SOAK_FLOW_WARM_REPLAY=1` for the headless Flow soak to repeat 32
camera positions and alternate pointer moves between a node and a gap.
The runner performs two warm-up passes before sampling. Each iteration
includes a camera draw and a pointer move with a second draw request.

At 50,000 nodes and 100,000 edges, index publication took 302–404 ms
from fixture start and 44–69 ms after the first draw across loaded
release runs. This measures the interval through publication in the
headless test dispatcher. The warm replay's two short windows each
recorded 97,708 KiB RSS and 94,918 KiB PSS. On this shared host, setup
through first draw took 259 ms and the second window's p99 was 37.5 ms
per two-draw iteration. Quiet single-event latency remains open.

| Self samples | 160 camera iterations | 1,200 warm camera and hover iterations |
| --- | ---: | ---: |
| GPUI BoundsTree insert | 4.72% | 5.69% |
| GPUI path clone and scale | 4.42% | 4.02% |
| Flow frame paint | 1.87% | 2.45% |
| Flow mesh paint | 1.75% | 1.50% |
| Flow deferred mesh lookup | 1.09% | 1.42% |

These are two different workloads on the same production implementation.
In the warm profile, deferred mesh lookup accounted for about 0.70 ms
of sampled CPU per iteration including callees. Flow frame paint
accounted for about 0.35 ms of self time. No individual Flow-owned
hot path was clearly above 1 ms after excluding GPUI work. The
existing cache limits and allocation measurements still apply.

The opt-in test `ten_million_nodes_keep_detailed_viewport_work_constant`
materializes both 10,000 and 10,000,000 nodes, with identical edges
near the view and a disabled minimap. It runs the mounted renderer
at three matching pan/zoom settings after index publication. Visible
node IDs, node and edge counts, edge vertex counts, and stroke and
marker batch counts matched at all three settings. Index traversal
visits were 20/20/22 at 10k and 40/40/44 at 10M. Exact equality of
total query work remains open because BVH navigation grows with
tree depth. Run the test explicitly with
`cargo test -p guiraffe-flow --lib ten_million_nodes_keep_detailed_viewport_work_constant -- --ignored --nocapture`.
The upstream debug run took 178 seconds, so the regular workspace
gate skips this fixture.
