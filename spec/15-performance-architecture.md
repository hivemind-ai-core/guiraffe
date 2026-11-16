# Performance architecture (from day one)

Four goals: **few re-renders, cheap re-renders, minimal data motion, nothing heavy on the UI thread.**

## 1. Few re-renders
gpui-fast retained rendering. Granular views. Change-only notifications through state selections and slot dirty bits.
Render-count tests (conventions §6).

## 2. Cheap re-renders
- Precompute in builders and models, not in `render`. Highlight ranges, layouts, shaped text and tessellated
  paths are cached by content hash and invalidated by version counters.
- Style context and theme are `Copy` structs, so there are no allocations to read them.
- Virtualise long content. Draw static backgrounds once and move them with transforms.

## 3. Minimal data motion
- **Share, don't copy:** `Arc`/`SharedString` for text and documents, structural sharing (`im`) for
  big collections, `Bytes`-style slices for buffers. Snapshots and undo are pointer copies.
- **Diffs, not documents:** state selections compare and notify, and data-UI logic sends slot patches, so the
  amount of data moved is proportional to the change, not the state size. This keeps a future wasm boundary cheap.
- **Small events:** ids (`u32`) plus compact payloads, and borrowed views where possible.

## 4. Background work without locks on hot paths
- Parsing (markdown, mermaid, data-UI documents), layout of large graphs, search indexing and IO run on the
  background executor and hand results to the UI thread whole, never as shared mutable state.
- **Hand-off structures:**
  - **double or triple buffering** with an atomic swap for continuously produced data (streaming tokens, flow layout, telemetry); the UI reads the last complete buffer without waiting;
  - **SPSC ring buffers** (lock-free) from one producer to the UI;
  - **channels** for discrete results;
  - immutable `Arc` snapshots published by pointer swap.
- **No `Mutex` on the UI thread's hot path.** Contention-prone shared state is redesigned as ownership plus hand-off.
- Every hand-off structure has loom or property tests and a benchmark.

## Enforcement
- Allocation-per-frame and render-count gates in CI (bench crate).
- Review checklist item: "what moves per interaction, and is it proportional to the change?"
- Benchmarks for every hand-off structure and for data-UI instantiation and patch application.
