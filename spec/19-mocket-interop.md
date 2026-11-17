# Mocket ↔ GUIraffe interop: make Mocket designs run with real behaviour

**Audience:** Mocket developers (hand-off spec) and the GUIraffe `mocket`, `ui` and `cli` crates.
**Status:** proposal, v0.1 (2026-11). Field names are Mocket's own (schema 32, `src/renderer/src/model/types.ts`,
`model/dataLayerTypes.ts`, `charts/v2.ts`) unless a section says it is **new**.

The goal: a designer builds a screen in Mocket, marks *what it does* ("this button emits `todo.add`")
and *what data it shows* ("this text is `user.name`, this list repeats `todos`"). GUIraffe then runs that file
with real Rust logic. No code goes into the design file, and changes stay small and additive to Mocket's
existing interaction and data-layer model.

---

## 1. Status today

`guiraffe-mocket` (`crates/mocket/src/lib.rs`) imports one screen (`screens[].root` → `nodes` map) into the
GUIraffe UI IR (`guiraffe_ui::Document`). It also runs as a `mocket.plugin/1` plugin, writing
`<screen>.guiraffe.json` plus a coverage report. `guiraffe preview <file.mocket.json>` hot-reloads the file on save.

| Mocket `type` | GUIraffe kind | Coverage | Notes |
|---|---|---|---|
| `frame`, `box`, `stack` | `stack` | mapped | `layout.gap` snapped to a Size token |
| `row` | `group` | mapped | |
| `grid` | `grid` | mapped | `layout.columns` string → column count |
| `heading` | `title` | mapped | `props.level` → `order`; text bound to slot `<id>.text` |
| `text` | `text` | mapped | `tone: muted` → `dimmed`; slot `<id>.text` |
| `button` | `button` | mapped | `props.label`; emits `<id>.click` |
| `spacer` | `spacer` | mapped | |
| `card` | `card` | partial | title and subtitle only |
| `kpi` | `card` | partial | label, value and delta; no sparkline |
| `last-updated` | `text` | partial | |
| `progress` | `ring` | partial | |
| everything else (~70 types: `input`, `select`, `checkbox`, `toggle`, `table`, `list`, `tabs`, `modal`, charts…) | `mocket:<type>` placeholder | unsupported | |

GUIraffe's runtime can already do more than the importer uses. It supports `bind` (prop → slot) and `on`
(widget event → event name), `when` (a conditional slot), `each` + `key` (keyed repeats with `@item.path` scope),
two-way native inputs (`text-input`, `checkbox`, `switch`, `select`, which emit `change`, with logic able to
`veto()` or normalise), and patches from `Logic::on_payload`.

**Gaps (both sides):**
- The importer ignores Mocket's `interactions`, `links`, `propBindings`, `dataLayer` (variables/fixtures),
  `screens[].states`, `variants`, `overlays`, and most `layout` (padding, align, justify, width/height).
- Its gap scale is Mantine's (10/12/16/20/32). Mocket uses `SPACING_SCALE` (0…128) and `spacing.*` token slots.
- Mocket can't express **app events** (only built-in actions such as `navigate` or `set-prop`), **structured data**
  (variables are scalars only), **repeats**, or **native component placeholders**.
- Ids are implicit: today every text gets a slot and every button gets an event, whether or not the app cares.

## 2. Principles

1. **Mocket owns layout and look. Rust owns behaviour.** They meet only at **stable string ids**
   (event names and data paths). Designers never write code, and Mocket never evaluates anything (this
   matches its rule that `Condition` text leaves "must never be executed").
2. **Extend, don't fork.** App events are a new `InteractionAction` kind. Data binding is a new `PropBinding`
   kind plus richer `DataVariable` types. Sample data stays in `DataFixture`. Mocket's existing behaviour
   annotations (`Annotation.behaviour`, category `behaviour`/`data`) can link to these through `refs`.
3. **Everything is optional and backward compatible.** A document with none of this still imports
   exactly as it does today. Unknown fields are kept by Mocket and ignored by GUIraffe, with a warning.
4. **Design-time faithful.** Mocket's preview runner and `guiraffe preview` render the same sample data,
   so a bound design looks real in both.
5. **Versioned.** The interop block declares its own version (`appContract.version`), independent of Mocket's
   `schema` and GUIraffe's IR `FORMAT_VERSION`.
6. **Compile-time safety in the app.** Codegen turns the contract into typed Rust. If the design renames an
   event or a path, the app stops compiling instead of silently breaking.

## 3. Stable ids and naming

- Mocket node `id` (human, unique per document, already used as the HTML `id`) is the **node id** in GUIraffe
  (`Node.id`). `uid` is never shown to the app.
- **Event ids** and **data paths** are separate names. They are *not* derived from node ids, so a designer can
  rename or move a button without breaking the app.
  - Event id: `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`, dot-namespaced by feature, e.g. `todo.add`,
    `todo.toggle`, `settings.theme_changed`. At most 64 characters.
  - Data path: the same segment grammar, e.g. `user.name`, `todos`, `filter.query`. Inside a repeat,
    `@item` and `@item.<path>` (GUIraffe's existing syntax) are also allowed, and `@index`.
- Reserved prefixes: `guiraffe.*` and `mocket.*` (built-ins such as `mocket.navigate`).
- Mocket should validate names on input, flag duplicates whose payload shapes conflict, and offer
  autocomplete from the declared contract (§5.4). An id must not change when the label changes.
- `locks.binding` / `locks.navigation` (existing `LockAspect`s) protect these from accidental edits once
  the app depends on them. Recommend locking `binding` when codegen has run.

## 4. Event annotations

### 4.1 Model: a new `InteractionAction` kind

Interactions already say "component `from` on `event` does `action`". We add one action kind:

```ts
// model/types.ts — NEW member of InteractionAction
| { kind: 'emit'; event: string; payload?: Record<string, PayloadSource> }

// NEW
type PayloadSource =
  | { kind: 'literal'; value: DataScalar }
  | { kind: 'event'; prop: 'value' | 'checked' | 'selected' }   // same as VariableSource.event
  | { kind: 'path'; path: string }                              // a data path or @item.<path>, read at fire time
```

`InteractionEvent` grows as follows (all optional for components to support):

| Event | Fires on | GUIraffe widget event |
|---|---|---|
| `click` (existing) | button, link, card, list item… | `click` |
| `change` (existing) | input, textarea, select, checkbox, toggle, radio-group, tabs | `change` (two-way, see §5.3) |
| `submit` (existing) | form frame / Enter in an input | `submit` |
| `dismiss` (existing) | layer root | `dismiss` |
| `select` **new** | table row, list item, tabs, dropdown-menu item | `select` (payload: item key) |
| `key` **new** | any focusable; needs `keys: ["Enter","Escape","mod+s"]` on the interaction | `key` |
| `hover` **new, optional** | anything; design-time only in Mocket preview | `hover` (enter/leave, bool) |

Rules:
- Several interactions per `(from, event)` are allowed and run in array order. Mocket's preview runner
  runs built-in actions and **logs** `emit` actions (it shows a toast/console "emitted `todo.add` {title:"…"}").
- `guard` / `condition` keep their meaning. GUIraffe evaluates a machine-checkable `guard` before emitting
  and drops `text` leaves (intent only).
- `emit` inside a repeat automatically carries the item context. GUIraffe passes `ItemContext`
  (key, index, key path), so payloads need not repeat `@item.id`.
- Built-in actions map to GUIraffe too: `navigate`/`back` → router (§8); `open-layer`/`close-layer` → overlays;
  `set-variable` → a slot patch done by the host with no Rust needed; `set-state` → screen state slot;
  `set-prop`, `set-filter`, `reset-form` → host-side patches.

### 4.2 Component-level events

A reusable component (a saved `UserComponent`/`NodeSpec` with a `SpecificationFragment`) carries its
`interactions`. When a component's events should be *re-targetable* by the instance, the fragment declares
**outputs** (**new**, optional):

```jsonc
"specification": { "version": 1, "outputs": [{ "name": "remove", "payload": { "id": "string" } }], ... }
```

Inside the component, `emit` uses `event: "$remove"`. An instance maps it in `NodeSpec.props.$events`
(`{ "remove": "todo.remove" }`). Unmapped outputs warn and are dropped.

### 4.3 How it looks in Mocket

- **Interactions panel** (existing): add an action type "App event". It has an event-id field with autocomplete
  from the contract, plus a payload table (key → literal / this field's value / data path).
- **Canvas:** an app-event interaction draws a small "⚡ todo.add" chip on the component, like `NavLink` edges.
- **Annotations:** a `behaviour` annotation may `refs: [{ kind: 'interaction', uid }]` to an emit
  interaction, so the prose ("adds the todo and clears the field") travels with the machine-readable wiring.
  The existing `prototype.ts` suggestion flow can propose `emit` interactions from behaviour notes.
- **Spec export** (`spec.json` / agent pack): list app events in the transitions table.

## 5. Data binding

### 5.1 Model: richer variables and a `path` binding

Mocket's `dataLayer` version 2 has `variables: DataVariable[]` (scalar) and `fixtures: DataFixture[]`.
Today `MockNode.propBindings` supports kinds `variable`, `view` and `table`. We add:

```ts
// dataLayerTypes.ts — NEW
type DataType =
  | 'string' | 'number' | 'boolean' | 'date'                     // existing scalar types
  | { list: DataType; key?: string }                              // e.g. todos: list of record, key "id"
  | { record: Record<string, DataType | { type: DataType; nullable?: boolean }> }
  | { enum: string[] }

// DataVariable.type widens from the scalar union to DataType; DataScalar widens to JSON for defaults.

// PropBinding — NEW member
| { kind: 'path'; path: string; format?: BindingFormat; pipe?: Pipe[]; twoWay?: boolean }

type Pipe =                                  // closed set, data only, never code
  | { op: 'default'; value: DataScalar }
  | { op: 'count' }                          // list → number
  | { op: 'not' } | { op: 'empty' } | { op: 'filled' }
  | { op: 'equals'; value: DataScalar }
  | { op: 'template'; text: string }         // "{count} items left", fields from the same scope only
  | { op: 'truncate'; length: number }
```

- `propBindings` keys are prop names: `{ "text": { "kind": "path", "path": "user.name" } }`.
- `format` reuses `BindingFormat` (`number`, `currency`, `date`, `unit`). GUIraffe implements the same
  formatters in Rust (`plain|compact|currency|percent`; `iso|month|month-year|day-month|weekday`).
- Pipes are applied left to right. Anything more complex belongs in Rust: expose a derived path
  (`todos_left_label`) instead.
- Paths resolve against the screen's data root. The first segment must be a declared variable `id`.

### 5.2 Conditionals and repeats (new node fields)

```ts
// MockNode — NEW optional fields
when?: { path: string; pipe?: Pipe[] }       // render only when the result is truthy
repeat?: { path: string; key: string; empty?: Uid; loading?: Uid; limit?: number }
```

- `repeat`: the node is the **item template**. Mocket renders it once per fixture row (it currently draws one copy
  plus "×N" in edit mode, and all rows in preview). Inside it, bindings may use `@item`, `@item.<field>` and `@index`.
  `key` is required and must name a field of the list's record type. GUIraffe's keyed repeats rely on it for
  identity, focus, and state that survives reorder.
- `empty` / `loading`: uids of sibling nodes shown instead when the list is empty or the path is
  `null`/pending. Alternatively, reuse **screen states**: a `ScreenState` with `id: "empty"`/`"loading"`
  is selected by GUIraffe when the bound list is empty/pending (see open question 3).
- Maps to GUIraffe IR: `Node.when = <slot>`, `Node.each = <slot>`, `Node.key = <field>`, bindings to
  `@item.<field>`. Pipes and formatters compile to derived slots computed by the host.

### 5.3 Two-way inputs

`twoWay: true` on an input's `value`/`checked`/`selected` binding means the widget writes the path optimistically
on `change`, and the app receives the change (GUIraffe `on_payload`). The app can `veto()` (revert) or patch a
normalised value. Inside a repeat, the change carries `ItemChange { item, path, value }`. Mocket's preview runner
already does the equivalent with `set-variable` + `VariableSource { kind: 'event' }`; `twoWay` is shorthand for it.

### 5.4 Contract declaration and sample data

A document-level **app contract** (new, optional) collects everything the app must provide or handle:

```ts
// MockDocument — NEW
appContract?: {
  version: 1
  /** Rust module/feature name for codegen, e.g. "todos" */
  name: string
  /** event id → payload field types (inferred from emit actions if omitted; declared wins) */
  events: Record<string, { payload?: Record<string, DataType>; doc?: string }>
  /** data roots are dataLayer.variables; nothing duplicated here */
  natives?: Record<string, { props?: Record<string, DataType>; doc?: string }>   // §6.2
}
```

- **Data schema** = `dataLayer.variables` (with the widened `DataType`). One set per document. Screens use the
  subset they reference, which codegen reports per screen.
- **Sample data** = `dataLayer.fixtures` (`DataFixture.values`, widened to JSON). Mocket's preview and
  `guiraffe preview --fixture <id>` both render with the chosen fixture. The default is the first fixture,
  and failing that, the variable `default`s. Fixtures named `empty` / `loading` / `error` are conventional
  and pair with `screens[].states`. `Scenario.dataFixture` already points at fixtures, which lets scenarios
  become GUIraffe interaction tests (§9).
- Validation (in Mocket, and again in GUIraffe at import): every `path` resolves to a declared variable/field;
  `repeat.key` exists; `@item` is used only under a `repeat`; emit payload sources type-check against
  `events[*].payload`; event ids are well-formed. Report these through Mocket's existing
  `bindingProblems` / `specWarnings` channel.

## 6. Components, slots, variants, native placeholders

### 6.1 Mapping
- **Library types** map through the single `MAPPING` table in `guiraffe-mocket`. That table is also the
  conformance test list. GUIraffe will grow it for inputs, lists, tables, tabs, modals and badges first.
- **User components** (`NodeSpec` subtrees) are inlined at import. If `NodeSpec.name` is set and
  `appContract` lists it as a component (**new**: `appContract.components?: string[]`), GUIraffe codegen
  emits a reusable `fn <name>(..) -> Node` builder instead of duplicating the subtree.
- **Slots:** Mocket has no component slots today. Proposal: a child `frame` with `props.slot: "<name>"`
  inside a saved component marks an insertion point. An instance's children with matching `props.slotFill`
  replace it. Optional; skip until components gain instances (open question 4).
- **Screen states** (`screens[].states`, prop overrides by uid) → GUIraffe applies the overrides when the
  state slot `screen.<screen-id>.state` equals the state `id`. **Variants** (`screens[].variants`) →
  GUIraffe `viewport` breakpoints: the base screen is desktop, and overrides and `hidden` apply at the
  variant width and below.
- **Overlays** (`screens[].overlays`) → GUIraffe overlay layers. `backdrop` maps to the modal scrim token.

### 6.2 Native component placeholders
Designers need a box that the app fills with something Mocket can't draw (a code viewer, a live chart, a map, a canvas).

```jsonc
{ "type": "native", "id": "editor", "props": { "native": "code_viewer", "label": "Code viewer", "aspect": "16:9" },
  "propBindings": { "source": { "kind": "path", "path": "file.text" } } }
```

- **New** library type `native`. Mocket draws it as a labelled hatched box (or the `previewAsset` image) at its
  layout size.
- `props.native` names an entry in `appContract.natives`. GUIraffe looks it up in its widget `Registry`, where
  the app registers `"native:code_viewer"`. If it is missing, GUIraffe shows the placeholder and reports
  `unsupported`.
- Bound props and interactions work as on any node, so the native component receives data and emits events.

## 7. Styling and tokens

- **No raw px where a token exists.** Mocket gaps and padding already come from `SPACING_SCALE` and the
  `spacing.*` slots. GUIraffe maps them to its `Size` scale by a fixed table (0→none, 4→xs, 8→xs,
  12→sm, 16→md, 24→lg, 32→xl, 48+→xl plus a warning). This replaces today's Mantine-pixel snapping.
  Layout gap/padding become **container recipes** (spec/14): `stack`/`row`/`grid` carry gap/padding, and
  widgets never get margins.
- **Colour:** Mocket slots `color.{primary,grey,danger,warning,success}.{050…900}` map to the GUIraffe theme's
  primary colour / `ColorName` shades. `color.canvas|surface|surface-raised|inverse` map to `StyleContext.surface`
  (body / raised / sunken / accent). Node `props.surface` (e.g. `"none"`, `"raised"`) → `surface`.
- **Radius** `radius.sm|md|lg|full` and `theme.radius` (`square|small|large|round`) → theme radius.
  **Type** `fontSize.*`, `fontWeight.*` → `Size` / weight tokens. `heading.level` → `Title.order`.
- **Theme:** `theme.projectTokens` (DTCG 2025.10 or flat, with `mappings` slot → JSON Pointer) is the
  source for a generated GUIraffe theme (`guiraffe import --theme`). `theme.palette`/`scheme` pick the base.
- **Escape hatches:** a literal colour or px on a node is imported as an explicit override and listed in the
  coverage report as "off-token", so designers can see and fix drift.

## 8. Navigation

- `links: NavLink[]` (`from` node → `to` screen) and `navigate`/`back` actions → guiraffe-router.
- Each imported screen becomes a route. The path is `/<screen-id>`, or **new** optional `Screen.route`
  (e.g. `"/projects/:id"`). Params come from the emitting interaction's payload (`{ id: @item.id }`).
- `NavLink.label` stays descriptive. A link with a guard waits for the guard. A link that should depend on
  app logic ("on success") should instead be an `emit`, with Rust navigating.
- Codegen emits a `#[derive(Route)] enum <Name>Route` with one variant per routed screen.

## 9. Versioning, validation, compatibility

- `appContract.version` starts at 1. GUIraffe accepts versions ≤ its maximum. A newer version is a hard error
  with a clear message, matching `Document::from_json`.
- Mocket adds the new fields in one schema bump (33), with a migration that is a no-op for old files.
  GUIraffe reads `schema ≥ 30` and ignores unknown fields (they are reported, not fatal).
- **Additive-only within a version.** Renaming or removing a field, or changing a meaning, needs `version: 2`.
- Event ids and paths are the public API. Mocket should warn when an edit renames or removes one that
  appeared in the last export (it already has `documentDiff` and `specSnapshots`).

JSON Schema sketch (Mocket ships `appContract.schema.json` beside `charts/v2.schema.json`):

```json
{ "$id": "https://mocket.dev/schema/app-contract/1",
  "type": "object", "required": ["version", "name", "events"],
  "properties": {
    "version": { "const": 1 },
    "name": { "type": "string", "pattern": "^[a-z][a-z0-9_]*$" },
    "events": { "type": "object",
      "propertyNames": { "pattern": "^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)*$" },
      "additionalProperties": { "type": "object", "properties": {
        "payload": { "type": "object", "additionalProperties": { "$ref": "#/$defs/dataType" } },
        "doc": { "type": "string" } } } },
    "natives": { "type": "object", "additionalProperties": { "type": "object" } },
    "components": { "type": "array", "items": { "type": "string" } } },
  "$defs": { "dataType": { "oneOf": [
    { "enum": ["string", "number", "boolean", "date"] },
    { "type": "object", "required": ["list"], "properties": { "list": { "$ref": "#/$defs/dataType" }, "key": { "type": "string" } } },
    { "type": "object", "required": ["record"], "properties": { "record": { "type": "object" } } },
    { "type": "object", "required": ["enum"], "properties": { "enum": { "type": "array", "items": { "type": "string" } } } } ] } } }
```

## 10. Round trip and tooling (GUIraffe side)

- `guiraffe import app.mocket.json [--screen s] [--out ui/]` writes `<screen>.guiraffe.json` (IR) plus
  `coverage.json`/console table: per node mapped/partial/unsupported, off-token values, and unresolved
  paths/events.
- `guiraffe preview app.mocket.json [--fixture f] [--state s] [--variant mobile]` hot-reloads on save (already
  watching), renders fixture data, and logs emitted events with payloads. `--logic <bin>` attaches the real app logic.
- `guiraffe codegen app.mocket.json` (in `guiraffe-codegen`) emits `ui/<name>_contract.rs`:
  - `enum TodosEvent { Add { title: String }, Toggle { key: String, done: bool }, … }` plus `fn decode(EventId,
    &Payload) -> Option<TodosEvent>`;
  - `struct TodosData { … }` typed from `dataLayer.variables`, plus `fn patch(&self, prev, out: &mut PatchBuf)`
    that diffs and emits minimal patches (lists by `key`);
  - `enum TodosNative`, `enum TodosRoute`.
  The design file is the input, so renaming an event in Mocket breaks `match` arms at compile time.
- The Mocket plugin (`mocket.plugin/1`, trigger `export.after`/`save.after`) runs the same import and returns
  the coverage summary as its `message`. A later **implementation target** `guiraffe-rust` (alongside
  `mantine-react`, `react-tailwind` in `IMPLEMENTATION_TARGETS`) could make this a first-class export.
- **Scenarios → tests:** each `Scenario` with `dataFixture` and `steps` referencing interactions generates a
  GUIraffe `TestAppContext` test skeleton: it loads the fixture, fires the events, and leaves `assert` TODOs
  from `outcomes[].text`.
- Mocket never imports back from GUIraffe. Round trip means re-importing after every Mocket save, which is
  idempotent and deterministic (stable ordering, so diffs stay small).

## 11. Worked example: todo screen

Mocket document (excerpt, new fields marked `// new`):

```jsonc
{ "version": 1, "schema": 33, "title": "Todos",
  "appContract": { "version": 1, "name": "todos",                                   // new
    "events": { "todo.add": { "payload": { "title": "string" } },
                "todo.toggle": { "payload": { "done": "boolean" } },
                "todo.remove": {}, "todo.clear_done": {} } },
  "dataLayer": { "version": 2, "views": [],
    "variables": [
      { "uid": "v1", "id": "draft", "name": "Draft", "type": "string", "nullable": false, "default": "" },
      { "uid": "v2", "id": "todos", "name": "Todos", "nullable": false, "default": [],
        "type": { "list": { "record": { "id": "string", "title": "string", "done": "boolean" } }, "key": "id" } } ], // new type
    "fixtures": [
      { "uid": "f1", "id": "sample", "name": "Sample", "datasets": {},
        "values": { "v1": "", "v2": [ { "id": "a", "title": "Buy milk", "done": false },
                                      { "id": "b", "title": "Ship 0.1", "done": true } ] } },
      { "uid": "f2", "id": "empty", "name": "Empty", "datasets": {}, "values": { "v1": "", "v2": [] } } ] },
  "screens": [{ "uid": "s1", "id": "todos", "name": "Todos", "root": "n-root", "width": 480, "height": 640,
                "position": { "x": 0, "y": 0 } }],
  "nodes": {
    "n-root": { "uid": "n-root", "id": "root", "type": "stack", "parent": null, "props": {},
      "layout": { "display": "stack", "gap": 16, "padding": 24 }, "children": ["n-h", "n-add", "n-list", "n-none", "n-left"] },
    "n-h":   { "uid": "n-h", "id": "title", "type": "heading", "parent": "n-root", "props": { "text": "Todos", "level": 2 }, "layout": {}, "children": [] },
    "n-add": { "uid": "n-add", "id": "new-todo", "type": "input", "parent": "n-root",
      "props": { "placeholder": "What needs doing?" }, "layout": { "width": "fill" }, "children": [],
      "propBindings": { "value": { "kind": "path", "path": "draft", "twoWay": true } } },        // new kind
    "n-list": { "uid": "n-list", "id": "todo-row", "type": "row", "parent": "n-root", "props": {},
      "layout": { "display": "row", "gap": 8, "align": "center" }, "children": ["n-chk", "n-del"],
      "repeat": { "path": "todos", "key": "id", "empty": "n-none" } },                           // new
    "n-chk": { "uid": "n-chk", "id": "todo-done", "type": "checkbox", "parent": "n-list", "props": {}, "layout": { "width": "fill" }, "children": [],
      "propBindings": { "label": { "kind": "path", "path": "@item.title" },
                        "checked": { "kind": "path", "path": "@item.done" } } },
    "n-del": { "uid": "n-del", "id": "todo-delete", "type": "button", "parent": "n-list", "props": { "label": "Delete", "variant": "subtle" }, "layout": {}, "children": [] },
    "n-none": { "uid": "n-none", "id": "no-todos", "type": "empty-state", "parent": "n-root", "props": { "title": "Nothing to do" }, "layout": {}, "children": [] },
    "n-left": { "uid": "n-left", "id": "left", "type": "text", "parent": "n-root", "props": { "tone": "muted" }, "layout": {}, "children": [],
      "propBindings": { "text": { "kind": "path", "path": "todos",
        "pipe": [{ "op": "count" }, { "op": "template", "text": "{value} items" }] } } } },
  "interactions": [
    { "uid": "i1", "from": "n-add", "event": "submit",
      "guard": { "target": "n-add", "prop": "value", "test": "filled" },
      "action": { "kind": "emit", "event": "todo.add", "payload": { "title": { "kind": "path", "path": "draft" } } } },
    { "uid": "i2", "from": "n-chk", "event": "change",
      "action": { "kind": "emit", "event": "todo.toggle", "payload": { "done": { "kind": "event", "prop": "checked" } } } },
    { "uid": "i3", "from": "n-del", "event": "click", "action": { "kind": "emit", "event": "todo.remove" } } ],
  "annotations": [{ "uid": "a1", "screen": "s1", "kind": "note", "number": 1, "category": "behaviour", "target": "n-add",
    "text": "Enter adds the todo and clears the field.", "refs": [{ "kind": "interaction", "uid": "i1" }] }] }
```

Rust side, using the generated `todos_contract.rs`:

```rust
use todos_contract::{TodosData, TodosEvent, Todo};

struct Todos { data: TodosData, names: todos_contract::Names }

impl Logic for Todos {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        self.names = todos_contract::Names::resolve(names);   // typed slot/event ids
        self.data.patch_all(&self.names, out);
    }
    fn on_payload(&mut self, event: EventId, p: &Payload, out: &mut PatchBuf) {
        let prev = self.data.clone();
        match TodosEvent::decode(&self.names, event, p) {
            Some(TodosEvent::Add { title }) => {
                let title = title.trim();
                if title.is_empty() { return out.veto(); }
                self.data.todos.push(Todo { id: new_id(), title: title.into(), done: false });
                self.data.draft.clear();
            }
            Some(TodosEvent::Toggle { key, done }) => self.data.set_done(&key, done),
            Some(TodosEvent::Remove { key }) => self.data.todos.retain(|t| t.id != key),
            Some(TodosEvent::ClearDone) => self.data.todos.retain(|t| !t.done),
            None => {}
        }
        self.data.patch(&prev, &self.names, out);   // minimal patches; list diff by `id`
    }
}
```

`key` arrives automatically because `todo.toggle` and `todo.remove` fire inside the `todos` repeat.

## 12. Open questions for the Mocket developers

1. Is widening `DataVariable.type` and `DataScalar` (to lists and records) acceptable? Or would you prefer a
   separate `appContract.data` schema, keeping `dataLayer` scalar for dashboards and filters?
2. Should `repeat` be a node field (as proposed) or a wrapper library type (`"type": "repeat"`) that
   reuses the layers panel and locks more naturally?
3. Empty/loading: `repeat.empty`/`loading` node refs, or convention-named `screens[].states` (`empty`,
   `loading`) selected automatically? Supporting both is possible but adds a second way to do the same thing.
4. Do you plan component **instances** (linked copies of a `UserComponent`)? Slots and outputs (§4.2, §6.1)
   only make sense with instances; otherwise saved components stay copy-on-insert and we inline them.
5. Should the preview runner execute `emit` against a stub (log only), or allow a plugin (`mocket.plugin/1`
   `command`) to answer with fixture patches, giving live "fake backend" previews?
6. Event payload inference: is it OK to infer `appContract.events` from `emit` actions and have the panel
   write the declared form on first use? We want one source of truth that the designer can see.
7. Where should the `native` placeholder preview image live: `assets` (hash-addressed) via
   `props.previewAsset`?
8. `key` and `hover` events: do they fit your accessibility model (`docs/accessibility.md`), or should
   keyboard shortcuts be declared only via GUIraffe commands?
9. Would you take `guiraffe-rust` as an `ImplementationTarget`, or keep GUIraffe as a plugin only?
