# Changelog

## 0.3.0 (unreleased)

### Added

- Store binding (provisional): `store!`, `#[derive(Lens)]`, lenses with per-path change keys,
  `KeyedList`, and `ui!` bindings (`store(s) { … }`, `prop <= lens`, `prop <=> lens`,
  `{<= lens}`, `if <= lens`, `for row in <= list`, `on_x => Event`). See
  [Stores and binding](docs/guides/stores.md).
- `#[component(stateful)]`: components with their own state, registered on the action bus.
- `Event::source` and `EventRecord::source`; `ActionBus::current_source`.

### Changed

- `EventRecord` has a `source` field. Code that builds an `EventRecord` must set it.

## 0.2.0 (unreleased)

### Added

- Sheet, Calendar, multi-track event timeline, and date and time pickers.
- A redesigned Spotlight, system notifications, notification windows and a system tray.
- The `ui!` and `#[component]` macros, and opt-in layout animation.
- DataUi coverage for the built-in widgets, with a generated catalogue and schema.
- CodeViewer annotations: squiggles, gutter lanes, blame, inlays, code lens, review bubbles.
- PropertyGrid editing options, resizable columns, search and typed editors.
- `scripts/leak-check.py`, a short Valgrind leak check over every soak scenario.

### Changed

- Extensive time, memory and allocation work across the components. Budgets still open are in
  [known issues](KNOWN_ISSUES.md).
- Flow's public model now uses `Name` and `Handles`.
- `Decorations` style keys stay valid until `release_unused_styles` is called.
- `TreeView::forget_removed` and other removal APIs release state for deleted data.
- Kanban columns, rather than cards, are the drop zones.
- Apps must copy the vendored `[patch.crates-io]` table. `guiraffe new` does this.

## 0.1.0 (unreleased)

The first version: theming, layout, components, inputs, overlays, windows, state, routing,
motion, data grid, charts, timeline editors, flow, code, Markdown, Mermaid, chat, devtools, DataUi
and the CLI.

## Catalogue versions

Catalogue 1.0.0 snapshot `3b5ae0c8b27d9e56` introduces descriptors and a schema for the built-in DataUi vocabulary.
Catalogue 1.1.0 snapshot `135f06ec7e53a9cb` adds invalid text-input and select states.
Catalogue 1.2.0 snapshot `9cfca66b723596b0` adds crate-owned views and controls, plus more properties and events on existing components (including the Spotlight command bar, picker, input-wrapper and event-timeline kinds, and PropertyGrid editing options).
Catalogue 1.3.0 snapshot `f02fd1904da6e29f` adds the `sheet` kind, PropertyGrid and Stack `animate-layout`, and ActionIcon `toggled` and `tooltip`.
Catalogue 1.4.0 snapshot `32f852d2b157035f` adds the `calendar` kind.
Catalogue 1.4.0 snapshot `bcbe38d18d3993d0` adds the `command-ribbon` kind.
Catalogue 1.4.0 snapshot `762e8e6453dbd752` adds `tracks-view` track `id` and `height` fields and the `header` prop.
Catalogue 1.4.0 snapshot `b411d15c62b969b8` adds `trackId` (the track key) to the `tracks-view` clip-change payload.
Catalogue 1.4.0 snapshot `3a0a8d98c2582322` adds the `ease-in`, `ease-out` and `ease-in-out` interpolations to curve and dope sheet keys.
