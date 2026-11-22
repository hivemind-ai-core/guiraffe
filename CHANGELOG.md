# Changelog

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
