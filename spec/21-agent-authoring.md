# Agent-first authoring: catalogue, inspectable preview, cheap iteration

Goal: an agent (or a designer) can generate a screen, run it, inspect it, fix it, and replay the
interaction — with structured feedback at every step. The showcase, DataUi and the test harness
become one workflow instead of three tools.

## 1. Authoritative component catalogue (single source of truth)
- **One definition per widget** drives the running app, the DataUi registry, the showcase entry,
  the schema and the docs. A `#[derive(Widget)]`-style descriptor (or a `WidgetSpec` impl) per
  component declares: id, crate, summary, **props** (name, type, default, accepted values/range,
  token-backed or literal, required), **slots** (name, cardinality, accepted children), **events**
  (id, payload schema), **state** (controlled/uncontrolled), **theme tokens** it reads, **a11y role**,
  and **examples** (named, minimal JSON documents).
- **Versioned output** generated at build time (`guiraffe catalog --json`, also embedded in the
  binary): `catalog.json` + JSON Schema for screen documents, semver'd; breaking changes bump the
  major; a test fails if the generated catalogue changes without a version bump or changelog line.
- **Recipes**, validated against the schema and rendered in CI: forms (with validation states),
  settings pages, dashboards, editors (code/markdown + toolbar), split-pane tools (navigator/
  inspector), data screens (grid + filters), chat, wizard.
- **Validation errors are actionable**: `{node: "/screens/settings/nodes/save-btn", property:
  "variant", got: "primery", accepted: ["filled","light","outline","subtle","default"], hint:
  "did you mean \"primary\" color? variant controls style"}`; unknown props, wrong slot children,
  missing required props, bad token names, unbound events, duplicate ids — all with node path.

For a crate-owned widget, define the descriptor in its `widget_specs.rs` and install its
native renderer from the crate's `init(cx)`. Include the descriptor in the DataUi catalogue
so build-time generation and screen validation see the same contract. Application hosts call
the widget crate's initializer before constructing `Registry::for_app(cx)`; the CLI initializes
the bundled widgets. Add a headless test that validates, compiles, and mounts the example.
Callback builders can expose named events with serializable payloads, and application-owned
entities can expose input records that the renderer turns into keyed native state.

The showcase `catalogue_drift` test compares discovered public Rust surfaces with their
catalogue kinds, props, and events. Classify an existing Rust-only API by exact owner and member
in the test's `IMPERATIVE` table and give its authoring rationale. The issue list in
`catalogue_drift/known_gaps.txt` tracks existing coverage debt. Remove its matching entries as
adapters land. A new entry in that file represents unresolved document coverage.

## 2. Inspectable, drivable preview
- Every node has a **stable node id** (document id, or `#[test_id]` on static widgets) that maps
  to the element tree and the AccessKit node.
- **Inspect API** (in-process, exposed over CLI and MCP):
  - `screenshot(node?)` → PNG (whole window or a node's bounds), plus a theme/state label;
  - `layout(node?)` → bounds, content size vs. box (overflow / clipping / text truncated flags),
    visibility, z-order, scroll offsets;
  - `a11y()` → role, name, state, value, focusable, and **focus order** (Tab sequence) as a list;
  - `style(node)` → resolved tokens (size, colors, radius) and which StyleContext supplied them;
  - `events()` → recent events with payloads (the showcase EventLog, generalised).
- **Drive API**: `click(id)`, `hover(id)`, `type(id, text)`, `key("tab"|"enter"|...)`,
  `focus(id)`, `scroll(id, dx, dy)`, `drag(id, to)`, `set_fixture(name)`, `set_theme(...)`,
  `resize(w, h)`. Each returns a structured result (what changed: events fired, focus moved,
  nodes re-rendered, new screenshot optional).
- **Checks** returning structured findings: clipped/overflowing text, controls < 24×24 hit
  target, contrast below floor (reuse `state::contrast_checks`), unlabeled interactive nodes,
  focus traps / unreachable focusables / Tab order not matching visual order, layout jumps on
  focus/hover (reuse `guiraffe_test::layout::assert_same_bounds`).
- **Replay**: a recorded interaction script (JSON) that the agent can re-run after a fix; it
  becomes a test (`guiraffe test --script settings-save.json`) with assertions on bounds,
  focus, events and screenshots.
- **Transport**: `guiraffe preview --inspect` exposes the API on a local socket; `guiraffe mcp`
  serves it as MCP tools (catalogue lookup, validate, render, inspect, drive, check, replay) so
  any coding agent can use it. Headless mode uses the existing snapshot renderer (no display).

## 3. Cheap visual iteration as the app becomes real
- **Hot reload** of screen documents (exists in `preview`) plus **fixtures** per screen:
  `loading`, `empty`, `error`, `populated` (and custom), switchable live and in tests; fixture
  data validated against the screen's data schema (spec/19 data contract).
- **Typed Rust handlers** for business logic, generated stubs from the contract (spec/19 §10):
  renaming an event in a document is a compile error; handler edits keep the preview running
  (handlers in a dylib or via `guiraffe dev` relaunch preserving the selected screen+fixture).
- **Custom Rust widgets in documents**: `#[guiraffe::widget]` registers a Rust component into the
  catalogue (schema, props, events) so documents can use it like built-ins; a `native` placeholder
  (spec/19 §6.2) for one-off widgets the app fills in.
- **Mocket path**: `guiraffe import`/`preview` on a Mocket file shows the same screen with a
  **coverage overlay**: mapped / partial / needs-implementation per node (from the importer's
  coverage report), clickable to show what is missing.

## Delivery plan (owners)
1. Widget descriptors + catalogue generator + schema + validation errors — **codex**.
2. Inspect/drive API, checks, replay, CLI `--inspect`, MCP server — **kilo** (with the test harness).
3. Fixtures + coverage overlay + custom widget registration — codex after (1).
4. Recipes library (forms, settings, dashboard, editor, split-pane, data, chat, wizard) rendered
   in CI and in the showcase — whoever frees up first.
Gates as usual (both backends, clippy, fmt, goldens viewed); every check has a test that fails
when the defect is introduced.
