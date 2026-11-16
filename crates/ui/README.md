# Data-driven UI

Documents compile once into `Template`: widget schemas, slot/event IDs, item paths, and literal size/color tokens are resolved before rendering. Templates are `Send + Sync`. `DataUi` instantiates that template inside any static GPUI view. Initialize `guiraffe_components` first and use a GPUI Controls `Root` when the document includes popup controls such as `select`.

## Lists and conditionals

```json
{"version":1,"root":{"kind":"stack","each":"todos","key":"id","when":"visible","children":[
  {"kind":"text","props":{"text":"@item.title","size":"sm","color":"blue"}}
]}}
```

`each` repeats the node's child template for each list item. Populate `todos` with `Value::List(Arc<[Value]>)`; list records can be built with `Value::item_from_json` or `Value::Record`. `@item.title` resolves a precompiled field path in the nearest item, and `@item` refers to the entire scalar item. Both prop expressions and explicit bindings support item references. Ordinary `@title` prop expressions and `bind: {"text":"title"}` resolve to global slots.

`key` is an item field path, defaulting to `id`. String, finite numeric, and boolean keys preserve entities across replacement and reordering. Missing or duplicate keys get distinct ephemeral identities; they never alias another row's input state. `when` names a boolean slot; false, null, and non-boolean values hide the node and its children. Patches while hidden are read when it becomes visible again.

`Patch::Splice(slot, start..end, Arc<[Value]>)` replaces an exclusive range. `PatchBuf::splice` accepts both `SlotId` and typed slots. Ranges are checked before mutation: reversed/out-of-bounds ranges and non-list or unknown slots are inert. Sequential splices use the result of preceding patches. Identical replacements and empty insertions do not bump versions or notify.

Slot lists use a persistent vector internally, sharing unaffected tree nodes. Equal-length replacements edit their changed paths directly. Keyed reconciliation creates, updates, and removes only affected item states; a whole-list `Set` performs full keyed reconciliation. The public `list_len`/`list_get` accessors cover both incoming shared slices and persistent sequences.

Repeats use an eight-row scrolling viewport with equal row heights from `Size::Md.control_height()`. Each item owns a cached GPUI entity; replacing one item leaves other rows' render counts unchanged on gpui-fast. Insertion/reordering preserves entity identity, but GPUI may re-render rows whose absolute layout bounds move. Variable-height row measurement is a follow-up; design repeat templates to fit one control-height row. Upstream GPUI can also re-render ancestors on child notifications, as documented in the performance conventions.

## Events and two-way controls

Builtins `text-input`, `checkbox`, `switch`, and `select` wrap GPUI Controls. Their schema accepts `value`, `label`, `options`, `disabled`, and `placeholder`; `select` uses a list of string options. Style and control sizes inherit GUIraffe's theme/context.

```json
{"kind":"text-input","props":{"value":"Designer default","placeholder":"Title"},"bind":{"value":"title"},"on":{"change":"rename"}}
```

Binding `value` to a global slot implies a change event. An explicit `on.change` overrides its name; otherwise the compiler generates `<slot>.change`, such as `title.change`. The host writes the proposed value optimistically **before** calling logic. Override `Logic::on_payload` to receive `Payload::Text`, `Bool`, or `Selection`; emit a replacement patch to normalize the value, or call `out.veto()` to restore the previous value. Rejection also repairs the native input's state, even when the final prop equals its previous value.

Existing `Logic::on_event(event, &Value, out)` implementations keep working: the default payload hook converts compact input payloads into a cheap shared `Value`. Buttons and existing custom widgets retain their Value-based handlers. Generation stamps reject callbacks from replaced documents, and removed row callbacks become inert. The event path updates shared slot data and notifies affected entities with `cx.notify(entity_id)`; it never enters `DataUi::update` to deliver an event or refresh a view.

Two-way targets are global slots. Item-relative expressions are reads; editing list fields should be handled by application logic with a splice patch.

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
