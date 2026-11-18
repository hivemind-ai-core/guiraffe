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
