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

- **FlameGraph** (new crate `guiraffe-flamegraph`): low-overhead flame/icicle graph over a
  zero-copy frame source (id, parent, self/total value, label); canvas painting with per-row
  culling and min-width merging so 1M frames stay fast; zoom (wheel/ctrl, click-to-focus a frame),
  pan, breadcrumb reset; hover tooltip; click emits `FrameClicked { id, path, value }`; search
  highlight; theme-token colour schemes (by package/hash/heat); differential mode later.
- **Tree views — two components sharing one virtualized `TreeModel`** (flattened visible-rows
  index, lazy/async children, expand/collapse, keyboard: arrows/←→ collapse-expand, type-ahead,
  multi-select, AccessKit tree/treeitem roles). Two components because their row layout and
  editing differ and a shared model keeps memory O(visible):
  1. `TreeList`: collapsible branches with icons, badges, checkboxes, drag-to-reorder.
  2. `PropertyGrid`: tree of categories/labels + a value column with typed inline editors
     (text, number, bool, enum, colour, nested objects), validation, reset-to-default, read-only.
- **Accordion**: Kit already has `Accordion` (showcase "Kit Accordion"); theme it fully and add
  GUIraffe variants (separated/contained/filled, chevron position, multiple open, controlled).
- **ExpandingList** ("Google Inbox" style): a list where the clicked item animates out of the column
  and expands in place (elevated card, siblings slide apart); closing animates it back into the list;
  one open at a time by default; reduce-motion aware; keyboard Enter/Esc.
- **SpeedDial** (floating action menu): a primary icon button that expands into icon-button
  bubbles vertically, horizontally, or radially (quarter/half circle, configurable radius, start
  angle and direction); labels/tooltips; staggered spring animation; Esc/outside-click closes;
  opt-in focus with arrow navigation.
- **Spotlight redesign**: slick command bar inspired by Mantine Spotlight, VS Code's command palette
  and macOS Spotlight: centered elevated panel with blur-ish backdrop, large search input with icon,
  grouped sections with headers, result rows with icon, title, description, right-aligned shortcut
  chips (from the commands registry), fuzzy match highlighting, recent/pinned items, empty state,
  keyboard-first (↑↓, Enter, Tab to cycle groups, Esc), smooth open/close; clean up the API.
- **Desktop notifications**: (a) system notifications via the OS notification service
  (`overlays::desktop` exists — audit, add actions/buttons, icons, urgency, click → event, macOS/
  Windows/Linux); (b) custom notifications as lightweight borderless always-on-top windows
  (guiraffe-windows) stacked in a screen corner, themed, with actions, auto-dismiss and progress.
- **System tray**: tray icon with menu (from the commands registry), tooltip, click/double-click
  events, dynamic icon/badge, minimize-to-tray helper; Linux (StatusNotifierItem), macOS, Windows.

## B. Mocket → GUIraffe (0.2)
Status audit of `guiraffe import` vs Mocket's JSON; write **spec/19-mocket-interop.md** (hand-off for Mocket devs): event annotations (`on:<event> → <event-id>` with payload schema), data bindings (`bind:<prop> → <path>` incl. lists/keys, two-way for inputs, formatters), slots/components, style tokens (map to GUIraffe theme/StyleContext, not raw px), versioned schema + validation, round-trip guarantees.

- **Flow auto-layout** (crates/flow-core + flow): pluggable layout engines with beautiful defaults
  — layered/Sugiyama (directions TB/LR/BT/RL, crossing minimisation, orthogonal/spline edge
  routing, port-aware), force-directed (stable, incremental), tree/radial, grid/compact packing;
  **constraints**: pinned nodes (fixed position, others lay out around them), relative constraints
  (left-of/above/aligned/same-layer/same-rank), groups/subflows laid out as units, min spacing,
  adjacency/ordering hints, edge-crossing minimisation weight, minimal-movement relayout
  (incremental: only affected region moves, existing layout preserved); animated transitions;
  layout runs off the UI thread, cancellable; deterministic for tests. Evaluate ELK-style options
  and existing crates (e.g. a Rust Sugiyama/`layout-rs`, force engines) before writing our own.

- **Drag & drop framework (0.2)**, on GUIraffe-managed pointer gestures (decision 2026-11-18: GPUI's native
  drag refreshes the whole window on every move on both backends, defeating render isolation; native
  drag is kept only for opt-in external/file drops):
  a `DragSource`/`DropTarget` API with typed payloads, drag previews (ghost), drop indicators,
  auto-scroll near edges, keyboard alternative (pick up with Space, move with arrows, drop with
  Space, Esc cancels — accessible DnD), cancel on Esc, constraints (axis lock, containment, accept
  predicate); OS file drops (exists in Dropzone) and dragging data out to other apps where the
  platform allows. Built on it: **Sortable** lists/grids (reorder with animated displacement,
  multi-select drag, cross-list transfer), DataGrid row/column reordering, TreeList drag-to-reparent,
  Kanban board, Flow node palette → canvas, timeline/curve drags already use their own; tab
  reordering. Render isolation: dragging must not re-render unrelated views.
- **Docking within a window (0.2)**: theme and integrate gpui-kit's `DockArea` (panels, tabbed
  panels, splits, toggleable left/right/bottom docks) with the GUIraffe theme and §12 rules; layout
  persistence (serialize/restore, versioned) via LayoutStore; drag tabs/panels to split or re-dock
  inside the window with drop-zone overlays; panel registry so apps declare panels once; showcase
  "IDE layout" example.

## 0.3 candidates
- **Multi-window docking**: pop a panel out into its own OS window, drag panels between windows,
  dock a floating window's panel back into another window; cross-window drag sessions, window
  layout persistence and restore across monitors/DPI; depends on the 0.2 DnD framework and
  in-window docking, and on GPUI cross-window drag support (investigate first; may need upstream work).

## C2. Production-readiness pass (0.2; spec/23)
Audit and harden DataGrid, Sheet, FlameGraph, Tree views, Flow, Timeline, CodeViewer, Charts,
Markdown/Mermaid and Chat against spec/23: scale targets, ≤8 ms p99 frame work, O(viewport)
memory with per-item budgets, zero-alloc warm paths, property tests + fuzzing, 30-min soak tests
with flat memory, keyboard/a11y under load. Output: PRODUCTION.md readiness table backed by CI.

## C. Optimisation pass (0.2)
Fewer re-renders, lower-latency frames, fewer copies, less memory. Measure first (bench + devtools counters + RSS), per crate; publish Electron/React comparison numbers.

## E. Component requests (2026-11-21, before release readiness and docs)
- **PropertyGrid / TreeList polish** (spec/26): resizable name/value split, alternating rows, row
  heights, auto-sizing value editors, scrub numbers, colour/enum/flags/vector editors, search,
  sticky group headers, keyboard + copy/paste.
- **CodeViewer annotations at scale** (spec/25): stress test (100k marks, 20k squiggles, 5k bubbles,
  full blame) then optimise; underline styles (wavy/dotted/dashed/double), circled/boxed text,
  gutter lanes (breakpoints, change/coverage bars), detailed blame with age heatmap and hover cards,
  inlay hints, ghost text, code lens, review-comment bubbles, minimap marks.
- **Multi-track event timeline** (Mantine Timeline style, new): several timelines side by side as
  parallel tracks (git-branch / concurrent-process style), dots at times/dates/indices with label +
  content rows; multiple dots on the same index stack their content one after another; per-track
  and per-item colour coding; branch/merge connectors between tracks; vertical and horizontal
  orientations; virtualised for long histories; keyboard + a11y.
- **Calendar views** (new crate or in `guiraffe-timeline`): **year** (12 month mini-grids of day
  numbers, heat/marker tint per day), **month** (day cells with number + dot markers / event chips,
  overflow "+N"), **week** (day columns × hour rows, timed events as blocks with overlap layout,
  all-day lane, now-line), **day** (week layout for one day with full width). Shared event model,
  navigation (prev/next/today), drag to create/move/resize events, locale-aware week start, time
  zones, keyboard + a11y, virtualised hour grid.
- **Date & time pickers**: Kit 0.9.1 already has `Calendar`, `DatePicker` (single + range, optional
  time precision → date-time) and `TimeField`. Wrap them as GUIraffe inputs with Mantine props
  (DatePicker, DateRangePicker, TimePicker, DateTimePicker, presets like "Last 7 days", min/max,
  disabled dates, locale/week start), theme tokens and §12 focus/a11y; showcase entries for every
  variant (only the plain date picker is shown today); fill any gaps found.

## D. Docs and tooling (after the above)
1. Static docs site (Rust→wasm live demos where feasible; otherwise screenshots/recordings), modelled on Mantine + TanStack docs.
2. Tutorial: quick-start whirlwind tour.
3. Guides: theming/StyleContext, state, router, layout, data UI, performance, testing, accessibility, multi-window.
4. Reference: every component with props, examples, live knobs (generated from showcase entries).
5. llms.txt + agent skills.
6. Tooling: LSP/MCP where they add value (e.g. MCP for querying components/theme tokens, inspecting a running app via devtools).
7. i18n/l10n (own pass): message catalogs (Fluent), plurals/formatting, RTL layout, IME, locale-aware widgets.
