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


### D.1 Composable plotting and editing architecture (timeline overhaul)

`PlotSurface` owns the x `Axis`, optional y `ValueRange`, viewport navigation and an ordered stack of shared `Layer` implementations. Layers prepaint bounded drawing commands from a `PlotContext` (visible time/value range, viewport height and row offset); paint consumes those commands in order. Coordinates remain f64 until the GPUI boundary. The stack supports built-in ruler, major/minor x/y gridlines, ticks, playhead with current-frame badge, selection rectangle, markers, zebra stripes and background bands, plus custom layers. Rulers can target a separate header while sharing the same axis. All four timeline views compose this surface; hosts can add a layer with one call. Nothing requires dates or tracks, so charts can reuse the foundation. Native text shaping occurs only for visible labels.

Pan, anchored zoom and fit are implemented once on the surface. Timeline items retain their pure models; editing notifies only the editor entity, never an owning shell. Uniform rows are culled vertically and time-sorted keys/clips horizontally. Model changes rebuild hierarchy summaries; pointer motion does not clone all channels. Heavy host analysis stays outside the editor.

Locks control admission independently of drawing. `editable=false` blocks editing globally, while per-item/channel locks remain independent. Optional `LockStyle::{None, Badge, Dimmed, Hatched}` defaults to Badge for item locks and None for global locks. Dimmed appearance resolves semantic theme tokens, never fixed opacity. Channel/row lock icons are Kit buttons and remain operable under a global lock. Navigation and seeking remain available.

The dope sheet has stable channel identities, parent/group hierarchy, expand/collapse, summary keys, mute/visibility/lock controls, typed key shapes and equal-value hold bars. Selection is explicit: click, additive shift-click, box-select and select-all. A drag applies a frame-snapped delta atomically to selected editable keys; collisions reject the operation. Opt-in focus enables delete, duplicate and frame nudging, with no default focus capture. Collapsing a group retains child selection and channel state. Visible rows are cached/rebuilt on hierarchy changes and culled to the viewport.

The display-focused curve surface exposes editing methods for keys, interpolation, segment insertion/removal and aligned/broken/free tangent handles, so external Kit toolbars own editing controls. Multiple coloured/visible curves share axes; fit and normalize are explicit operations. Selecting a bezier key reveals draggable tangent handles. No automatic keyboard capture is required.

A separate quantized `BarCurve` pure model stores N bounded values, optional snap steps and bipolar presentation. Drawing maps a pointer segment onto every intervening bar, interpolating values in either direction; endpoints and clamping are deterministic. A native `BarCurveView` emits changed index ranges and composes the plot layers. Curve import/export samples over a supplied finite time interval. Zero-bar, single-bar, invalid/non-finite input and high-speed drag cases have explicit tests; generated paths check gap-free interpolation.

Validation includes pure invariants, native pointer/key/toolbar interactions, shell render-count isolation, light/dark snapshots on upstream and fast backends, visual inspection of every regenerated image, Clippy and formatting. Per-editor examples and showcase entries exercise the public composition/editing APIs. Remaining interaction or performance limits must be recorded rather than represented as complete acceptance.

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
