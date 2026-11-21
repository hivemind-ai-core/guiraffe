# Static and dynamic UI

GUIraffe supports both, with one widget vocabulary:

| | Static (default) | Dynamic (data-driven) |
|---|---|---|
| Built from | Rust code (builders, `RenderOnce`, entities) | UI document (JSON IR, typically authored in **Mocket**) |
| Dispatch | static: generics, monomorphised, inlined | dynamic: `dyn Widget` from a registry |
| Use | the app shell, hot paths, known screens | user- or design-defined screens, plugins, hot-reload, rapid prototyping |
| Bridge | `guiraffe-codegen` turns a document into static Rust when it should be compiled in | |

## The host component: dynamic UI anywhere inside a static UI
`DataUi` is an ordinary Control. Initialize the component crates the document uses before building
the application registry. The CLI initializes its bundled widgets for preview and inspection.

```rust
let registry = Arc::new(Registry::for_app(cx));
let host = DataUi::new(&document, registry, logic, cx)?;
div().child(sidebar).child(host)
```
- **Pre-created:** loading compiles the document *once* into a `Template`. That is a flat arena of nodes
  with resolved widget factories (function pointers looked up in the registry at load), pre-resolved style
  recipes, and bindings resolved from string ids to **slot indices**. Instantiating and re-rendering never parses,
  hashes strings or allocates per node. The only dynamic cost is one `dyn` call per node.
- **Inherits the style context** (spec/14), so dynamic screens look identical to static ones.
- **Hot reload:** a new document compiles to a new Template. State in slots is kept by slot id.

## Bindings: events in, view data out, by id
The document puts literal values in `props`, assigns data slots in `bind`, and names handlers in
`on`. For example, `"bind": {"text": "title"}` reads the `title` slot and
`"on": {"click": "save"}` routes the widget's click to the `save` event. The generated catalogue
describes the property shapes, child slots, and event payloads available to documents. A Rust
`ui!` tree calls the component's public builder methods directly.

The host resolves event and data ids against its logic:
```rust
trait Logic {
    /// Small, copyable event (id + compact payload). Returns view-data patches.
    fn on_event(&mut self, event: EventId, payload: Payload<'_>, out: &mut PatchBuf);
}
```
- **View data** lives in a `SlotTable` inside the host (slot index → value). Logic never sends whole views,
  only **patches** (`set slot 7 = "Saved"`, `splice list slot 3 [2..4]`). The host applies them and notifies
  only the nodes bound to the touched slots (dirty bitset → minimal re-render).
- **Payloads are borrowed and small:** ids are `u32`, strings are `SharedString`/`Arc<str>`, large data is
  passed as handles (an `Arc` of shared, read-only data) rather than copied.
- **Static logic:** implement `Logic` in Rust, or bind ids to guiraffe-state events and selections.
  This is how compiled code attaches to a Mocket-designed screen.

## Logic in WebAssembly (later, designed for now)
The `Logic` boundary is deliberately message-shaped, which makes it a natural **wasm component** interface
(WIT: `on-event(id, payload) -> list<patch>`), hosted with wasmtime. The Rust host keeps the
shell and the built-in services. Because only events and patches cross the boundary, and both are
compact, the per-interaction crossing cost stays low.
Not in scope until the native path is solid. Nothing in the IR, the host or the `Logic` trait may assume native-only logic.

## Authoring: Mocket
Mocket is the design tool for UI-as-data. Two routes, as in DESIGN §10:
- **Import:** `guiraffe-mocket` converts Mocket's document (its JSON) into the GUIraffe IR. The IR stays our
  canonical runtime format, so we're not coupled to Mocket's internal schema version.
- **Export target / plugin:** Mocket emits the IR, or Rust via codegen, directly.
Mocket's component catalogue maps onto our widgets through one mapping table, which also drives conformance tests.
