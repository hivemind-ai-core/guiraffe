# Advanced components

Built as families on shared foundations, so each is fast (virtualized, retained) and consistent.

## Foundation A: `guiraffe-code` (the code surface)
A virtualized **line grid**: only visible lines are shaped and painted, and shaped lines are cached by content hash.
It carries a span/decoration model plus an **overlay layer** in the same coordinate space. Everything in this family builds on it.
- **Code viewer with annotations and analysis:**
  - Syntax highlighting (Controls highlighter/tree-sitter), gutters (line numbers, fold, blame, coverage), inline highlights.
  - **Symbols**: hover cards, go-to-definition callbacks, occurrences highlighted.
  - **Trace lines**: bezier connectors between ranges, such as variable def → uses, or call → callee. They are routed in the overlay and follow scrolling, and they can link across two viewers.
  - **Bubbles**: anchored detail cards with markdown, values or analysis results, collision-avoided along the margin.
  - **Lens rows**: inline hints between lines. **Minimap**.
  - Analysis input is data (`Annotation { range, kind, payload }`), so an LSP, a static analyser or an AI agent (MCP, as CodeCanvas does) can supply it.
- **Hex view**: offset/bytes/ASCII columns over a `Bytes` handle (zero-copy, memory-mapped capable), selection synced across columns, byte grouping and endianness inspector, highlight regions (parsed structures), and 1 GB+ files via virtualization.
- **Assembler view**: address/bytes/mnemonic/operands columns, **branch arrows** in the gutter (the trace-line overlay), symbol labels, and linked highlighting with a hex view or source lines.

## Foundation B: flow (existing) + the code surface → **code canvas / code bubbles**
CodeCanvas-style. Files, functions or code blocks are **flow nodes whose body is a code viewer**, on an infinite canvas.
- Edges come from a relationship graph supplied as data: imports/exports, calls, references and symbol links (LSP).
- **Semantic zoom**: zoomed out shows modules and dependency edges; mid shows file outlines; zoomed in shows live code with symbol links. Layout is automatic (layered/clustered by folder) or manual.
- Diff overlays (review AI or PR changes in place), agent notes as bubbles, and canvas search.
- A file watcher (host-provided) keeps the canvas live. Large repos rely on virtualization: only visible nodes instantiate code views.

## Foundation C: `guiraffe-git` (history graph)
GitKraken-style.
- A **pure lane-assignment algorithm** places commits into columns with stable colours, plus merge and branch curves, computed incrementally as history pages in.
- A virtualized row list: graph column (lanes, avatar dots), ref badges (branch/tag/remote/PR), message, author and time. Rows are selected and the details panel shows the commit, changed files and diff.
- Data comes in through a trait (`HistorySource`: pages of commits with parents and refs) and is backend-agnostic (git2/gix adapter as an optional feature).
- Performance target: 100k commits scroll at 120 fps, with lanes computed off-thread.

## Foundation D: `guiraffe-timeline` (time-based editors)
A shared **time axis**: zoom/pan, rulers (time, frames, timecode, dates), snapping, playhead, and selection rectangles. Tracks are a virtualized vertical list; items are retained per track.
- **Gantt**: tasks with dates and durations, dependencies (finish→start arrows), critical path, milestones, drag to move or resize, group rows.
- **DAW / animation timeline**: tracks containing clips (move, trim, split, snap), playhead scrubbing, markers, loop region, mute/solo lanes, and waveform/thumbnail rendering via dynamic textures.
- **Dope sheet**: rows per property and keyframes as diamonds; select, move or scale groups of keys, with summary rows.
- **Curve editor**: per-property curves with bezier keys and tangent handles (aligned/broken), interpolation modes (constant/linear/bezier) and normalise/frame-all. The curve maths is shared with `guiraffe-motion` easing (exact cubic bezier).
- The pure models (key times, interpolation, snapping, the critical path) are exhaustively unit-tested.

## Required: efficient spreadsheet / data sheet

A reusable spreadsheet-style data sheet for large, typed datasets, usable standalone or embedded in an application.
- Virtualize both rows and columns: instantiate, shape and paint visible cells plus bounded overscan. Keep scrolling work and resident UI state proportional to the viewport.
- Accept host-owned data through a provider with stable row/column identities, lazy ranges and incremental cell/row updates. Preserve selection across updates and avoid cloning the whole dataset on scroll or a single-cell change.
- Spreadsheet interaction: active cell, keyboard navigation, range selection, row/column selection, and clipboard copy/paste with explicit host callbacks. Support read-only mode and optional host-controlled cell editing and validation.
- Configurable column types and formatting, column resizing/reordering, frozen headers/columns, and sorting/filtering through the data provider.
- Reuse GPUI Controls table/grid primitives where they meet these requirements; share the data, selection and rendering foundations across standalone and embedded forms.
- Acceptance: pure tests for viewport/range mapping and selection; GPUI interaction tests for keyboard, pointer and clipboard behavior; light/dark showcase examples; a large-data stress benchmark reporting frame time, allocations, resident cells and update costs as total rows/columns grow with a fixed viewport.

## Performance rules (all families)
Virtualize every axis. Cache shaping and tessellation by content. Overlays are separate leaf entities, so a hover or trace-line change never re-renders the code text.
Data comes in through `Arc`/`Bytes` handles (zero-copy). Heavy analysis (lanes, layout, highlighting) runs off-thread with hand-off as in spec/15.
Each family ships a stress benchmark: a 1M-line file, a 1 GB hex file, 100k commits, and 10k timeline items.

## Order
A (code surface) → hex/asm → code canvas (needs A + flow) → git → timeline (Gantt and DAW first, then dope sheet and curve editor).
