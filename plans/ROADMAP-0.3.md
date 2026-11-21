# GUIraffe 0.3 roadmap

Decided with the user 2026-11-21. 0.3 has two headline themes and one new component. Everything
else is carry-over and is only pulled in when it doesn't compete with the headlines.

## A. Core state model and control protocol (spec/30)

State becomes a core part of the framework instead of an optional add-on. Every interactive
control becomes addressable (stable id), observable (typed snapshot) and drivable (typed actions)
through one action bus. Physical input, keybindings, Spotlight, the command ribbon, DataUi, MCP and
scripts all go through that bus, so they behave identically and can be recorded, replayed and
undone.

The non-negotiable rule: **registration and binding cost nothing at render time.** Unchanged
repaints still allocate nothing, a control nobody has bound to costs the same as today, and every
gain is locked in by load-independent allocation and scaling tests.

Phases (spec/30 §9): core protocol → simple controls → rich components → Store binding →
MCP/scripts/replay on actions → devtools.

## B. Store-first `ui!` as the canonical path (spec/31)

`ui!` stops being a purely mechanical translation of the builder API. It becomes the structured
path: two-way binding of control props to Store paths, events dispatched straight into the store,
derived values, keyed lists bound to collections, and command/shortcut/Spotlight/ribbon bindings
declared next to the control. Companion macros (`store!`, `#[derive(Event)]`/`events!`,
`#[component(stateful)]`) remove the remaining boilerplate.

The plain builder API stays first-class for standalone UIs that don't want a store, e.g. a small
tool, a transient modal or a component's internals. The structured path is opt-in, and it is the
one that unlocks devtools, undo/redo, time travel, shortcut/Spotlight/ribbon bindings and
agent/MCP control.

## C. New component: command ribbon (spec/32)

A keyboard-first, contextual command bar above the status bar: category columns, chord keys,
submenus, parameter prompts. Ported from the `../gui/research/cmd` prototype (Kit 0.7) onto
`guiraffe-commands` and the spec/30 action bus, so every ribbon entry is also a Spotlight result,
a bindable shortcut and an MCP-invocable action.

## D. Multi-window docking

Pop a panel out into its own OS window, drag panels between windows, dock a floating window's
panel back into another window; cross-window drag sessions; window layout persistence and restore
across monitors and DPI. Builds on the 0.2 DnD framework and in-window docking. Investigate GPUI
cross-window drag support first (may need an upstream patch). Control ids from spec/30 are scoped
per window but must stay addressable across windows (`window/control`), so docking and the
protocol are designed together: moving a panel moves its controls' registrations, not their ids.

Related window work in scope when it falls out naturally: tear-off panels, parent/child and modal
windows, `examples/multi.rs`, the X11 2 px restore offset.

## Carry-over (only if 0.2 doesn't ship them)

Sheet (spec/22); stable inspect/drive node ids (superseded by spec/30 ids, so do them together);
agent authoring rest (spec/21: fixtures, typed handlers, custom widget registration, Mocket
coverage overlay, recipes); `feat-mermaid-2` salvage; calendar agenda view; UI/UX backlog;
startup study (roadmap 0.2 §F, after 0.2 is on main).

## Still later

macOS/Windows CI and testing; crates.io publishing; `guiraffe new` templates and `guiraffe dev`
hot relaunch; docs site from the showcase; compare app input-latency and RAPL metrics.

## Ordering

1. Write and review spec/30 and spec/31 together (the API commitment is the risky part); land the
   catalogue contract in 0.2 already shaped to carry actions and snapshots.
2. spec/30 phase 1 (core) with its zero-cost bench gates from day one.
3. In parallel: command ribbon (it is the first real consumer of the bus), docking investigation.
4. Control adoption by crate (parallel), Store binding, store-first `ui!`.
5. MCP/replay on actions, devtools, docs and the `examples/ui-macro-demo` rewrite onto stores.
