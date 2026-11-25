# Stores and binding

> **Provisional (0.3).** The `store!` macro, the binding syntax in `ui!`, `#[component(stateful)]`
> and the lens API can still change. They freeze once the retained-scene work reports (ROADMAP
> 0.3, item 8). The builder API isn't affected and stays first-class.

A store holds your app's state. Controls bind to parts of it, and every edit is an event the
store records. Undo, interceptors, devtools and the action bus all see the same edits, and a
change re-renders only the controls that read what changed.

## Declare a store

```rust
use guiraffe_state::{KeyedList, store};

#[derive(Clone, Debug, PartialEq, guiraffe_state::Lens)]
pub struct Clip {
    pub name: SharedString,
    pub start: f64,
}

store! {
    pub Project {
        tempo: f32 = 120.0  where 20.0..=300.0,
        swing: u32 = 0      where |v: &u32| *v <= 100,
        follow: bool = true #[no_undo],
        clips: KeyedList<u64, Clip> = KeyedList::new() #[persist],
    }
    events {
        ResetTempo => |s| s.tempo = 120.0,
        Rename(id: u64, name: SharedString) => |s, fx| {
            if let Some(clip) = s.clips.get_mut(&id) { clip.name = name.clone() }
            fx.effect(Renamed(id, name))
        },
    }
}
```

`store!` writes plain code against `guiraffe-state`:

- the `Project` struct (`Clone`, `PartialEq`, `Debug`) and its `Default` from the initializers;
- `Project::store(cx)`, a `Store<Project>` holding the default state;
- a lens per field, `Project::tempo`, `Project::clips` and so on;
- a `Set` event per field (`SetTempo(f32)`, `SetFollow(bool)`, …);
- the listed events. `|s|` mutates the state. `|s, fx|` also gets the handler's `Fx` for
  effects and follow-up events.

`where lo..=hi` clamps every write. `where |v| …` rejects a value the predicate refuses. Every
write path applies these checks: the `Set` events, bound controls, `store.set` and the bus.
`#[no_undo]` keeps a field's writes out of undo history. `#[persist]` is metadata for a
persistence layer (`Field::is_persisted`).

The macro needs `guiraffe-state` and `gpui-kit` as dependencies. `#[derive(Lens)]` adds lenses
to nested structs, so paths can reach their fields.

## Lenses and keys

A lens is a `Copy` path into the state: `Project::tempo`, or
`Project::clips.by_key(7).then(Clip::start)`. It reads by reference and writes in place.

Each path prefix has a key. A write through a lens marks its own key and everything below it as
changed, and marks its prefixes so that watchers registered exactly there check again. All
other watchers are left alone. Editing clip 7's start therefore checks the watchers of clip 7
and the list's own watcher, which compares one order stamp. The other 9,999 clips aren't
touched.

- `by_key(k)` follows an item by its key through inserts, removes and moves. Once the item is
  removed, the lens reads `None`.
- `fx.set_at(lens, value)` is the keyed write. `fx.update(|s| …)` is unkeyed, so it re-checks
  every watcher. Use it for edits that span many fields.
- `fx.list_insert`, `fx.list_remove` and `fx.list_move` are structural edits on a `KeyedList`.
  They notify the list's watchers and the item's, and no other item's.
- `store.select_lens(lens, cx)` and `store.select_at(lens, cx, f)` give builder-API views the
  same routing.

`KeyedList<K, T>` is persistent (built on `imbl`). Cloning it is `O(1)`, and an edit copies only
the touched path while a snapshot shares it. Keys must be `Copy + Hash + Eq`.

## Bind controls in `ui!`

```rust
ui! {
    store(project) {
        Slider("tempo") { value <=> tempo  min: 20.  max: 300. }
        Switch("follow") { checked <=> follow }
        Text { "BPM " {<= tempo} }
        if <= follow { FollowBadge {} }
        for (id, clip) in <= clips, row_height: px(28.) {
            ClipRow(clip.name.clone()) { on_rename => Rename(id, clip.name.clone()) }
        }
    }
}
```

| Syntax | Meaning |
|---|---|
| `store(s) { … }` | `s` is a `Store` value. It is the store for every binding in the block. |
| `prop <= lens` | One-way: the prop reads the lens. |
| `prop <=> lens` | Two-way: the prop reads the lens, and the control's change callback writes it back. `value`, `checked` and `selected` pair with `on_change`. Any other `prop` pairs with `on_prop_change`. |
| `prop <=> lens via on_input` | Two-way through a named callback. |
| `{<= lens}` | Text that reads the lens. |
| `if <= lens { … } else { … }` | A branch on a `bool` lens. |
| `for row in <= list { … }` | A store-bound list (see below). `for (key, row) in …` also binds the key. |
| `on_x => Event(args)` | Dispatch an event into the store. |

A lens path starts at a store field (`tempo`) or at a row name (`clip.start`). It continues with
fields and `.by_key(k)`. Typos are rustc errors at the token you wrote. A change callback must
pass `&T` for the lens's `T`.

Each control with a binding becomes a small retained view. The view keeps the last value of each
lens it reads. When a write marks one of them, it compares, and if the value changed it updates
and notifies itself, at most once per frame. The parent doesn't render. Unbound props and
children are rebuilt in that view's render. They are captured by a `move` closure, so pass owned
values (`let min = self.min;`) rather than borrowing `self`.

Edits go through the store. A two-way control dispatches a `Set` event; `on_x => Event` dispatches
the event. Undo, interceptors and devtools see both. Each edit is tagged with its `Source`:

- the bus source while a control applies a bus action (`ActionBus::current_source`), such as
  `Shortcut` or `Mcp`;
- `Pointer` for a direct gesture;
- `Host` for `store.set(lens, v)` from app code.

A host write updates bound controls and is never reported back as a user edit, so an app needs
no feedback filter. An equal write isn't a change: there is no undo step and nothing notifies.

Without `store(…)`, `ui!` is exactly the 0.2 macro. An unbound control expands to the same
builder calls and costs the same (gated in `guiraffe-bench`).

### Protocol control entities

The controls in `guiraffe_components::controls` (`SwitchControl`, `CheckboxControl`,
`SliderControl`, `TextInputControl`, `SelectControl`) own their state and register on the bus.
Bind one with a single binding. The rest of its body configures its builder:

```rust
ui! {
    store(prefs) {
        SwitchControl { checked <=> dark  .path("prefs/dark")  .label("Dark") }
        SliderControl { value <=> volume  .path("prefs/volume")  .range(0., 1.) }
    }
}
```

The entity is built once with the lens's value. Edits arrive through its typed event and are
written with `Set`. Host writes reach it through its non-emitting setter, so they never echo.
Slider, text-input and select setters need the window today, so a host write reaches them one
effect cycle later. A bus edit of a control entity is tagged `Pointer` until its events carry
their source (both are being fixed in the components crate).

## Store-bound lists

`for row in <= list { … }` binds a `KeyedList`. Each item gets its own retained row view, which
watches only that item. The list watches its order.

- **Virtualized by default.** Only the rows in view are mounted (`uniform_list`, so rows share
  one height). Give `row_height: px(h)` so that each row is a cached view: an edit then
  repaints one row on both backends.
- **`mounted`** mounts every row. Use it for short lists. Its cost grows with the row count (see
  the [performance guide](../performance.md)).
- Inside the body, `row` is `&T`, so plain expressions read the item: `{row.name.clone()}`.
  Bindings can start at the row: `value <=> row.start` writes `list.by_key(key).start`.
- An item edit renders that row alone. An insert, remove or move renders the list and the
  affected row. The other rows keep their views.

The row views are `RowViews`, independent of the layout. A variable-height layout, such as
track heights from prefix sums, can drive the same row views.

## Stateful components

```rust
#[component(stateful)]
fn Counter(#[state] count: u32, step: u32) -> impl IntoElement {
    ui! { div { Button("inc") { label: "+"  on_click => count += step }  {count} } }
}

ui! { Counter("clicks", 1) {} }                                  // owns its state
ui! { store(app) { Counter("total", 5) { count <=> clicks } } } // state lives in the store
```

The first constructor argument is the component's id. `#[state]` parameters live in an entity
keyed by that id. In the body they are plain values. `on_x => field op= value` changes them.

The generated control is registered on the action bus at its id. `Observe` reports the state as
`value`: the field itself for one `#[state]`, or a record for several. `Drive` accepts
`set_value` in the same shape and rejects values of the wrong type. Each state field also has
the controlled pair `.count(v)` and `.on_count_change(f)`, plus `.default_count(v)` for the initial
value. That is what `count <=> lens` uses, so a bound component's state is the store's. Bus
writes then go to the store, tagged with the bus source.

State field types are `bool`, the integer and float types, `String` and `SharedString`.

## Builder API

Everything above is plain code. Without the macros:

```rust
let tempo = guiraffe_state::bound(&project, (Project::tempo,), |(tempo,), store, _, _| {
    let Some(tempo) = tempo.get() else { return Empty.into_any_element() };
    let store = store.downgrade();
    Slider::new("tempo")
        .value(*tempo)
        .on_change(move |v, _, cx| {
            if let Some(store) = store.upgrade() {
                store.set_from_control(Project::tempo, *v, cx);
            }
        })
        .into_any_element()
});
```

`bound_list(&store, Project::clips, |item, clip, store, window, cx| …)` binds a list.

## Backends

On gpui-fast, a bound edit re-renders only the bound control. Upstream GPUI re-renders the
ancestors of a notified view, so the parent and its unbound children render too. The Store's
work is the same on both: only the watchers on the written path run. For large lists on
upstream, virtualize and set `row_height`.

## Not yet

- The `shortcut:`, `spotlight:` and `ribbon:` props from the design, and `bind!`.
- Text inputs whose state lives in an entity (`InputState`) need an adapter to bind two-way.
- Positional `.at(i)` lenses. Use `by_key`.
- Generic and tuple structs in `#[derive(Lens)]`.
