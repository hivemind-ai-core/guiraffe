# Reference

Generate the API documentation with `cargo doc --workspace --no-deps --open`.

## Crates

### Foundation

| Crate | Provides |
|---|---|
| [`guiraffe-core`](../crates/core) | Theme, tokens, style context, focus and press behavior |
| [`guiraffe-layout`](../crates/layout) | Stack, Group, Grid, AppShell, Splitter and other layout |
| [`guiraffe-icons`](../crates/icons) | The Lucide icon set |
| [`guiraffe-motion`](../crates/motion) | Springs, transitions, presence, layout animation |

### Writing UI

| Crate | Provides |
|---|---|
| [`guiraffe-ui`](../crates/ui/README.md) | `ui!`, `#[component]` and DataUi |
| [`guiraffe-macros`](../crates/macros/README.md) | The macros behind `guiraffe-ui` |
| [`guiraffe-mocket`](../crates/mocket) | Import Mocket designs as DataUi documents |

### App services

| Crate | Provides |
|---|---|
| [`guiraffe-state`](../crates/state) | Event-driven store, effects, undo, selections |
| [`guiraffe-router`](../crates/router) | Typed routes, history, guards |
| [`guiraffe-commands`](../crates/commands/README.md) | Commands, keybindings, menus, Spotlight adapter |
| [`guiraffe-windows`](../crates/windows) | Window registry, persistence, notification windows |
| [`guiraffe-tray`](../crates/tray) | System tray |
| [`guiraffe-dnd`](../crates/dnd) | Drag and drop, sortable boards, docking |

### Components

| Crate | Provides |
|---|---|
| [`guiraffe-components`](../crates/components) | Cards, typography, segmented control, stepper and more |
| [`guiraffe-inputs`](../crates/inputs) | Chips, tags, range slider, dropzone, date and time pickers |
| [`guiraffe-overlays`](../crates/overlays/README.md) | Modals, drawers, toasts, menus, Spotlight, system notifications |

### Data

| Crate | Provides |
|---|---|
| [`guiraffe-datagrid`](../crates/datagrid) | Virtualized data grid |
| [`guiraffe-sheet`](../crates/sheet) | Spreadsheet with formulas |
| [`guiraffe-tree`](../crates/tree) | Tree list and property grid |
| [`guiraffe-calendar`](../crates/calendar) | Year, month, week and day calendar |

### Visualization

| Crate | Provides |
|---|---|
| [`guiraffe-charts`](../crates/charts) | Grammar-of-graphics charts |
| [`guiraffe-flamegraph`](../crates/flamegraph) | Flame graphs |
| [`guiraffe-timeline`](../crates/timeline/README.md) | Gantt, tracks, dope sheet, curve and bar editors, event timeline |
| [`guiraffe-flow`](../crates/flow/README.md) | Node graph editor |
| [`guiraffe-flow-core`](../crates/flow-core/README.md) | The GPUI-free graph model |

### Documents

| Crate | Provides |
|---|---|
| [`guiraffe-code`](../crates/code/README.md) | Code viewer with annotations |
| [`guiraffe-markdown`](../crates/markdown/README.md) | Markdown viewer |
| [`guiraffe-mermaid`](../crates/mermaid/README.md) | Mermaid diagrams |
| [`guiraffe-chat`](../crates/chat/README.md) | Streaming agent chat |

### Tools

| Crate | Provides |
|---|---|
| [`guiraffe-cli`](../crates/cli/README.md) | The `guiraffe` command |
| [`guiraffe-devtools`](../crates/devtools/README.md) | Render inspector, profiler, state history |
| [`guiraffe-test`](../crates/test) | Snapshots, inspect and drive, render counters |
| [`guiraffe-showcase`](../crates/showcase) | Every component, live |

## Examples

| Example | Run |
|---|---|
| Showcase | `cargo run --release -p guiraffe-showcase --features gpui-fast` |
| Quickstart | `cargo run -p getting-started-examples --example 00_quickstart --features gpui-fast` |
| Tutorial apps | `cargo run -p getting-started-examples --example 01_hello --features gpui-fast` (through `06_router_store`) |
| Team tasks, written with `ui!` | `cargo run -p team-tasks --features gpui-fast` |
| Notifications and tray | `cargo run -p desktop --features gpui-fast` |

Several crates also have focused examples in their `examples` directory.

## Command line

Build it with `cargo build -p guiraffe-cli`.

| Command | Does |
|---|---|
| `guiraffe new <dir> --path <checkout>` | Create an app |
| `guiraffe add <crate>...` | Add GUIraffe crates to an app |
| `guiraffe dev` | Rebuild and relaunch on change |
| `guiraffe doctor` | Check pins and setup |
| `guiraffe catalog --json\|--schema\|--markdown` | Print the DataUi catalogue |
| `guiraffe preview <file>` | Preview a DataUi document with reload |
| `guiraffe import <file>` | Convert a Mocket design |

## Features

- `gpui-fast`: the retained rendering backend. Recommended for apps.
- `devtools`: developer overlay. Absent from normal builds.
