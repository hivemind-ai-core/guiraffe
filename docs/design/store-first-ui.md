# Store-first `ui!`: the canonical structured path

Status: draft for 0.3, not implemented (see the [roadmap](../../ROADMAP.md)). Builds on today's `ui!` (`ui!` v1) and the control protocol
(control protocol).

## 1. Position

GUIraffe has two supported ways to build UI:

- **Builder API (plain Rust).** Standalone, no store, no ids. Best for small tools, transient
  modals, one-off dialogs and the internals of components. Stays first-class and is never
  deprecated.
- **Structured path: `ui!` + stores + events (canonical).** State lives in `Store`s, controls bind
  to it declaratively, changes are typed events. This path gives you, at no per-render cost:
  devtools (state tree, event log, time travel), undo/redo, persistence, shortcut, Spotlight and
  command-ribbon bindings for any control action, MCP/script control and replay.

Today `ui!` is a purely mechanical translation of the builder API (today's `ui!`). In 0.3 it gains
first-class binding syntax so the structured path is the easy one. Docs, templates
(`guiraffe new`), examples and the showcase lead with it.

## 2. Stores: `store!`

```rust
store! {
    pub Settings {
        theme: Theme = Theme::System,
        font_size: f32 = 14.0  where 8.0..=48.0,
        recent: Vec<PathBuf> = vec![]  #[persist],
        draft: Draft = Draft::default()  #[no_undo],
    }
    events {
        ResetFonts => |s| s.font_size = 14.0,
        Open(path: PathBuf) => |s, fx| { s.recent.insert(0, path.clone()); fx.effect(OpenFile(path)) },
    }
}
```

Generates: the state struct (`Clone + PartialEq`), `Store<Settings>` constructor, typed field
lenses (`Settings::theme`, `Settings::font_size`…), a `Set<field>` event per field with the
validation (`where` range/predicate), the listed events implementing `Event<Settings>`, and
metadata for devtools/persistence/undo exclusions. All of it is plain code a user could write by
hand against `guiraffe-state`; the macro only removes boilerplate. Nested structs and
collections get lenses through `#[derive(Lens)]`.

## 3. Binding syntax in `ui!`

```rust
ui! {
    store(settings) {                       // id scope + default store for bindings
        Select("theme") { value <=> theme   options: Theme::ALL }
        Slider("font-size") { value <=> font_size  min: 8.  max: 48. }
        Text { "Preview at " {font_size} "px" }        // one-way read, re-renders only this node
        Button("reset") { label: "Reset"  on_click => ResetFonts  shortcut: "mod+0" }
        Switch("compact") { checked <=> ui.compact  disabled <= ui.locked }
        for item in recent key item { RecentRow(item) { on_click => Open(item.clone()) } }
        if <= ui.show_advanced { AdvancedPanel {} }
    }
}
```

| Syntax | Meaning |
|---|---|
| `store(s) { … }` | default store and id scope for the block; nests (`store(other)`) |
| `prop <= lens` | one-way binding: prop reads the lens; subscription via `Selected` |
| `prop <=> lens` | two-way binding: read plus the control's change event dispatches `Set` |
| `on_x => Event` / `on_x => Event(args)` | dispatch a typed event into the scope's store |
| `{lens}` in text, `if <= lens`, `for … in <= lens` | reactive reads of a slice |
| `shortcut: "…"`, `spotlight: "…"`, `ribbon: "path/…"` | register the control's primary action as a command (the control protocol §6) |
| `id`/`key` | contribute to the `ControlId` path (the control protocol §3) |

Rules:

- Lens names resolve against the scope's store type at compile time; typos are rustc errors on
  the user's token, with a note listing the store's fields.
- Reads expand to a projection that returns a reference; no clone unless the prop takes an owned
  value, and then exactly one clone at change time, not per render.
- Bound regions compile to fine-grained **reactive sinks** (ROADMAP.md, reactive sinks): a store
  change updates the smallest region that read it, not the whole `ui!` tree. A sink is
  *not* a full view (entity + `Render` + subscription + stored copy + guard per label, the cost
  "A Critique of GPUI" measures); it is a lightweight, framework-managed binding that patches a
  retained element (text, style, visibility, list range) and re-runs only its own closure. Falling
  back to a generated cached view is allowed only where the sink design can't express it, and
  must be measured.
- Everything still expands to normal builder calls plus `guiraffe-state` calls; `cargo expand`
  output is readable and documented.
- Without `store(…)`, `ui!` behaves exactly as v1 (today's `ui!`): mechanical, zero extra code.

## 4. Stateful components: `#[component(stateful)]`

```rust
#[component(stateful)]
fn Counter(#[state] count: i32, step: i32) -> impl IntoElement {
    ui! { Group { Button("dec") { label: "−" on_click => count -= step }
                  Text { {count} }
                  Button("inc") { label: "+" on_click => count += step } } }
}
```

Generates a Control with a state entity, an id, `Observe`/`Drive` impls (snapshot of `#[state]`
fields, `SetValue` per field), and the option to bind its state to an outer store
(`Counter { count <=> clicks }`). This is the today's `ui!` §6 follow-up, now protocol-aware, so user
components are as drivable as built-ins.

## 5. Other helpers (only if they earn their keep)

- `#[derive(Event)]` for standalone event types with an `apply` fn.
- `command!` for app-level commands with ids, labels, default shortcuts and ribbon placement,
  shared with the command ribbon.
- `bind!(store.lens)` for builder-API users who want one binding without `ui!`.

## 6. Performance (same gates as the control protocol §8)

- Allocation parity: a `ui!` page with N bound controls vs the equivalent hand-written
  builder-plus-subscriptions code ⇒ identical allocation counts; unchanged repaints allocate
  nothing.
- Render isolation: changing one store field re-renders only the bound nodes (`RenderCounter`),
  on both backends.
- Compile time: expansion size per bound control is bounded and measured (macro bench in CI);
  deeply nested documents don't blow up compile time.

## 7. Migration and docs

- Rewrite `examples/team-tasks` and `examples/getting-started` on stores; keep a builder-only
  example ("standalone tool") to show the other path.
- `docs/ui-macro.md` gains "Stores and binding"; `docs/from-react-electron.md` maps
  useState/Redux/Zustand to stores and lenses.
- Showcase entries expose their knobs through a store, so the showcase itself demonstrates
  devtools time travel and shortcut binding of any knob.

## 8. Acceptance

- Expansion snapshot tests and `trybuild` diagnostics for every row in §3 and §2.
- GPUI tests: two-way binding round-trips; undo restores bound controls; Spotlight/shortcut/MCP
  driving a bound control updates the store and every other control bound to the same lens.
- Gates in §6.

### Provisional fixed-metric label experiment

The bench-only `reactive_sinks` test mounts one binding per row and patches
eight glyph commands in a retained buffer on each changed write. A standalone
optimized build of the same sweep measured 6.30 ns CPU/edit at 1,000 rows and
6.35 ns at 3,000 (median of seven processes, one million edits per process,
including startup and mount). These timings cover indexed dispatch and the
buffer write. They exclude Store notification, shaping, layout, GPUI frame
submission and presentation.

Keep the binding syntax provisional. An indexed write can carry a stable row
key and property identity straight to its sink. A positional row index works
only while rows stay put. Reordering, removal and width-changing text need
invalidation semantics before freezing that contract. Leave unbound builder
calls on their existing expansion. Defer adoption until the composed-window
benchmark measures the complete frame and compares unbound controls.

A backend experiment would need stable handles to retained text paint commands,
an invalidation queue keyed by property, and a replay path that splices a dirty
command range without traversing sibling views. Preserve paint order, clipping,
transforms, hit testing and accessibility. Re-shape changed text and propagate
layout invalidation when advances or bounds change. Retire handles on unmount.
This should remove sibling-dependent replay CPU for fixed-size labels. The
buffer experiment cannot predict the absolute frame time. Even a free draw
would leave the cost of notifying all current row selectors, so a keyed write
path in the Store is a separate prerequisite.
