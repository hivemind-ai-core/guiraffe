# Changelog

## 0.1.0 (unreleased)

First release of GUIraffe 🦒: a Mantine-inspired framework and component library for rich,
fast desktop apps on GPUI (gpui-kit 0.9.1), aiming to be as convenient as React and lighter than Electron.

Catalogue 1.0.0 snapshot `3b5ae0c8b27d9e56` introduces descriptors and a schema for the built-in DataUi vocabulary.

### Framework
- **core:** theme (light/dark, Mantine palette, radius/spacing/type scales), inherited
  `StyleContext` (layouts decide size/spacing; per-instance props are escape hatches),
  semantic state tokens (`theme.state`, scrim, veil) meeting WCAG contrast floors for all
  primaries in standard and high contrast, Kit (GPUI Controls) theme bridge, shared control
  behaviour (`Press`: hover/pressed/disabled, no focus on click, opt-in tab stops, AccessKit
  roles), non-layout focus ring, focus release on view swap.
- **motion:** springs/tweens, presence, reduce-motion aware.
- **state:** re-frame-style store: pure handlers, effects/coeffects, interceptors, lens undo,
  change-only selections, debounce.
- **router:** typed routes (`routes!`), guards, history.
- **windows:** multi-window manager (keys, open modes, persistence).
- **overlays:** modals, confirm, toasts, desktop notifications, popovers, floating tooltips,
  Spotlight, LoadingOverlay, Affix.
- **ui:** UI-as-data host (`DataUi`) with granular re-rendering, keyed repeats with stable
  identity, scoped row events, native two-way bindings, typed payloads.
- **mocket:** Mocket design → GUIraffe importer.

### Components
- **layout:** Stack, Group, Flex, Grid/Col, SimpleGrid, Container, Center, AspectRatio,
  Divider, Space, ScrollArea (`track_scroll`), Splitter (persisted), AppShell.
- **components:** Text, Title, Highlight, Mark, Paper, Card, Blockquote, ThemeIcon,
  ActionIcon, Indicator, Pill, RingProgress, SegmentedControl.
- **inputs:** ChipGroup, TagsInput, RangeSlider, PasswordStrength, Dropzone.
- **icons:** full Lucide set (2,359, ISC) as an asset source.
- Plus GPUI Controls (buttons, inputs, select, checkbox/radio/switch, slider, date picker,
  tabs, accordion, menus, table, badges, progress, avatar, alert, breadcrumb, pagination) and
  Kit charts, themed by GUIraffe.

### Rich widgets
- **markdown:** streaming Markdown viewer (code highlighting, tables, task lists, Mermaid).
- **mermaid:** standalone diagram viewer; background rendering; pan/zoom with sharp re-raster.
- **flow / flow-core:** React-Flow-style graph editor: drag, connect, box-select, undo/redo,
  minimap (drag to pan), animated edges.
- **chat:** agent chat UI (streaming replies, reasoning, tool calls, attachments, composer).
- **timeline:** Gantt (critical path, resize either edge, locks), DAW track timeline (move,
  trim, snap, locks, keyboard editing), dope sheet, curve editor.
- **code:** virtualized code viewer: highlighting, analysis highlights, inlay hints, lenses,
  bubbles, folds, trace lines (gutter-routed, cross-viewer), minimap (click/drag), symbol
  cards with go-to-definition; million-line documents.
- **datagrid:** virtualized data grid over a zero-copy source (1M × 50): pinned, resizable,
  reorderable columns; multi-column sort and filters off the UI thread; row/cell/range
  selection that follows rows through sorting; keyboard navigation; inline editing with
  validation; TSV copy; AccessKit grid roles.
- **timeline (overhaul):** composable plot surface layers (ruler, gridlines, marks, playhead,
  selection, markers, stripes, bands); lock state separate from lock styling; Blender-style
  dope sheet (hierarchy, summaries, toggles, key types, hold bars, box selection, keyboard);
  curve editor with a full external editing API (keys, segments, tangents, multi-curve);
  quantized bar curve editor.

- **text menu:** shared right-click menu for selectable text (Copy, Select all, extras;
  Shift+F10) in code, markdown and chat.
- **keys:** `guiraffe_core::keys::{binding, bind_keys}`: OS-aware (`secondary-` → ⌘/Ctrl),
  fallible key bindings that report bad specs instead of panicking.
- **components:** `ActionIcon::loading` (spinner, ignores presses, announced to screen readers).

### Tooling
- **cli:** `guiraffe new` (standalone app), `dev` (watch, rebuild, relaunch; failed builds keep
  the previous app), `add <crate>` (idempotent, reuses the app's checkout), `doctor`
  (toolchain, system libraries with install hints, Vulkan, pins), `preview` (hot-reloaded
  UI-as-data), `import`.
- **showcase:** every component live, searchable, both themes, live option panels; a
  coverage test fails when a public component has no entry.
- **gallery:** reference app.
- **test:** headless snapshot harness with per-backend goldens, image diff, render counters,
  layout-bounds assertions.
- **bench:** reproducible benchmarks, baselines, commit-to-commit compare.

### Fixes since the first showcase review
- Layout: ScrollArea scrolls and supports `track_scroll`; Flex/Grid `grow` work (Mantine
  semantics); vertical `Divider` is drawn inside rows.
- Controls: no focus on click, working Tab order, focus rings that never move layout, AccessKit
  roles; SegmentedControl selects on first click; tooltips and overlays dismiss correctly.
- Theme: every state colour meets WCAG contrast in both schemes and all primaries; Kit primary,
  pressed and semantic (danger/success/warning/info) colours follow the GUIraffe theme.
- Mermaid renders and re-rasterises sharply on zoom; Markdown scrollable mode keeps its height.
- Data grid: even cell cursor, editor matches the cell, horizontal scrollbar drag and paging.
- Icons: build script no longer embeds zero icons from a stale build cache.

### Measurement
- `examples/compare`: the same app in GUIraffe and in Electron + React (Mantine, TanStack
  Table/Virtual, Recharts, react-markdown/Shiki), and `bench/electron/run.sh`, which refuses to
  publish numbers from a loaded or software-rendered machine.

### Known limitations
See KNOWN_ISSUES.md. Notably: gpui-fast pinned to 0.1.15; no OS high-contrast/forced-colours
signal yet; Kit has four size steps; Kit charts lack legends.

Catalogue 1.1.0 snapshot `135f06ec7e53a9cb` adds invalid text-input and select states.
Catalogue 1.2.0 snapshot `3105c738635a3c4d` adds crate-owned views and controls, plus more properties and events on existing components.
