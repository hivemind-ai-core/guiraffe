# Data-driven UI

Documents compile once into `Template`: widget schemas, slot/event IDs, item paths, and literal size/color tokens are resolved before rendering. Templates are `Send + Sync`. `DataUi` instantiates that template inside any static GPUI view. Initialize `guiraffe_components` first and use a GPUI Controls `Root` when the document includes popup controls such as `select`.

## Lists and conditionals

```json
{"version":1,"root":{"kind":"stack","each":"todos","key":"id","when":"visible","children":[
  {"kind":"text","props":{"text":"@item.title","size":"sm","color":"blue"}}
]}}
```

`each` repeats the node's child template for each list item. Populate `todos` with `Value::List(Arc<[Value]>)`; list records can be built with `Value::item_from_json` or `Value::Record`. `@item.title` resolves a precompiled field path in the nearest item, and `@item` refers to the entire scalar item. Both prop expressions and explicit bindings support item references. Ordinary `@title` prop expressions and `bind: {"text":"title"}` resolve to global slots.

`key` is an item field path, defaulting to `id`. String, finite numeric, and boolean keys preserve entities across replacement and reordering. Missing or duplicate keys get distinct positional identities; they never alias another row's input state. Field edits preserve that positional state. A surviving duplicate recovers keyed identity when its conflicting owner disappears. `when` names a boolean slot; false, null, and non-boolean values hide the node and its children. Patches while hidden are read when it becomes visible again.

`Patch::Splice(slot, start..end, Arc<[Value]>)` replaces an exclusive range. `PatchBuf::splice` accepts both `SlotId` and typed slots. Ranges are checked before mutation: reversed/out-of-bounds ranges and non-list or unknown slots are inert. Sequential splices use the result of preceding patches. Identical replacements and empty insertions do not bump versions or notify.

Slot lists use a persistent vector internally, sharing unaffected tree nodes. Equal-length replacements edit their changed paths directly. Keyed reconciliation creates, updates, and removes only affected item states; a whole-list `Set` performs full keyed reconciliation. The public `list_len`/`list_get` accessors cover both incoming shared slices and persistent sequences.

Repeats use natural layout for multiline text, multi-control rows, and templates of unknown height. Short lists occupy their content height, with no eight-row minimum. Large lists virtualize only when the resolved widget factory declares a bounded-height leaf contract: native text inputs/selects, or text explicitly marked `"single-line": true`. That text option truncates within a fixed GUIraffe control-height line. Custom widget factories default to natural layout even if they override a builtin kind name; an explicit `uniform_rows_if` factory contract can opt in. Qualified lists use an eight-row maximum viewport.

Each item owns its own view, created lazily when rendered. A data-only replacement notifies that item rather than rebuilding the list, leaving other row render counts unchanged on gpui-fast. Insertion/reordering preserves keyed identities but may re-layout rows whose bounds move. Upstream GPUI can re-render ancestors on a child notification, as documented in the performance conventions. Item event addresses use live order statistics rather than scanning/re-indexing the entire list after each splice.

## Events and two-way controls

Builtins `text-input`, `checkbox`, `switch`, and `select` wrap GPUI Controls. Their schema accepts `value`, `label`, `options`, `disabled`, and `placeholder`; `select` uses a list of string options. Style and control sizes inherit GUIraffe's theme/context.

```json
{"kind":"text-input","props":{"value":"Designer default","placeholder":"Title"},"bind":{"value":"title"},"on":{"change":"rename"}}
```

Binding `value` implies a change event. An explicit `on.change` overrides its name; otherwise a global slot generates `<slot>.change`, while `@item.done` generates `item.done.change`. Unqualified slot names intentionally address shared/global view data; use `bind: {"value":"@item.done"}` for independent row state.

All row events, including button clicks, arrive through `Logic::on_payload` with scoped context. `payload.item()` exposes the live key, index, stable host identity, source slot, repeat node and optional parent context. The address is resolved **when the callback fires**, so a cached delete handler or input still targets its item after reorder. `payload.input()` reads the compact underlying `Text`, `Bool`, `Selection` or legacy `Value` payload.

For example, a todo row can contain:

```json
{"kind":"stack","children":[
  {"kind":"text-input","bind":{"value":"@item.title"}},
  {"kind":"checkbox","props":{"label":"Done"},"bind":{"value":"@item.done"}},
  {"kind":"button","props":{"label":"Delete"},"on":{"click":"delete"}}
]}
```

The host applies an item-relative edit optimistically as a guarded field patch. Logic can inspect `payload.item_change()` and emit `out.item(change.with_value(normalised))`, or call `out.veto()`. A scoped delete handler can emit `out.splice(item.slot, item.index..item.index + 1, Arc::from([]))`. Patches emitted by one response are processed in order: later item patches rebase their stable identities after preceding structural edits, and patches for deleted items are ignored. The pure `SlotTable` form of `Patch::SetItem` checks the captured index/key; the host adds live identity resolution.

**Veto wins over normalization:** it discards every patch returned by that event and restores the previous target value. The optimistic value is available to logic synchronously; normalization or rollback completes before deferred control synchronization and the next ordinary frame. Rejection repairs the native editor too, even if its final prop equals the old prop. Global input changes follow the same policy.

Existing `Logic::on_event(event, &Value, out)` implementations keep working through the default payload hook; override `on_payload` to use scoped row context. Custom widgets using `NodeView::handler` get the same implied write-back for declared value/change bindings. Generation stamps reject callbacks from replaced documents, and removed row callbacks become inert.

The event path never writes the `DataUi` entity to deliver an event or refresh a view. Model observers coalesce native control synchronization after a patch transaction, and views receive pure `cx.notify(entity_id)` notifications. Native controls are not updated from a parent's render. `DataUi::host_renders()` counts real host Render invocations for tests/tools, replacing the former fallback-refresh counter.

## Typed logic bindings

`Bindings::new(names)` resolves names during `Logic::bind`:

```rust,ignore
let mut b = Bindings::new(names);
let title: Slot<SharedString> = b.slot("title");
let save: Event = b.event("save");
title.set(out, "Saved");
if save.matches(event) { /* handle event */ }
for diagnostic in b.diagnostics() { /* report unresolved names */ }
```

Typed slots support checked reads via `get(&SlotTable)` and typed writes via `set`. Missing names produce diagnostics and inert IDs, so logic does not juggle `Option<SlotId>`. Rebind after hot reload, as with existing raw IDs. `DataUiHandle` supplies external patches without writing the host entity.

Literal size/color tokens compile into `Prop::Size` and `Prop::Color`; invalid tokens are diagnosed. Global style slots parse their tokens when values change, not during rendering. Item-relative size/color expressions are currently unsupported; use compiled literals or global style slots.

Run `cargo run -p guiraffe-ui --example bindings` for all four native inputs, list splices, conditional visibility, and typed logic. Tests run on both backends; render-count assertions branch on `gpui_kit::GPUI_FAST`.
