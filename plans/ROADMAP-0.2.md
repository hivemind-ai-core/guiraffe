# 0.2 and beyond (user direction, 2026-11-17)

## A. Components (0.2)
- **DataGrid**: virtualised rows (and columns), fixed/pinned columns, sort/filter/group, column resize/reorder, selection, inline edit, keyboard nav; millions of rows at 60fps with O(viewport) memory, zero-copy row access via a data-source trait.
- **Sheet** (spreadsheet cells): virtualised 2D cells, A1 addressing, selection ranges, fill handle, formulas (pluggable engine), frozen panes, copy/paste TSV; sparse storage.
- Timeline overhaul in flight (codex): composable plot surface layers, Blender-level dope sheet, curve-editor API, quantized bar editor.
- Candidates (to triage): tree view (virtualised, async children), command palette (exists: Spotlight), stepper/wizard, carousel, color picker (Kit?), rich text editor, file browser, terminal emulator view, log viewer (virtualised, ANSI), diff viewer (pairs with code viewer), JSON/inspector tree, kanban board, calendar (month/week/agenda), map/geo (later), audio waveform, image annotator, property inspector (pairs with node graph), hex/asm view, code canvas, git graph (spec/16).

- **DataGrid record editing** (spec/22): type-to-edit, Enter/Tab commit-and-move, single-range
  keyboard/mouse selection, TSV range copy/paste with per-cell validation, Delete, undo.
- **Sheet** (`guiraffe-sheet`, spec/22): full spreadsheet editing — formulas, fill handle/series,
  multiple ranges, Ctrl+D/R, Ctrl+arrow jumps, insert/delete rows/cols, frozen panes, formatting.
- **Shared selection/clipboard/undo model** for DataGrid and Sheet so overlapping behaviour is
  identical. Showcase: a "Which one?" page with the decision table and a live example of each.

## B. Mocket → GUIraffe (0.2)
Status audit of `guiraffe import` vs Mocket's JSON; write **spec/19-mocket-interop.md** (hand-off for Mocket devs): event annotations (`on:<event> → <event-id>` with payload schema), data bindings (`bind:<prop> → <path>` incl. lists/keys, two-way for inputs, formatters), slots/components, style tokens (map to GUIraffe theme/StyleContext, not raw px), versioned schema + validation, round-trip guarantees.

## C. Optimisation pass (0.2)
Fewer re-renders, lower-latency frames, fewer copies, less memory. Measure first (bench + devtools counters + RSS), per crate; publish Electron/React comparison numbers.

## D. Docs and tooling (after the above)
1. Static docs site (Rust→wasm live demos where feasible; otherwise screenshots/recordings), modelled on Mantine + TanStack docs.
2. Tutorial: quick-start whirlwind tour.
3. Guides: theming/StyleContext, state, router, layout, data UI, performance, testing, accessibility, multi-window.
4. Reference: every component with props, examples, live knobs (generated from showcase entries).
5. llms.txt + agent skills.
6. Tooling: LSP/MCP where they add value (e.g. MCP for querying components/theme tokens, inspecting a running app via devtools).
7. i18n/l10n (own pass): message catalogs (Fluent), plurals/formatting, RTL layout, IME, locale-aware widgets.
