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

## Baseline (quiet machine, 2026-11-21 01:19, release, upstream backend; `soak <x> --short`, `production_soak`)
| scenario (scale) | mount | p50 / p95 / p99 / max frame | RSS | verdict |
|---|---|---|---|---|
| curve-editor (100k keys) | 17 ms | 1.9 / 3.9 / 4.6 / 6.4 ms | 27 MiB | ✅ |
| property-grid (110k rows) | 11 ms | 1.4 / 4.7 / 6.1 / 9.3 ms | 29 MiB | ✅ (max > 8) |
| chart-stream (10k pts/s) | 4 ms | 0.4 / 2.2 / 2.9 / 4.2 ms | 26 MiB | ✅ |
| grid (production_soak) | 36 ms | 5.1 / 5.6 / 7.3 / 7.3 ms | 178 MiB | ✅ tight |
| code (production_soak) | 26 ms | 0.3 / 0.3 / 0.5 / 0.5 ms | 345 MiB | ❌ memory (raw 20 MB) |
| timeline / dope sheet (100×1000 keys) | 22 ms | 5.6 / 11.0 / 13.5 / 16.1 ms | 48 MiB | ❌ p99 |
| chart-line (10M pts) | 7 ms | 2.4 / 5.2 / 6.0 / 7.8 ms | 1.82 GiB | ❌ memory (limit ~255 MiB) |
| chart-scatter (1M pts) | 11 ms | 7.4 / 20.0 / 21.6 / 23.3 ms | 219 MiB | ❌ p99, memory (~84 MiB) |
| flow (50k / 100k) | 221 ms | 4.1 / 5.5 / 6.9 / 9.5 ms | 316→412 MiB | ❌ mount, drift |
| bar-editor (100k bars) | 18 ms | 0.02 / 22.5 / 24.2 / 25.4 ms | 19 MiB | ❌ p99 (strokes) |
| tree (1M nodes, 100k expanded) | 81 ms | 0.8 / 61.1 / 64.0 / 82.8 ms | 124 MiB | ❌ p99 |
| flamegraph (10M frames) | 1.47 s | 56 / 336 / 343 / 343 ms | 729 MiB | ❌ all |
| tracks (100k clips) | 2.41 s | 330–487 / 724–1055 / 746–1055 ms | 721 MiB | ❌ all |
| markdown (50k lines + streaming) | 4.69 s | 2.3 / 46.8 / 48.9 / 51.7 ms | 799 MiB | ❌ all |
| mermaid (500 nodes) | 31.8 s | 0.04 / 0.5 / 15.7 / 17.2 ms | 33 MiB | ❌ mount, zoom spike |
| gantt (100k bars + deps) | 26.7 s | 6.7 / 6.97 s / – | 6.66 GiB | ❌ all, catastrophic |
| chat (100k messages + streaming) | 130.5 s | 1.4 / 2.8 / 3.3 / 16.2 ms | 4.49 GiB | ❌ mount, memory |

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
