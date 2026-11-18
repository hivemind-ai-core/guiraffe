# Production readiness for complex components

Scope: DataGrid, Sheet, Kit Table usage, FlameGraph, TreeList/PropertyGrid, Flow (node graph),
Timeline family (Gantt, tracks, dope sheet, curve/bar editors), CodeViewer, Charts, Markdown/
Mermaid, Chat. Goal: ready for heavy industrial use — big data, long sessions, no surprises.

A component is **production-ready** when it meets every item below, each backed by an automated
check in CI (not a one-off measurement). Budgets are for a release build on the reference machine
(record CPU/GPU in `bench/baselines`); CI fails on regression beyond the bench threshold.

## 1. Scale targets (must stay interactive)
| Component | Dataset |
|---|---|
| DataGrid | 10M rows × 50 cols (computed source), 1M rows materialised |
| Sheet | 1M × 1k sparse cells, 100k formulas |
| FlameGraph | 10M frames, depth 500 |
| TreeList / PropertyGrid | 1M nodes, 100k expanded |
| Flow | 50k nodes / 100k edges |
| Timeline | 100k bars/clips/keys |
| CodeViewer | 10M lines, 1 MB single line |
| Charts | 10M points line, 1M scatter, 100 series |
| Markdown | 50k lines streaming; Mermaid 500-node diagram |
| Chat | 100k messages history |

## 2. Latency budgets (p99 over a scripted session)
- Input → frame for scroll/pan/zoom/hover/selection: **≤ 8 ms** work per frame (120 Hz-capable),
  never > 16 ms; no main-thread task > 50 ms (long-task detector).
- Expensive work (sort, filter, layout of large graphs, parsing, highlighting, formula recalc,
  downsampling) runs off the UI thread, is cancellable, and the UI stays responsive (progress state).
- Cold mount of the scale dataset ≤ 200 ms to first useful frame (lazy/virtualised).

## 3. Memory budgets
- Memory **O(viewport)** for rendering state; data held once, borrowed (no per-row clones).
- Per-item overhead documented and tested (e.g. DataGrid ≤ 16 B per row for index/permutation,
  Tree ≤ 48 B per visible row, Flow ≤ 256 B per node excluding user data).
- **Zero allocations** on warm repaint/scroll paths (allocator gate); bounded allocations for
  recompute paths with a test.
- **No leaks**: soak test (§5) shows flat RSS/PSS after warm-up (≤ 2 % drift over 30 min).

## 4. Correctness & robustness
- Property tests (proptest, Hegel pilot from plans/RELEASE-0.1 §Hardening) for every model:
  random op sequences vs a naive model (edits, undo/redo, sort/filter, expand/collapse, graph ops).
- Fuzzing of parsers/importers (markdown, mermaid, TSV/clipboard, Mocket, catalogue documents)
  with cargo-fuzz; no panics, bounded time/memory per input.
- Edge cases tested: empty, one item, huge values, NaN/inf, very long text, RTL/CJK/emoji text,
  zero-size viewport, DPI changes, theme switches mid-interaction, rapid input bursts.
- No `unwrap`/`expect`/panics in library code; errors surface as events/results.
- Concurrency: background jobs cancel on supersede/drop; no UI-thread blocking on locks.

## 5. Long-session & stress
- **Soak tests** (nightly): 30-minute scripted sessions per component at scale (scroll, edit,
  zoom, stream) using the inspect/drive replay (spec/21), recording frame-time percentiles and
  RSS/PSS every 10 s; fail on drift or budget breach.
- **Streaming**: sustained appends (chat 50 tok/s, charts 10k pts/s, markdown, logs) for 30 min
  without frame drops beyond budget.

## 6. Accessibility & UX under load
- Keyboard-only operation complete; focus never lost on data updates; screen-reader announces
  selection/edits; virtualised items expose correct indices/counts.
- Visual: snapshot tests at scale (top, middle, end of dataset), both themes, both backends.

## 7. Process
For each component: audit against this spec → gap list → fixes with tests → independent review →
update `bench/baselines` and a `PRODUCTION.md` table (component × criterion: ✅/⚠️ with link to the
check). A component is labelled "production-ready" in docs only when all rows are ✅.
