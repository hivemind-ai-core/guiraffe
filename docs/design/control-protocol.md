# Control protocol: addressable, observable, drivable controls

Status: phase 1 has a provisional core implementation. Component adoption, binding and input-source integration follow in later phases (see the [roadmap](../../ROADMAP.md)).
Read with store-first `ui!` (store-first `ui!`) and the command ribbon (command ribbon).

## 1. Goal

Today four separate paths drive the UI, and none of them can reach an arbitrary control's state:

1. the commands registry (`guiraffe-commands`: keybindings, Spotlight) dispatches app commands;
2. DataUi documents have their own binding and dispatch (`crates/ui/src/host.rs`);
3. the inspect/drive API (MCP, replay) simulates mouse and keyboard on test ids;
4. physical input, handled privately by each control.

After this spec there is **one** way in. Every interactive control:

- has a stable **id** (§3);
- exposes a typed **snapshot** of its state (§4);
- accepts typed **actions** (§5), which are the same operations user input performs, with the
  same validation and the same events;
- reports changes through the same events it emits today.

Every input source turns into `(target, action)` on one **action bus** (§6). A click on a checkbox
becomes `Toggle` internally. So "bind Ctrl+B to toggle the bold button", "Spotlight: set zoom to
200 %", "MCP: select row 40 in the grid" and a recorded replay all execute the code path a user's
click executes.

## 2. Non-goals

- Not a new component model. Controls stay Rust builders over GPUI/Kit, controlled or
  uncontrolled as today.
- Not mandatory. A control rendered without an id, a store or any binding behaves exactly as in
  0.2 and costs exactly the same (§8).
- Not a virtual DOM or a diffing layer. No retained tree of control descriptions is built per
  frame.

## 3. Ids and the registry

- `ControlId`: an interned path, e.g. `settings/appearance/theme`. Built from the nearest id
  scope (window → panel → `ui!` document → `keyed` row) plus the control's own `.id()`. The
  storage is an interned `u64` plus a side table for the display path; comparisons and hashing
  never touch strings.
- Unifies the existing `test_id`, DataUi node ids and `ui!` keys: one id, one namespace. The 0.2
  "inspect/drive node ids" item is implemented by this section.
- `ControlRegistry` per window: `ControlId → (entity handle, ControlKind, capability bits)`.
  Registration happens when a control's state entity is **created or re-keyed**, never per render.
  Stateless `RenderOnce` controls (Button, Badge…) register only when they carry an explicit id
  and something is bound to them; otherwise they are invisible to the registry.
- Cross-window addressing: `WindowId/ControlId`. When a docked panel moves windows (0.3 §D), its
  registrations move with it; ids don't change.
  `guiraffe_dnd::dock::DockablePanel` does this today: its key is the panel's id scope, its own
  `dock-panel` control (shared `open` = pop out, `close` = dock back) re-registers in whichever
  window hosts the panel, and commands issued from the detached window run in the home window.
  Panel-internal controls should build their paths under the same key and re-register with it.
- Duplicate ids within a window are a debug-build error with both call sites, and a no-op
  (first wins) in release.

## 4. Snapshots

```rust
pub trait Observe {
    type Snapshot: Clone + PartialEq + 'static;
    fn snapshot(&self, cx: &App) -> Self::Snapshot;
}
```

- Typed per control (`SliderSnapshot { value, min, max, step, disabled }`), plus a uniform erased
  form `ControlSnapshot` (a small enum of common fields: value as `Value`, checked, selected
  indices, open, disabled, focused, label) for MCP/devtools/scripts.
- Snapshots are computed on demand (MCP query, devtools open, replay assertion), never every
  frame. Rich components expose summary snapshots plus paged queries (grid: dimensions,
  selection, sort, visible range; `rows(range)` on request) so a 1M-row grid never serialises 1M
  rows.

## 5. Actions

```rust
pub trait Drive {
    type Action: 'static;
    fn apply(&mut self, action: Self::Action, window: &mut Window, cx: &mut Context<Self>)
        -> Result<(), ActionError>;
}
```

- A shared vocabulary covers most controls: `SetValue(Value)`, `Activate`, `Toggle`,
  `Open`/`Close`, `Select(Selection)`, `Focus`, `ScrollTo(Target)`, `Increment`/`Decrement`.
- Rich components add domain actions: grid `SelectRows`, `SortBy`, `EditCell`; Flow `MoveNode`,
  `Connect`; Timeline `MoveItem`, `ZoomTo`; CodeViewer `RevealLine`, `ToggleFold`; Kanban
  `MoveCard`; Calendar `GoTo(date, view)`; PropertyGrid `Edit(path, value)`.
- Every action has an erased, serialisable form (`ActionName` + `Value` payload) with a JSON
  schema generated from the catalogue (agent authoring, DataUi catalogue), so MCP and scripts can list and
  validate them.
- `ActionError`: `UnknownTarget`, `Unsupported`, `Disabled`, `Invalid { reason }`, `ReadOnly`.
  Never panics; invalid input leaves state unchanged.
- **Physical input goes through `apply` too.** A control's mouse/key handlers translate input into
  its `Action` and call the same function. That is what guarantees identical behaviour and makes
  recording complete.

## 6. The action bus and input sources

One per-window bus: `bus.dispatch(target, action, source, cx)` where `source` is `Pointer`,
`Keyboard`, `Shortcut`, `Spotlight`, `Ribbon`, `DataUi`, `Script`, `Mcp`, `Replay`.

- Resolution: the registry gives the entity; `apply` runs synchronously on the UI thread inside
  the normal GPUI update, so events, `cx.notify` and Store dispatch behave as today.
- Interceptors (debug/recording/permissions) see `(target, action, source)` before and the
  outcome after. Recording is an interceptor; replay is a source. An interceptor can deny (e.g. an
  app that disallows MCP mutating anything outside one panel).
- `guiraffe-commands` becomes a client of the bus: a command's handler can be "apply action X to
  control Y" (declarative binding, no closure), and every registered control action can be
  surfaced as a command (`control.settings/appearance/theme.set_value`) with an auto-derived label
  from the catalogue. That is what lets users bind **any** input to a shortcut, Spotlight entry
  or ribbon entry.
- DataUi's `host.rs` dispatch is reimplemented on the bus (its documents already have ids).
- The inspect/drive API gains `query(id) → snapshot` and `apply(id, action)`. Simulated input
  stays for tests that exercise hit-testing and focus; agents and scripts use actions.

Status (0.3 item 12):

- Commands: `Handler::apply(path, action)` is the declarative binding. Any registered control
  action is the command `control.<path>.<action>` (command ids accept `/`), resolved on the bus
  only when invoked; `Commands::control_command` builds one titled from the control's label for
  a shortcut or ribbon entry, and Spotlight lists every control action when it opens (none are
  enumerated or allocated before). Keymap runs dispatch as `Shortcut`, the palette as
  `Spotlight`, menus as `Pointer`, programmatic `execute` as `Host`.
- DataUi: `DataUi::register(host, path, window, cx)` makes the host a control; each document
  event is an action named after the event. Pointer input dispatches with `Source::DataUi`.
  Events inside repeated rows still dispatch directly: row identity is not addressable yet.
- Inspect/drive and MCP: `Action::Apply { control, action }` and `Call::Query { control }`;
  MCP tools `query` and `apply`. Applies are `Source::Script` from scripts and the socket, and
  `Source::Mcp` from MCP tool calls.
- `Recorder` is the recording interceptor (by path, batches kept as groups); `replay` dispatches
  a recording with `Source::Replay`, which is never recorded again.
- Toasts: `Toast::action(label, control, action)` adds a button that dispatches the action with
  `Source::Pointer` and dismisses the toast.

## 7. Store binding

- `guiraffe-state`'s `Store<S>` becomes the canonical binding layer. A control prop can be bound
  to a lens into a store (`Lens<S, T>`: get by reference, set by event).
- One-way (`value ← store`) uses a `Selected` subscription with reference-equality projection,
  so only controls whose slice changed are notified.
- Two-way (`value ⇄ store`): the control's change event dispatches a generated `Set` event (or a
  user event) into the store; undo/redo, interceptors, persistence and devtools time travel come
  from the Store.
- Batching: store notifications coalesce to one notify per control per frame.
- Controlled/uncontrolled stay: "controlled" is now "bound to a store or to a caller value",
  "uncontrolled" stays the default for unbound controls.
- store-first `ui!` defines the ergonomic surface (`ui!` binding syntax, `store!`).

## 8. Performance rules (gates, not goals)

These are acceptance criteria for every phase, enforced in `guiraffe-bench` with allocator and
scaling tests that don't depend on machine load:

1. **Zero cost when unused.** A control without id or binding: identical allocations and
   render work to 0.2 (bench: 1k-control page, protocol on vs off ⇒ equal allocation counts,
   equal element count).
2. **Zero cost at render when used.** Registration happens on entity creation, re-key or
   unmount, never in `render`. Phase 1 gates the protocol’s own cost at zero: an unchanged frame with 1k registered
   controls must allocate exactly the same count and bytes as the identical unregistered frame.
   The whole-frame zero-allocation target belongs to roadmap items 8 and 19.
3. **No string work on hot paths.** Ids are interned integers; lookups are `O(1)` hash or
   index; display paths are built only for MCP/devtools/errors.
4. **Precise invalidation.** A store change notifies only controls whose lens result changed
   (`RenderCounter` tests: changing one field of a 1k-field store re-renders exactly the bound
   control(s)).
5. **Bounded dispatch cost.** `dispatch` is `O(1)` plus the control's own work; interceptors are
   opt-in and the empty interceptor list costs a branch.
6. **Scaling.** Registry memory is `O(registered controls)`, not `O(rendered elements)`;
   virtualised components register once, not per visible row (rows are addressed through the
   component's own actions, e.g. `SelectRows`).
7. Existing performance budgets (mount ≤200 ms, p99 ≤8 ms, no task >50 ms, memory) still hold on
   every component after adoption; no regression against the 0.2 bench baseline.

## 9. Phases and size

| Phase | Work | Size |
|---|---|---|
| 1. Core | `ControlId`, interning, registry, `Observe`/`Drive`, erased forms, bus, interceptors, bench gates §8.1–8.3 | medium, 1 worker |
| 2. Simple controls | button, checkbox, switch, radio, slider/range, text/number inputs, select/combobox, tabs, segmented, pickers, menus; a derive/helper macro for the common case | large but mechanical, 2–3 workers by crate |
| 3. Rich components | DataGrid, Sheet, Tree/PropertyGrid, Flow, Timeline family, CodeViewer, Charts, Calendar, Kanban, Chat, Markdown | large, 3–4 workers by crate |
| 4. Store binding | lenses, one/two-way binding, batching, `RenderCounter` gates; generalise DataUi binding | medium |
| 5. Sources | commands ⇄ bus, DataUi on bus, MCP/inspect `query`/`apply`, recording and replay as action logs | medium |
| 6. Devtools | control tree with live snapshots, action log, time travel through Store | small–medium |

Breaking changes: none intended. Adoption is additive; a component whose public builder changes
gets a CHANGELOG entry.

## 10. Acceptance

- Property tests: for each control, `apply(a)` from any reachable state equals the state reached
  by the equivalent simulated input (pointer/keyboard), and invalid actions leave state unchanged.
- GPUI `TestAppContext` tests: every source (pointer, shortcut, Spotlight, ribbon, DataUi, MCP,
  replay) produces identical events and final state for the same action.
- Allocation and render-isolation gates in §8, both backends.
- Catalogue: every control's actions and snapshot fields are listed with JSON schemas; a
  coverage test fails when a catalogue control lacks them.
- Docs: protocol guide, "make your own control drivable" recipe, MCP examples.

## Provisional Rust API

The phase 1 API lives in `guiraffe_core::protocol`. Persist the inspect path
for replay. A copyable integer `ControlId` is meaningful only in its owning
app and does not persist across runs. Windows in one app share the path
interner and resolve the same handle against their own registrations.
Register at entity creation and unregister before changing the path.
The registry observes entity release and window closure. To inspect a path
without creating a registration, use `NodeId::control_id` in guiraffe-test.

The provisional `Capabilities` flags advertise the ten shared actions.
Keymaps, Spotlight and the ribbon can enumerate their wire names with
`action_names()`. Bits 10–63 are reserved for future shared actions.
Item 10 will supply descriptors for domain actions and generate payload
schemas from the catalogue as controls adopt the protocol.

`Source` and `ActionError` are non-exhaustive so later sources and rejection
reasons are additive. `Source::Host` tags application-initiated actions;
plain store writes from the host (a playhead, a view range) never pass through
the bus and so never return as user actions. `Devtools` and `Test` tag those
clients.

`Drive::apply` returns `Drive::Output: Into<Outcome>`. `Outcome` wraps an
erased `Value`, empty by default and free to construct, so interceptors and
recorders see typed results such as `KeyInserted { t }` without subscribing to
entity events. Use `()` when an action has nothing to report.

Interceptors receive `&Window` and `&App` in `before` and `after`, so permission
checks and recorders can read the store. They cannot mutate it during dispatch.

`ActionBus::dispatch_batch(id, actions, source, ..)` applies a run of actions to
one target as a `Group`. Interceptors see `begin_group`, each action's
`before`/`after`, then `end_group`. The batch stops at the first rejected
action. Store history (phase 4) maps one group to one undo step, so a
drag-paint of N cells undoes at once. A single `dispatch` is its own step. Dispatch has an allocation gate of zero
(`bench/tests/allocator.rs`) and a Callgrind ceiling of 3,000 instructions
(`bench/benches/instructions.rs`; 2,111 measured with no interceptors), which excludes path lookup.
