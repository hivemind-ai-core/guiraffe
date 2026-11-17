# Changelog

## 0.1.0 (unreleased)

First release of GUIraffe 🦒: a Mantine-inspired framework and component library for rich,
fast desktop apps on GPUI (gpui-kit 0.9.1), aiming to be as convenient as React and lighter than Electron.

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
  bubbles, folds, trace lines; million-line documents.

### Tooling
- **cli:** `guiraffe new` (standalone app), `preview` (hot-reloaded UI-as-data), `import`.
- **showcase:** every component live, searchable, both themes, live option panels; a
  coverage test fails when a public component has no entry.
- **gallery:** reference app.
- **test:** headless snapshot harness with per-backend goldens, image diff, render counters,
  layout-bounds assertions.
- **bench:** reproducible benchmarks, baselines, commit-to-commit compare.

### Known limitations
See KNOWN_ISSUES.md. Notably: gpui-fast pinned to 0.1.15; no OS high-contrast/forced-colours
signal yet; Kit has four size steps; Kit charts lack legends.
