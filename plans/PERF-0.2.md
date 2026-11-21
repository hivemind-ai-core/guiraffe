# Performance programme 0.2 (foreman plan, 2026-11-21)

Goal: every spec/23 component within budget at its scale target, memory under control, locked in by
load-independent tests. Foreman: Claude (profiling, diagnosis review, review, gates, quiet-window
timing, merges). Workers never merge, push or create branches other than their own.

## Budgets (release build, quiet machine)
- mount → first useful frame ≤ 200 ms; scroll/pan/zoom/hover/select p99 ≤ 8 ms; no main-thread task > 50 ms
- Electron comparison scenarios: p99 frame ≤ 16.7 ms, no regression elsewhere
- peak RSS at scale ≤ 1.25 × raw data + 64 MiB; every cache bounded and documented in PRODUCTION.md;
  soak drift ≤ 2 % over 30 min
- warm repaint of an unchanged view allocates nothing; per-frame work at 10M items == at 10k
- every improvement locked by an instruction-count, allocation-count or scaling test

## Tools
- `bench/examples/soak.rs <flow|timeline|chart-line|chart-scatter|chart-stream> --short` — mount + 2×10 s
  frame percentiles + RSS/PSS. `SOAK_SCALE=n` divides sizes; `SOAK_FRAMES=n` runs n frames only (profiling).
- `bench/examples/production_soak.rs <grid|code>`.
- Profile: `samply record --save-only --unstable-presymbolicate -o prof.json.gz -- target/release/examples/soak X --short`
  then `scripts/profile-summary.py prof.json.gz [--filter SUBSTR] [--top N]` (self + inclusive hot list).
  Also available: valgrind/callgrind (+qcachegrind), iai-callgrind suites in guiraffe-bench.
- Wall-clock numbers only count from the foreman's quiet-window runs. Workers iterate with profiles,
  instruction counts, allocation counts and scaling tests, and may take rough local timings.

## Baseline (quiet machine, 2026-11-21, release, upstream backend)
| scenario | mount | p50 / p95 / p99 / max frame | RSS | RSS limit | verdict |
|---|---|---|---|---|---|
| timeline (dope sheet 100×1000 keys) | 25 ms | 6.0 / 12.0 / 13.5 / 17.2 ms | 45 MiB | ~66 MiB | p99 ❌ |
| chart-line (10M pts) | 6 ms | 2.3 / 5.2 / 5.8 / 8.2 ms | 1.86 GiB | ~255 MiB | memory ❌ (7×) |
| chart-scatter (1M pts) | 12 ms | 7.1 / 15.9 / 17.6 / 18.7 ms | 216 MiB | ~84 MiB | p99 ❌, memory ❌ |
| flow (50k nodes) | 226 ms | 4.0 / 5.6 / 6.7 / 8.5 ms | 314→409 MiB in 10 s | tbd | mount ❌, drift ❌ |
| grid / code / flamegraph / tree / gantt / tracks / curve+bar editors / markdown / mermaid / chat | — | — | — | — | not yet probed mounted |

## Diagnoses (from samply profiles in perf-run/, reviewed by foreman)
- **Charts scatter**: 56 % `Projection::generate` — every zoom step re-projects all 1M points and rebuilds the
  scene; 28 % `gpui::BoundsTree::insert` — one `paint_quad` per point. Fix: per-pixel aggregation (≤1 mark per
  device pixel via a bitset/density grid; density raster at overview zoom), batch marks into few primitives,
  spatial index (grid/kd) for viewport culling, reuse scene between zoom steps (incremental re-projection or
  transform), keep generate off-thread & cancellable.
- **Charts line**: 24 % `Projection::anchor` + 23 % `PreparedChart::nearest` + 10 % `xy` — hover/nearest and
  per-frame projection scan points linearly. Fix: binary search on sorted x over the LOD level in view,
  cache projected polyline per (viewport, level). Memory 1.86 GiB for 10M points: find duplicated
  series/levels/copies (store columns once as f32/f64 SoA, LOD pyramid ≤ 1/3 extra, no per-point structs).
- **Timeline dope sheet**: per frame renders a Kit `Button` + layout tree per channel row (≈30 % of time in
  `request_layout` memcpy of large elements), 7.8 % `DopeSheet::render`, 8 % path triangles. Fix: render only
  visible rows (virtualise), replace per-row Kit widgets with one painted canvas for keys + lightweight row
  headers, merge keys per pixel column (LOD), batch keys into one path/quad batch, and don't re-render
  the view on pure scroll (scroll offset applied in paint).
- **Flow**: mount 226 ms; per frame clones edge `PathVertex` vectors and rescales paths (`Vec::clone`,
  `Path::scale`, ~12 %), 8 % `BoundsTree::insert`, large layout memcpy; RSS grows 30 % in 10 s → unbounded
  cache or leak. Fix: cache tessellated edge paths per zoom bucket instead of cloning, LOD at low zoom
  (straight lines, no ports/labels, batched), culling via spatial index, staged mount (first screen then
  background indexing), find and bound the growing structure.

## Work packages (owners)
1. Charts (line, scatter, series, stream) — codex
2. Timeline family (dope sheet, Gantt, tracks, curve + bar editors; add mounted probes for each) — codex
3. Flow — codex
4. FlameGraph + TreeList/PropertyGrid (add mounted probes, profile, fix) — codex
5. DataGrid + CodeViewer + Sheet-ready grid paths (mounted probes exist: production_soak) — codex
6. Frame pacing in the Electron comparison app (p99 17.5 → ≤16.7 ms; GPUI frame scheduling, idle work,
   text shaping caches) + cross-cutting render allocation audit — kilo
7. Markdown 50k lines streaming, Mermaid 500 nodes, Chat 100k messages: mounted probes + profiles (side
   work, slow is fine, commit often) — omp
8. Independent reviews of the riskiest rewrites (batched painting, canvas rewrites) — Claude subagents
