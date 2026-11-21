# Allocation audit (work in progress — foreman HOLD 2026-11-21)

## Harness
- `guiraffe_test::alloc` (crates/test/src/alloc.rs): thread-local counting allocator, mirror of `bench/src/allocator.rs` (which stays the instruction-count gate owner). Install as `#[global_allocator]` in a test binary, warm, then `count_allocations(draw)`.

## Calibration (guiraffe-ui test binary, upstream backend, debug profile)
- Plain `div` tree repaint, no DataUi: **63 allocations / 52 360 B** per forced draw.
- Bare `vcx.update(|_,_| {})`: 1 allocation. `run_until_parked` alone: 0.
- So ~62 of those are GPUI frame scaffolding (scene/tree/path scratch) charged to every window, not to the component under test. A per-view budget must be stated relative to that framework floor, or measured as "component above plain-tree floor".

## DataUi (crates/ui)
- Mounted DataUi (5 nodes incl. bound text/colour, button, nested stack + sidebar shell), forced repaint: **213 allocations / 156 938 B**.
- Above floor: ~150 allocations. Not yet attributed to call sites — next step is a per-frame allocation backtrace sample (requires a build, blocked by HOLD).
- WIP test (mount + 2 warm draws + counted draw, asserting >= 0 after fixes) parked at `target/alloc-audit-wip/alloc_repaint.rs` so the gate script does not see an unverified failing test.

## Not started (HOLD)
- windows, motion, commands, devtools, state, router, icons, ui! demo: no probes written. Motion probe is written and parked at `target/alloc-audit-wip/motion_alloc_repaint.rs` (unrun).
