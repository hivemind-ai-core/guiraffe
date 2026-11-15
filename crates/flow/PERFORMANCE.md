# Flow implementation hand-in and measurements

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
