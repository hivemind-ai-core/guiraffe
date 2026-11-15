# Flow review #1 validation and alternating benchmarks

Validated source: `6860754de5b06b4c72d4255f4dc972bc3f7fa541`, rebased onto
release `8d0f992f37a22d271ae7d49f505ee3e3beefe681`, **gpui-kit =0.9.1**.
The foreman requested this upgrade after the original 0.7.0 task dispatch.
All 69 tests, package Clippy with `-D warnings`, formatting and diff checks pass.
See [REVIEW-1.md](REVIEW-1.md) for the per-item resolution and exact test limits.
No branch was merged or pushed.

## Alternating old/new measurements

The original flow sources at `4c9969c` were installed in a temporary Orca worktree
based on release `f96e93a`; its core/theme and root manifest match `8d0f992`.
Only the deprecated scroll names were ported to 0.9.1 for the baseline UI, and the
same `edit_stress` example was added to both. Both variants therefore use the
same GPUI version, host, toolchain and benchmark source. The new variant is
`6860754` with the review fixes. The baseline worktree was removed after measurement.

Binaries were built once, copied to distinct paths to avoid Cargo's two `stress`
example filename collision, then run **ten times per variant per benchmark** on
CPU 15 with `taskset`. Each run alternated AB/BA order; no new build was started
by this worker during the measurement. Other workloads continued on the shared
16-CPU host (observed load approximately 35–68 during this work). This reduces
order bias but does not make the host quiet or establish formal performance acceptance.

Entries below are **medians across runs**, with the **minimum–maximum across
runs** in parentheses. For distribution rows these are medians/ranges of the
individual runs' reported percentiles, not pooled percentiles.

| Measurement | Before | After |
|---|---:|---:|
| Scene build | 22.03 ms (14.89–49.41) | 29.28 ms (23.48–80.41) |
| Spatial query + cached routes p50 | 16 us (13–17) | 19.5 us (12–20) |
| Spatial query + cached routes p95 | 22 us (19–23) | 29.5 us (22–33) |
| Spatial query + cached routes p99 | 33 us (27–110) | 49 us (37–1077) |
| One-node edit p50 | 23.542 ms (18.735–55.075) | 0.157 ms (0.134–0.187) |
| One-node edit p95 | 29.481 ms (22.400–93.086) | 0.240 ms (0.184–3.035) |
| One-node edit p99 | 41.365 ms (24.243–234.965) | 0.617 ms (0.233–4.860) |
| Headless frame p50 | 14.406 ms (7.754–35.495) | 11.933 ms (8.026–26.612) |
| Headless frame p95 | 24.355 ms (11.643–51.658) | 26.418 ms (10.227–56.361) |
| Headless frame p99 | 30.842 ms (12.643–60.200) | 29.563 ms (11.910–67.668) |

The median edit result improved by about 150x. Persistent maps and path-copyable
indexes retain a **33% median scene-build cost and 22% median query cost** in this
comparison; these are explicit tradeoffs, not improvements. Transient std indexes
for bulk handle lookup/validation, bulk location collection and packed BVH paths
removed avoidable overhead. The initial isolated 12→62 ms build observations were
not sustained by the interleaved comparison. Frame medians overlap broadly and
p95 worsened in this sample; no stable frame speedup or GPU refresh rate is claimed.
The 8 ms p99 design budget remains unproven.

Geometry and UI graph/view parameters are the same as the historical protocol
below. The new edit example uses that 5k/10k graph, 20 warmups and 100 measured
moves of one node with the default history limit of 100. It also prints a full
reconstruction reference within each run; the table compares actual Editor edits
between revisions. The reference is not substituted for the old Editor result.

[review-1-benchmark.json](review-1-benchmark.json) retains all 60 process outputs,
per-run metrics, revision labels, ordering and summary ranges. To repeat after
building/copying old and new binaries to separate paths:

```sh
python3 crates/flow/benchmark_ab.py \
  --old-core /tmp/flow-ab/core-old --new-core /tmp/flow-ab/core-new \
  --old-edit /tmp/flow-ab/edit-old --new-edit /tmp/flow-ab/edit-new \
  --old-ui /tmp/flow-ab/ui-old --new-ui /tmp/flow-ab/ui-new \
  --cpu 15 --runs 10 --old-revision <baseline> --new-revision <candidate> \
  --output /tmp/flow-ab-result.json
```

Core binaries use `cargo build --release -p guiraffe-flow-core --examples`.
UI binaries use `CARGO_PROFILE_DEV_OPT_LEVEL=2 cargo build -p guiraffe-flow --example stress`.
The two variants must use the same toolchain/dependency version and hardware.
These are CPU/headless measurements, with no GPU presentation or native display proof.

## Earlier single-run observations

Before rebasing, same-version 0.7.0 observations were build **12.144→61.984 ms**,
query p50/p95/p99 **10/14/18→20/31/52 us**, and UI p50/p95/p99
**8.405/10.474/11.127→13.143/31.619/41.786 ms**. These unpaired runs overlapped other
builds and motivated the profiling and alternating protocol above. They should
not be used as controlled comparative evidence.

---

# Historical hand-in before review #1 (gpui-kit 0.7.0)

Source revision: `8bb0bbdcbc145b43c9c58f26b6796cfcdd913dac` on `feat-flow`,
rebased onto `release/0.1` at `830a01a`. This report is a documentation-only
follow-up; source and test results below belong to that revision.
Recorded 2026-11-15 UTC on Linux x86_64, 16 available CPUs,
`rustc 1.98.0 (88d9e12ae 2026-08-18)`, gpui-kit exactly 0.7.0.

## Commits for foreman review

- `2ff0216` — validated graph model/editor, serialization, routes, BVHs and tests.
- `04240ad` — incident-edge index for drag previews.
- `a551f95` — GPUI canvas, interactions, Controls, minimap, example and benchmark.
- `a40ce48` — read-only scene caches, finite derived geometry, bounded redo, docs.
- `8bb0bbd` — shared theme integration, retained node entities, cached background
  and minimap triangles, conservative mesh bounds and additional GPUI regressions.

No branch was merged. Only the assigned two crates and the one-line lockfile
entry for flow-core's serde_json dependency were changed.

## Validation

All passed at the source revision:

```sh
cargo test -p guiraffe-flow -p guiraffe-flow-core
cargo clippy -p guiraffe-flow -p guiraffe-flow-core --all-targets -- -D warnings
cargo fmt -p guiraffe-flow -p guiraffe-flow-core -- --check
git diff --check
```

40 tests passed: 24 flow-core integration tests and 16 flow tests. Of the latter,
eight use GPUI TestAppContext with real window event dispatch; five cover the
pure gesture state machine and three cover mesh/minimap geometry and cache
bounds. No tests were ignored. Library production code has no unwrap/expect or
unsafe code; expects are limited to tests and examples.

The GPUI tests assert control activation, node drag/snap, keyboard undo/redo,
selection/deletion, valid and duplicate handle connections, wheel pan, keyboard
zoom, native pinch anchoring, custom renderer use, viewport unmounting and
observed node bounds after pan. They do not assert rendered pixels or native
platform accessibility/trackpad delivery.

## Criterion-free stress measurements

Both examples construct 5,000 grid nodes and 10,000 edges. Edges connect each
node to the next node and the node 100 places ahead, wrapping at 5,000.

### Core geometry

```sh
CARGO_TARGET_DIR=/tmp/guiraffe-flow-core-target \
  cargo run --release -p guiraffe-flow-core --example stress
```

- Build immutable scene, routes and indexes: **11.125 ms**.
- Query plus cached route access, 10,000 measured views after 100 warm-up views:
  **p50 10 us, p95 16 us, p99 18 us**.
- Query window: 1920 × 1080 world units, moving across the graph.
- Last view: 63 nodes, 146 edges, 258 BVH branches visited.
- Checksum: 49,365,063 cached route samples consumed through `black_box`.

The query reuses pre-reserved output buffers. This measures spatial queries and
cached route access, not UI layout, tessellation, paint or presentation.

### GPUI headless frame path

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=2 \
  cargo run -p guiraffe-flow --example stress
```

- 200 measured pan frames after 20 warm-up frames, zoom 1.
- Default headless window (`WindowOptions::default()`); GPUI's default 1536 ×
  1095 size is clipped to its 1920 × 1080 test display.
- Full node content, dots, minimap, zoom/fit/history Controls and arrow markers
  enabled. Graph contains no animated edges or labels in this benchmark.
- Frame update + GPUI render/paint wall time:
  **p50 6.523 ms, p95 7.928 ms, p99 9.440 ms**.
- Last frame admits **49 nodes and 122 edges** after culling.
- Last canvas paint stages, individually sampled (not distributions):
  grid 148 us, edges 721 us, minimap 151 us.

This uses opt-level 2 for local crates and the workspace's opt-level 2 dependency
profile, with dev debug symbols disabled. It uses GPUI test support and forces
complete frames; it is not a release application or GPU/presentation benchmark.
The default unoptimized local-crate dev profile is a different measurement.

The host ran other agent builds during development. Earlier wall-time medians
ranged from 11.7 to 75.3 ms while one-minute load reached 67.7 on 16 CPUs; the
final run completed with load around 12.8. These are shared-host observations,
not controlled comparative speedup evidence. Stage instrumentation identified
repeated grid tessellation (about 5 ms in one earlier sample); cached triangles
removed that repeated work. A fresh quiet run is needed for formal performance
acceptance. The final sampled CPU path fits a 16.7 ms frame budget, but **60fps
GPU presentation and its tail latency have not been established**.

## Remaining limits

- The benchmark pans a normal-sized viewport through the large graph. Fitting
  all 5,000 nodes on screen mounts full node content and is not measured here;
  overview aggregation/LOD remains future work.
- GPUI paint cache keys include absolute bounds. Retained node entities reuse
  paint only while bounds/style/state match; panning can rebuild node content.
- UI element construction and submitted triangle copies allocate. There is no
  claim of a zero-allocation GPUI frame. Core spatial queries reuse buffers.
- Document edits rebuild routes and indexes and store bounded complete graph
  history snapshots. Dragging previews without rebuilding; release commits once.
- Parent ownership uses absolute coordinates, with no implicit group frames,
  child transforms or subtree dragging. Hosts can apply subtree moves atomically.
- Parallel edges overlap. Edge label truncation is bounded by the UI width;
  there is no lane separation or interactive edge-label inspection.
- Layered layout is deterministic topological layering with a cycle fallback;
  it does not implement dagre crossing minimization.
- Keyboard navigation supports canvas commands and stock Controls; individual
  handles and edge labels are not keyboard connection targets.

Run `cargo run -p guiraffe-flow --example editor` for the interactive three-node
example with a labelled animated edge. A native display visual check, real
trackpad check and GPU refresh-rate measurement are still required.
