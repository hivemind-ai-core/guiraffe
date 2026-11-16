# Flow review #2 follow-ups

Implementation source: `a0dae350e54c565dcae9b532442ff1cc77631098`; baseline
`aa82bb4c0c9ee98310fcc9dee8696fb59d19a6f7`, both on gpui-kit 0.9.1.
Both backend suites pass 90 tests, including gpui-fast host/sibling render isolation.
See [REVIEW-2.md](REVIEW-2.md) for per-item status, exact verification limits and
[raw alternating measurements](review-2-benchmark.json).

Ten alternating AB/BA runs per variant, pinned to CPU 15, give median results:

| Workload | Before | After |
|---|---:|---:|
| Scene build | 13.349 ms | 13.427 ms |
| Query p50 | 8 us | 8 us |
| Move p50 | 58 us | 57.5 us |
| Child delete + undo p50 | 1.085 ms | 0.105 ms |
| Headless frame p50 | 4.788 ms | 4.610 ms |
| Headless frame p99 | 7.783 ms | 7.713 ms |

Child deletion is about 10.3x faster; the other run distributions substantially overlap.
These remain CPU/headless observations, not a GPU or formal frame-budget acceptance.
The approved benchmark registration is now applied. The accepted
[`guiraffe-bench compare` output](review-2-harness-comparison.json) pins identical
harness source on baseline `4343b6f` and implementation `ceab4c8`, ten alternating
pairs on CPU 15. Child select/delete + undo improves from 3.086 ms to 0.267 ms
(about 11.6x; relative 95% CI -95.73% to -84.72%). Scene build, twenty-query and
move+undo intervals are wide and inconclusive on the noisy shared host. These
workloads differ from the custom protocol above; see REVIEW-2.md for exact scope.
Both backend combined suites pass 122 tests (90 flow, 32 benchmark).
The earlier [harness smoke output](review-2-harness-smoke.json) is historical only.

---

# Flow review #1 validation and alternating benchmarks

Validated source: `77530eabec54379ef1cbc34a50716cd59803245e`, rebased onto
release `8d0f992f37a22d271ae7d49f505ee3e3beefe681`, **gpui-kit =0.9.1**.
The foreman requested this upgrade after the original 0.7.0 task dispatch.
All 70 tests, package Clippy with `-D warnings`, formatting and diff checks pass.
See [REVIEW-1.md](REVIEW-1.md) for the per-item resolution and exact test limits.
No branch was merged or pushed.

## Alternating old/new measurements

The original flow sources at `4c9969c` were installed in a temporary Orca worktree
based on release `f96e93a`; its core/theme and root manifest match `8d0f992`.
Only the deprecated scroll names were ported to 0.9.1 for the baseline UI, and the
same `edit_stress` example was added to both. Both variants therefore use the
same GPUI version, host, toolchain and benchmark source. The new variant is
`77530ea` with the review fixes. The baseline worktree was removed after measurement.

Binaries were built once, copied to distinct paths to avoid Cargo's two `stress`
example filename collision, then run **ten times per variant per benchmark** on
CPU 15 with `taskset`. Each run alternated AB/BA order; no new build was started
by this worker during the measurement. Other workloads continued on the shared
16-CPU host. Load fell from roughly 35–68 earlier in development to checkpoints
of 3.8 and 3.5 during the final run. Alternation reduces order bias; it does not
establish formal performance acceptance or exclude scheduler interference.

Entries below are **medians across runs**, with the **minimum–maximum across
runs** in parentheses. For distribution rows these are medians/ranges of the
individual runs' reported percentiles, not pooled percentiles.

| Measurement | Before | After |
|---|---:|---:|
| Scene build | 9.924 ms (9.078–11.370) | 14.254 ms (14.010–16.856) |
| Spatial query + cached routes p50 | 9 us (8–9) | 8 us (8–8) |
| Spatial query + cached routes p95 | 10 us (10–13) | 10 us (10–13) |
| Spatial query + cached routes p99 | 13 us (13–16) | 14 us (13–18) |
| One-node edit p50 | 13.920 ms (13.612–14.109) | 0.085 ms (0.082–0.092) |
| One-node edit p95 | 16.169 ms (15.518–16.779) | 0.135 ms (0.126–0.141) |
| One-node edit p99 | 17.843 ms (16.385–19.998) | 0.192 ms (0.139–0.472) |
| Headless frame p50 | 6.383 ms (5.789–6.637) | 5.671 ms (4.948–6.447) |
| Headless frame p95 | 8.270 ms (7.720–9.026) | 7.673 ms (6.894–8.518) |
| Headless frame p99 | 9.248 ms (8.663–9.887) | 8.681 ms (7.868–9.402) |

The median edit result improved by about 164x. Persistent maps and path-copyable
indexes retain a **44% median scene-build cost** in this comparison, an explicit
tradeoff. Median query time was 8 us versus 9 us, with overlapping run ranges.
Transient std indexes for bulk handle lookup/validation, bulk location collection
and packed BVH paths removed avoidable overhead. The initial isolated 12→62 ms
build observations were not sustained by the interleaved comparison. Headless
frame distributions were modestly lower; their ranges overlap, and no GPU refresh
rate or formal frame acceptance is claimed. The new median run p99 is 8.682 ms,
so the 8 ms p99 design budget remains unproven.

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
