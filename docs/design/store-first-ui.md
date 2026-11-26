# Store-first `ui!`: the canonical structured path

Status: implemented as provisional for 0.3 (see [Stores and binding](../guides/stores.md) and §9). The syntax freezes once the retained-scene work reports (ROADMAP item 8). Builds on today's `ui!` (`ui!` v1) and the control protocol
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

The composed headless window sweep measures a changed label in a cached row
end to end. Run `cargo test --release -p guiraffe-bench --test composed
composed_label_phase_sweep -- --ignored --nocapture`. Each sample checks that
the edited row rendered. A FIFO marker after the Store flush separates dispatch
and selector refresh from subsequent GPUI notifications, rendering, draw and
replay. This split includes the row's GUIraffe render code in the second phase.
There are four warm-ups and 31 measured updates per size. CPU is measured on
the calling thread and allocation counts include benchmark marker overhead.

| Upstream, test profile | Total median CPU | Through Store flush | After flush | Median allocations |
| --- | ---: | ---: | ---: | ---: |
| 1,000 cached rows | 16.09 ms | 0.56 ms | 15.50 ms | 1,303 |
| 3,000 cached rows | 53.66 ms | 2.13 ms | 51.26 ms | 3,303 |

Phase medians are computed independently. The fixed-width buffer allocation
assertion passed at both sizes. Unbound controls receive no prototype code
because the binding table lives entirely in the benchmark.

| Upstream, release profile | Total median CPU | Through Store flush | After flush | Median allocations |
| --- | ---: | ---: | ---: | ---: |
| 1,000 cached rows | 12.44 ms | 0.26 ms | 12.17 ms | 1,303 |
| 3,000 cached rows | 42.07 ms | 1.20 ms | 40.93 ms | 3,303 |

**No-go on freezing the binding syntax or claiming the 0.5 ms budget.**
At 3,000 rows the complete frame costs 84 times the budget. The roughly 41 ms
after the flush is the upper bound on what a draw-side change could save in
this run. Actual savings require a backend implementation and measurement.
Even eliminating that entire phase leaves 1.20 ms before the marker. The
release harness measured 5.7 and 6.1 ns/edit for the buffer-only sweep with
zero allocations on the checked write. This host was shared during measurement.

### Sinks follow-up: splicing, keyed Store writes, measured end to end

Two changes landed on the 0.3 sinks branch. Neither adds GPUI public API.

- **gpui-fast splices a page around an edited row.** `RenderDependencies::union`
  dropped the other side's write windows whenever that side read nothing. A page
  holding a focused text input writes the input's state as the input draws. That
  is state the page read while rendering, so the writes looked foreign. Every row
  edit then rebuilt the page, and each of its rows was reused one by one. The page
  is now drawn from last frame around the edited row with the existing
  retained/splice machinery. Unchanged rows are copied as prepaint and paint
  ranges. There is no parallel cache (see `vendor/README.md`).
- **Keyed Store writes (provisional).** `Fx::set_keyed_field(key, value, read,
  field)` and `Store::select_key(key, ..)` route a row edit to the selections of
  its key and to unkeyed selections. Unkeyed writes, undo and redo refresh every
  selection. When no undo history or interceptor observes the old snapshot, the
  write lands in place without cloning the state. The flush is O(changed), no
  longer O(selections). The API is a correctness contract: a keyed selection
  may depend only on fields written under its key or written unkeyed. Treat it
  as provisional until `store!`/`ui!` generate keys.

Release profile, `composed` sweeps: one row label edited per frame, median of
31 samples, CPU on the calling thread. Virtualized lists use `uniform_list`
with 24 rows mounted. A visible edit changes row 3; an offscreen edit changes
the last row, which is not mounted. "Fully mounted" means every row is a cached
view. On gpui-fast, the window is tall enough to show every row
(`COMPOSED_ALL_VISIBLE`). Upstream uses the 800 px window, because a 3,000-row-tall
headless window costs upstream 5 s per frame. Instructions (Ir) are Callgrind
per changed frame. "Before" is the state at `cfa4860d`.

| Backend | Mode | Rows | Before: CPU / allocs | After: CPU / allocs | After: Ir per frame |
| --- | --- | ---: | ---: | ---: | ---: |
| fast | virtualized, visible edit | 1k | 0.263 ms / 397 | 0.253 ms / 395 | flat (gated ±2%) |
| fast | virtualized, visible edit | 3k | 0.260 ms / 397 | 0.254 ms / 395 | flat |
| fast | virtualized, visible edit | 10k | 0.290 ms / 397 | 0.259 ms / 395 | flat |
| fast | virtualized, offscreen edit | 1k | 0.007 ms / 6 | 0.0017 ms / 3 | flat |
| fast | virtualized, offscreen edit | 3k | 0.013 ms / 6 | 0.0017 ms / 3 | flat |
| fast | virtualized, offscreen edit | 10k | 0.040 ms / 6 | 0.0018 ms / 3 | flat |
| fast | fully mounted, all visible | 1k | 18.2 ms, 28.68M Ir / 4,357 | 10.1 ms / 185 | 12.63M |
| fast | fully mounted, all visible | 3k | 85.04M Ir / 12,362 | 29.7 ms / 191 | 38.33M |
| fast | fully mounted, all visible | 10k | 278.88M Ir / — | 62.3 ms / 196 | not gated |
| fast | fully mounted, 800 px window | 1k / 3k / 10k | — | 7.0 / 21.2 / 80.9 ms; 185 / 191 / 196 allocs | — |
| upstream | virtualized, visible edit | 1k / 3k / 10k | not measured | 1.16 / 1.12 / 1.16 ms; 312 / 318 / 310 allocs | — |
| upstream | virtualized, offscreen edit | 1k / 3k / 10k | not measured | 0.0027 / 0.0026 / 0.0024 ms; 3 allocs | — |
| upstream | fully mounted, 800 px window | 1k | 12.44 ms / 1,303 | 12.66 ms / 1,300 | — |
| upstream | fully mounted, 800 px window | 3k | 42.07 ms / 3,303 | 41.14 ms / 3,300 | — |
| upstream | fully mounted, 800 px window | 10k | — | 147.5 ms / 10,300 | — |

The Store flush is now flat on every row: 15–23 µs fully mounted, and 1.0–1.7 µs
for an offscreen virtualized edit. Wall-clock CPU was measured on a shared host.
Treat Ir as the primary measurement for gpui-fast.

**Where this leaves the 0.5 ms budget.**

- gpui-fast virtualized edits meet the budget and are flat from 1k to 10k.
  Upstream virtualized edits are flat but cost about 1.1 ms.
- Fully mounted gpui-fast edits take about 2.2 times fewer instructions (1.8 times
  less CPU at 1k) and allocate a flat ≈190 per edit, but they still grow linearly. What remains is
  per primitive, not per view. The remaining cost is roughly half scene replay
  and primitive insertion (with its bounds-tree insert), a third the scene's
  draw-order sort (`u128` and sprite sorts), then copying dispatch nodes,
  hitboxes and element-state accesses. Copying unchanged rows as ranges cannot
  remove that cost. A flat fully mounted frame needs a scene, bounds tree and
  dispatch tree that persist across frames and are patched in place. That is the
  retained-element backend experiment described above, beyond range reuse.
- Upstream (gpui-pre) has no splice: a notified row marks its ancestors dirty,
  and upstream renders the page again. The smallest equivalent change would
  port splicing, and there is no small one. The documented fallback is the
  0.3 default: virtualize store-bound lists and tables. Upstream gets a flat
  edit cost at about 1.1 ms. Fully mounted large lists are not supported within
  the budget on upstream.

Gates: `scripts/label-scaling.py` keeps virtualized visible and offscreen edits
within 2% from 1k to 10k. It also caps fully mounted frames at 14M Ir (1k) and
42M Ir (3k). Before the page was spliced, those frames cost 28.7M and 85.0M.
The `fully_visible_row_edits_splice_the_page` test runs on both backends. It
asserts that a keyed edit runs one selector. On gpui-fast it also asserts that
the page is spliced, with at most 300 allocations per edit and growth of at most
16 from 100 to 400 rows. The splice regression reproduces only in release
builds; the debug test profile splices either way. `crates/state/tests/keyed.rs`
covers keyed routing, equal writes, unkeyed writes, undo, batching and release.

## 9. What 0.3 built, and how it differs from this draft

The guide is [Stores and binding](../guides/stores.md). These decisions were made with the
foreman and are provisional:

- **Operators.** `<=` and `<=>`, as in §3. `<-` is rejected with a hint. Reactive text is
  `{<= lens}`, because a bare `{name}` is already a Rust expression in `ui!` v1.
- **Lenses** are `Copy` values (`guiraffe_state::Lens`). `store!` and `#[derive(Lens)]` generate a
  `Field` const per field and a path type, so `ui!` can resolve `a.b.by_key(k).c` against the
  store type. Every path prefix has a change key. A write marks its own key as a changed subtree
  and its prefixes as changed exactly. Watchers register at their full path in a key tree in the
  Store. The keyed `select_key`/`set_keyed_field` contract from the sinks work is the flat case of
  this tree.
- **Item identity is the key.** Collections are `KeyedList<K, T>` (persistent, `imbl`), and
  item lenses are `by_key(k)`. Inserts, removes and moves (`Fx::list_insert`/`list_remove`/
  `list_move`) notify the list's watchers and that item's, and nothing else. This is gated at
  10k rows. There is no positional lens yet.
- **Validation lives on the lens.** `where` clauses become `Lens::check`, so `Set` events,
  bound edits, host writes and bus writes all clamp or reject the same way.
- **Sources.** `Event::source()` (default `Host`) is recorded in `EventRecord::source` and
  readable as `Fx::source()`. Bound edits use `ActionBus::current_source`, set around
  `Drive::apply`, falling back to `Pointer` for direct gestures. App writes are `Host`.
- **No sinks yet.** §3 asks for reactive sinks. Until the retained-scene backend exists, a
  bound control is a small retained view with one Store watch: one compare per write on its
  path, and one notify per frame. This is the "generated cached view" fallback, and it is
  measured. A steady-state binding update allocates nothing, and a label edit is flat from 1k to
  10k rows (`bench/tests/bound.rs`). On upstream GPUI the notified view's ancestors re-render.
- **Lists** are virtualized by default with `uniform_list`. `mounted` opts into every row, and
  `row_height` makes rows cached views. Row views (`RowViews`) are separate from the layout, so
  a variable-height list can reuse them.
- **Stateful components** take an id first, register on the bus at that id, and expose the
  controlled pair `field(v)` / `on_field_change` per `#[state]` field. That is what `<=>`
  binds to.
- **Deferred:** `shortcut:`/`spotlight:`/`ribbon:` props, `bind!`, `#[derive(Event)]`,
  `command!`, expansion-size benchmarks and `trybuild` diagnostics for every row.

### Measured: the composed label edit in the new syntax

`bench/tests/bound.rs` builds the composed page from `store!` and `ui!`: a `KeyedList` of rows,
`for (id, row) in <= rows` and a bound title. Release profile, gpui-fast, one row label edited
through `store.set` per frame, from the write through the drawn frame. Median of 31 edits on a
shared host (`bound_label_sweep`).

| Mode | 1k rows | 3k rows | 10k rows |
|---|---:|---:|---:|
| virtualized (`row_height` set) | 0.140 ms, 195 allocs | 0.140 ms, 195 allocs | 0.140 ms, 195 allocs |
| fully mounted (`mounted`) | 3.34 ms, 162 allocs | 10.8 ms, 167 allocs | 38.6 ms, 173 allocs |

A virtualized edit is flat and within the 0.5 ms budget. A fully mounted edit still grows with
the row count, for the per-primitive reasons in the sinks follow-up above. These gates run on
both backends in every gate run:

- the binding update itself (write, keyed flush, compare, notify) allocates nothing at 1k, 3k
  and 10k rows;
- the edited frame's allocations vary by at most 8 across those sizes, and on gpui-fast one
  row renders per edit;
- an insert at 10k rows allocates at most 64 times in its flush and repaints only the viewport;
- an unbound `ui!` page with 1k controls allocates exactly what the builder page does.

### Timing: the first edit and parent re-renders

Two timing defects were fixed before the syntax freezes.

- **The first edit after mount rebuilt the parent (gpui-fast).** `EntityMap::insert` recorded
  an access for every entity created while a view rendered. A parent that mounts a bound
  control creates the control's view as it renders, so it depended on that view until it next
  rebuilt, and the control's first notification rebuilt the parent. Creating an entity no
  longer counts as reading it (vendored gpui-fast, see `vendor/README.md`). The first edit
  after mount renders one row and not the page. A row's first edit, once the window is drawn,
  allocates what a steady edit does (`bench/tests/bound.rs`). The window's very first spliced
  frame still grows GPUI's own frame buffers once.
- **A parent re-render reached bound children a frame late.** The body for a bound site now
  lives in its keyed element state (`Holder`). The parent writes it as it renders and the
  control's view reads it as it renders. gpui-fast builds a reader of an entity written while
  drawing again in the same frame. The parent depends only on the holder, never on the view
  the Store notifies. No deferred notification is left, and nothing allocates beyond the new
  body closure. A parent re-render and a store write in one tick render once, with both values
  (`a_parent_render_and_a_store_write_in_one_tick_show_together`).

### Passage (0.3.1) against the binding layer

| Passage need | How the binding layer expresses it | Gap |
|---|---|---|
| Keys, clips and step cells as store state | `KeyedList<Id, Clip>`, bound by key; a cell edit is a keyed write to one item | Cells in a 2-D grid need a `KeyedList` of rows or a composite key; no grid-shaped collection yet |
| Insert/remove/move keys and clips | `Fx::list_insert/remove/move`: notify the list and the item only | Bulk structural edits (drag-move of many clips) mark each item; no range form |
| Host writes that never echo | `store.set` is `Source::Host`; a bound control shows it and does not call back; equal writes are no-ops | — |
| User edits tagged with their source | `Source` on every event; `ActionBus::current_source` during `Drive::apply` | Grouped bus batches (`dispatch_batch`) are not one undo step in the Store yet |
| One undo step per drag-paint | A domain event that writes many cells is one step | Store history still snapshots the whole state (item 11 adds bounds); no "merge with previous" for scrubs |
| Per-track heights, prefix-sum rows | `RowViews` are layout-independent | The default list is uniform-height; a prefix-sum list element is 0.3.1 work |
| Meter levels and the playhead at frame rate | Bindable, at one compare per write | Through a store, each frame costs an event and a flush; hot values belong in sinks (item 8) |
| Value mapping and formatters shared by Knob/NumberField/Slider | Lens targets are plain data; mapping stays in the control | Not a binding-layer concern |
| Every control drivable without simulated clicks | Stateful components and bound controls go through the Store, and stateful components register on the bus | Built-in controls still need protocol adoption (item 10) |

