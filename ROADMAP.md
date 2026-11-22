# Roadmap

## 0.3: One action, every input

**Goal:** any control in a GUIraffe app can be addressed, observed and driven the same way from a
pointer, key, command palette, ribbon, script or agent, in any window, and a change costs work in
proportion to what changed.

0.3 is done when:

1. A real internal tool, with at least two windows, is built on stores and `ui!` and used for its job.
2. Every interactive control in that tool can be bound to a shortcut, Spotlight and the ribbon, and
   driven over MCP, without simulated clicks. Each source produces the same events and state.
3. A label edit among 3,000 live rows does the same work as among 1,000, in at most 0.5 ms of CPU.
   It took 19-46 ms in 0.2 (provisional).
4. A control nobody has bound costs what it did in 0.2.
5. No budget that 0.2 met regresses, and every P0 issue below is closed.

### How 0.3 is run

- Budgets are measured at the end of every phase, at a sweep of sizes as well as at the maximum.
  Level of detail hid 0.2's worst hot spots at the largest size.
- A bench is dry-run end to end before anyone is asked to leave a machine idle.
- A new component or request displaces something of equal size, or waits for 0.4.
- P0 finishes before P1 APIs are frozen. P3 is cut first.

### P0: foundations (first)

| # | Item | Why it comes first |
|---|---|---|
| 1 | Review 0.2's bench, long soak and heap profile; commit a baseline | Nothing can regress visibly without one |
| 2 | Make CI pass from a clean clone, including the size sweep and leak check | Every later gate depends on it |
| 3 | Resolve the Miri finding in `sized-chunks` (via `im`); evaluate `imbl` | Possible unsoundness under core state types |
| 4 | Fix FlameGraph, Tracks and dope sheet at mid sizes, and Mermaid's 64-node mount | They break the central performance claim |
| 5 | Fix Sheet memory and CodeViewer's load peak; attribute the remaining RSS growth | Known budget failures |
| 6 | Prototype reactive sinks and dirty-path replay on the 3,000-row benchmark | The biggest unknown. **Go/no-go before the binding syntax is frozen** |

### P1: the theme

| # | Item | Notes |
|---|---|---|
| 7 | Control protocol core: ids, snapshots, actions, one bus | [Design](docs/design/control-protocol.md). Includes the inspect and drive ids |
| 8 | Reactive sinks and cheap keyed boundaries | Completes item 6. Includes zero-allocation repaint for DataUi, `ui!`, menus and devtools |
| 9 | Store binding and store-first `ui!` | [Design](docs/design/store-first-ui.md). The builder API stays first-class |
| 10 | Adopt the protocol: simple controls, then rich components | Only as far as the dogfood tool needs before spreading to the rest |
| 11 | Bounded undo history | Count limits exist. Add an optional byte or weight limit, make limits configurable everywhere, drop oldest first |

### P2: what the theme unlocks

| # | Item | Notes |
|---|---|---|
| 12 | Keymap, Spotlight, DataUi, MCP, scripts and replay on the bus | Replaces simulated clicks |
| 13 | Command ribbon | [Design](docs/design/command-ribbon.md). The one new component, and the bus's first real client |
| 14 | Multi-window docking | Pop out, dock back, restore layout. Control ids survive the move. Spike cross-window drag early |
| 15 | The dogfood tool | Start with item 9, so it shapes the API |

### P3: release quality (cut first)

| # | Item | Notes |
|---|---|---|
| 16 | `guiraffe` umbrella crate and one supported way to consume the vendored patches | Copying a patch table is the worst part of adoption today |
| 17 | Compile-only CI for macOS and Windows | No hardware needed. Makes platform claims checkable |
| 18 | Startup study | Phase breakdown, floor against an empty GPUI window, cheap wins |
| 19 | Trim GPUI's per-frame high-water buffers | Accepted 0.2 exception. Falls out of item 8 |
| 20 | Re-test and close the 0.1 and 0.2 visual bug reports, remove the date-picker StyleContext workaround | Many are probably fixed already |

### Added for 0.3

The dogfood tool, the go/no-go on reactivity, the size-sweep gate, the soundness check, working
CI, the umbrella crate, compile-only platform CI and the process rules above.

### Moved to 0.4

| Item | Reason |
|---|---|
| Live drag of panels between windows, if the spike needs upstream GPUI work | Pop out and dock back deliver most of the value |
| Tear-off panels, parent/child and modal windows, X11 restore offset | Not needed for docking |
| Devtools time travel | 0.3 ships the action log it needs |
| Agent authoring: fixtures, recipes library, Mocket coverage overlay | The protocol supersedes the urgent part |
| Mermaid render-cache rework, calendar agenda view, DnD source-card ghost | Unrelated to the theme |
| UI/UX backlog: inactive-window presentation, layout-default pilot, workflow showcases | Better done once apps use stores |
| Selection across virtualized Chat history | Needs the text work in 0.4 |
| GPUI Kit visual and accessibility gaps | Report upstream first |
| Calendar-aware Timeline ticks, style helper traits, showcase option matrix | Polish |
| `guiraffe dev` hot relaunch, docs site, input-latency and energy benchmarks | Tooling |

## 0.4: Text, pixels and platforms

Candidates, in likely order:

1. **Text:** evaluate Parley against GPUI's text system, then rich text editing with IME,
   accessibility text ranges and selection across virtualized content.
2. **Accessibility:** an incremental tree fed by the control registry, its measured cost, and
   screen-reader passes.
3. **Rendering:** gradients, blend modes, filters, masks; decide between patching GPUI and Vello
   in a layer. System compositor layers for video and animation.
4. **Platforms:** test and support macOS and Windows on hardware; publish to crates.io.
5. **Architecture:** a conformance suite for UI documents and one virtualized data-source interface.
6. The items moved from 0.3 above.

Mobile is out of scope until the desktop platforms are supported.
