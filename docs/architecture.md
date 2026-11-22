# Architecture

```text
your app
  ├─ guiraffe-ui          ui!, #[component], DataUi documents
  ├─ components           layout, inputs, overlays, datagrid, sheet, charts, flow, ...
  ├─ app services         state, router, commands, windows, motion, tray
  ├─ guiraffe-core        theme tokens, style context, focus and press behavior
  └─ GPUI Kit + GPUI      widgets, text, layout, GPU rendering
```

Each crate is usable without the others above it. `guiraffe-state` is optional.

## Building blocks

- A **component** is a value built during render, like `Card::new()`. It has no identity.
- A **control** is backed by an entity that owns interaction state and emits typed events, like a
  grid or a picker. Create its state once and render it as often as you like.
- A **document** is JSON compiled once into a template with slots. Changes patch slots.

[API conventions](contributing/api-conventions.md) defines the rules all three follow.

## Style flows down

Containers provide size, spacing, density and radius through an inherited style context, and
widgets read it. A prop on one widget is an escape hatch. Colors come from semantic theme tokens,
so states keep their contrast across schemes.

## Work follows change

- State that changes often lives in a small view, which notifies only itself.
- Large collections are virtualized or culled to the viewport.
- Caches have a bound or an owner that releases them.
- Parsing, sorting, layout of large graphs and recalculation run off the UI thread and cancel when
  superseded.

## Two backends

`gpui-fast` retains layout and paint for unchanged subtrees and is recommended for apps. Upstream
GPUI is the default inside this workspace so that both stay tested: a default feature would be
unified into every crate. On upstream, notifying a child also re-renders its ancestors.

## Vendored patches

GUIraffe patches GPUI, GPUI Kit, Taffy and the Mermaid renderer. [vendor/README.md](../vendor/README.md)
lists every change. Apps outside the workspace must repeat the `[patch.crates-io]` table, which
`guiraffe new` does for you and [getting started](getting-started.md#create-your-own-app) shows for git
dependencies.
