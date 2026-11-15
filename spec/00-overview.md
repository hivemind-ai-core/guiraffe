# GUIraffe 🦒 — Specification

Goal: a framework + addon widget crates for rich, beautiful, FAST GPUI interfaces.
Base: `gpui-kit =0.9.1` (GPUI + GPUI Controls = `gpui_kit::component`, i.e. gpui-component).
Never re-implement something Controls already ships well (Button, Input, Select, Dialog,
Tabs, Table, Tree, Tooltip, Popover, Accordion, Slider, Switch, Checkbox, Radio, Badge,
Avatar, Kbd, Progress, Skeleton, Spinner, Sidebar, Dock, Resizable, Notification, ...).
Wrap/theme them instead; fill the gaps Mantine has.

## Principles
1. **Fast**: 120fps target. No per-frame allocation in hot paths, virtualize long lists,
   cache layout/tessellation, benchmarks for flow + markdown.
2. **Beautiful**: visual language from `../distrarec` (soft surfaces, subtle borders,
   generous spacing, tasteful accent) and `../mocket` (agent chat UI polish).
3. **Mantine-shaped API**: builder props (`.size(Size::Md)`, `.variant(Variant::Light)`,
   `.color(ColorName::Blue)`, `.radius(..)`), theme tokens, color scheme light/dark/auto.
4. **Rock solid**: no `unwrap` in library code, every public behaviour unit tested,
   pure logic split from GPUI so it can be tested headlessly; GPUI `TestAppContext`
   tests for interaction. Gallery app exercises every widget.

## Crates
| crate | purpose | spec |
|---|---|---|
| guiraffe-core | theme tokens, palettes (10 shades/color), sizes, variants, style helpers | 01-core.md |
| guiraffe-layout | Stack, Group, Flex, Grid, SimpleGrid, Container, Center, AspectRatio, Space, AppShell | 02-layout.md |
| guiraffe-components | Mantine components missing from Controls | 03-components.md |
| guiraffe-overlays | modal manager, drawer, toasts, spotlight, context menu, desktop notifications | 04-overlays.md |
| guiraffe-windows | multi-window registry, lifecycle, messaging, persistence | 05-windows.md |
| guiraffe-markdown | Markdown viewer (+ `mermaid` feature) | 06-markdown-mermaid.md |
| guiraffe-mermaid | standalone Mermaid viewer | 06-markdown-mermaid.md |
| guiraffe-flow-core / guiraffe-flow | xyflow-style graph (model is GPUI-free) | 07-flow.md |
| guiraffe-gallery | storybook app | — |

## Prior art to mine
- `../gui/workbench/crates/{graph-core,graph-ui,mer,dock-host}` and `../gui/research/*`
  (graph, mermaid, docks, multi-window); read `../gui/KNOWN_ISSUES.md`.
- `../switchboard/src/gui*`, `notifications.rs` (overlays, desktop notifications, tray).
- `../distrarec/src/ui` styling. `../mocket/src` (React) agent chat UI.
