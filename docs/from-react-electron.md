# GUIraffe for React and Electron developers

GUIraffe is a native Rust desktop toolkit. It runs on [GPUI](https://www.gpui.rs/), the GPU
UI framework from Zed, through GPUI Kit (GPUI plus the GPUI Controls widget library). There's
no browser, DOM or JavaScript runtime. If you know React, most of the ideas carry over.
This page maps them, explains where the performance model differs, and summarises how a
GUIraffe app compares with the same app in Electron + React.

New to GUIraffe? Work through [getting started](getting-started.md) first. This page refers
to its examples.

## Concept map

| React / Electron | GUIraffe | Notes |
|---|---|---|
| Function component | **Component**: a `#[component] fn` or a `#[derive(IntoElement)]` struct with `impl RenderOnce` | Stateless and rebuilt on every render, like a function component without hooks. `Card`, `Text`, `Title` and the `Stat` in [02_counter.rs](../examples/getting-started/examples/02_counter.rs) are components. |
| Stateful component (`useState`, `useRef` for a DOM node) | **View**: a struct that implements `Render`, held in an `Entity<T>` | The struct *is* the state. GPUI keeps it alive between frames (retained), not just while a hook slot exists. |
| Controlled input widget (`<input value onChange>`) | **Control**: an `Entity<XState>` that owns interaction state and emits events, rendered with `X::new(&state)` | `InputState`/`Input`, `SegmentedControl`, `GridHandle`/`DataGrid`. See [spec/00-api-conventions.md](../spec/00-api-conventions.md) §1. |
| Props | Builder methods named after Mantine props: `.size(Size::Md)`, `.color(ColorName::Teal)`, `.dimmed(true)`. In `ui!`, `name: value` | `#[component]` makes each function argument a prop. `#[prop(default)]` makes it optional. |
| JSX | The `ui!` macro, or plain builder chains (`div().flex().child(..)`) | Both produce the same GPUI elements. See [ui-macro.md](ui-macro.md). |
| `useState` + `setState` | A field on the view, mutated in `cx.listener(|this, _, _, cx| { …; cx.notify() })` | Nothing re-renders until you call `cx.notify()`, and then only that view. |
| Redux / Zustand / re-frame | `guiraffe_state::Store<S>`: typed `Event`s, pure handlers, effects as data, undo | `store.select(cx, |s| …)` is a selector. The view re-renders only when the selected value changes. |
| `useMemo` / reselect | `store.select_memo(cx, input, compute)` | `compute` runs only when `input` changes. |
| Context (`createContext`, `useContext`) | `GuiraffeTheme` (an app-wide global) and `StyleContext` (inherited down the tree with `provide(StylePatch, child)`) | Read them with `GuiraffeTheme::global(cx)` and `StyleContext::current()`. |
| React Router / TanStack Router | `guiraffe_router`: `routes!` declares a typed route enum, `Router<R>` navigates | Routes are enum values (`Page::Todo { id: 7 }`), so a typo in a path is a compile error. Paths still round-trip for deep links. |
| `useEffect` for subscriptions | `cx.subscribe(&entity, …)` / `cx.observe(&entity, …)`, kept as a `Subscription` field | Dropping the `Subscription` unsubscribes, like an effect's cleanup function. |
| `useEffect` for side effects / `setTimeout` | `fx.effect(E)` with a handler registered by `store.on_effect`, `fx.dispatch_later`, `fx.dispatch_debounced`; `cx.spawn` and `cx.background_executor()` for async work | Store handlers describe effects as data and stay pure, so you can test them without a window. |
| CSS, Tailwind | Tailwind-like methods on elements (`flex`, `gap_4`, `p_4`, `bg(..)`) plus theme tokens | No cascade. Colours come from the theme (`surfaces()`, palettes) and spacing from `Size` tokens. Layouts own the gaps between widgets ([spec/14-style-context.md](../spec/14-style-context.md)). |
| `prefers-color-scheme`, CSS variables | `Scheme::Light`/`Scheme::Dark`, `set_scheme`, `apply(GuiraffeTheme { .. })` | Applying a theme updates GPUI Controls too and refreshes every window. |
| `key` on list items | `for item in items key item.id { … }` in `ui!`, or element IDs | Keeps element state attached to the right row when a list reorders. |
| React DevTools, Chrome profiler | `guiraffe-devtools` (render counters, frame profiler, store time travel) and the showcase app | See [debugging](getting-started.md#debugging). |
| Electron main/renderer IPC | Nothing: one process, and your code calls the platform directly | Windows are managed in-process (`guiraffe-windows`). |

## How rendering differs

**Retained views, not a virtual DOM.** A React render returns a new tree that React diffs
against the previous one. A GPUI view is a long-lived `Entity`. Its `render` builds elements
for one frame, and GPUI lays them out and paints them directly. There's no reconciliation
step. State lives in the view struct, not in hook slots, so it survives as long as you hold
the `Entity`. A view isn't torn down because its parent stopped rendering it for a frame.
The comparison app relies on this: it keeps each page alive after the first visit, where the
React app unmounts it.

**`cx.notify()` is the only trigger.** Changing a field does nothing visible until you call
`cx.notify()` on that view's context. There's no automatic dependency tracking for plain
fields. Some rules from [spec/00-api-conventions.md](../spec/00-api-conventions.md) §6:

- Keep fast-changing state in small views. A parent holds an `Entity<Child>` handle rather
  than the child's data, so a change re-renders only the child.
- Notify only when the output actually changed, and never inside `render`.
- `Store::select` does the comparison for you. A selection notifies its view only when the
  selected value differs from last time.

**Two backends.** On upstream GPUI, notifying a view also re-renders its ancestors and their
uncached children. With GPUI Fast (`--features gpui-fast`), GPUI records what each view read
and reuses everything else. GUIraffe's render-isolation test measured an animating leaf
re-rendering its parent and siblings 12 times over 12 frames on upstream, and 0 times on
GPUI Fast. Apps and templates default to GPUI Fast. Library crates work on both.

**Virtualise anything long.** A React app reaches for `react-window` once a list gets big.
In GUIraffe it's the default for big collections. `DataGrid` builds only the visible rows
*and* columns, borrows cells from your data without copying, and sorts and filters by
producing a row permutation, on the background executor for large data. GPUI's
`uniform_list` and `list` do the same for simple lists. Example 4 in the guide shows 5,000
rows. The showcase grid computes 1,000,000.

**No per-frame allocation in hot paths.** Render functions run often. Use version counters,
indexes and reused buffers rather than rebuilding collections each frame. The `Arc` fields
in Example 6's `AppState` keep cloning (for undo) and comparing cheap.

## Side by side

A counter with a reusable display component, in React:

```tsx
function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="stat">
      <span className="dimmed">{label}</span>
      <h1>{value}</h1>
      {hint && <span className="dimmed">{hint}</span>}
    </div>
  );
}

export function Counter() {
  const [count, setCount] = useState(0);
  return (
    <div className="card">
      <Stat label="Count" value={String(count)} hint={count === 0 ? "Press +1 to start" : undefined} />
      <button disabled={count === 0} onClick={() => setCount(c => c - 1)}>−1</button>
      <button className="primary" onClick={() => setCount(c => c + 1)}>+1</button>
      <button className="ghost" onClick={() => setCount(0)}>Reset</button>
    </div>
  );
}
```

The same in GUIraffe, from
[02_counter.rs](../examples/getting-started/examples/02_counter.rs):

```rust
/// A stateless component: props in, elements out. `#[component]` generates
/// `Stat::new(label, value)` plus a builder method for each optional prop.
#[component]
fn Stat(
    label: &'static str,
    value: SharedString,
    #[prop(default)] hint: Option<&'static str>,
) -> impl IntoElement {
    ui! {
        div {
            flex flex_col items_center gap_1
            Text(label) { dimmed: true }
            Title(value) { order: 1 }
            if let Some(hint) = hint { Text(hint) { dimmed: true } }
        }
    }
}

/// The view owns its state. Rendering reads it; listeners mutate it.
#[derive(Default)]
struct Counter {
    count: i64,
}

impl Counter {
    fn change(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.count = self.count.saturating_add(delta);
        // Ask GPUI to render this view again on the next frame.
        cx.notify();
    }
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.count;
        ui! {
            div {
                size_full flex items_center justify_center
                Card {
                    div {
                        flex flex_col items_center gap_4 p_4
                        Stat("Count", format!("{count}").into()) {
                            hint: (count == 0).then_some("Press +1 to start")
                        }
                        div {
                            flex gap_2
                            Button("decrement") {
                                label: "−1"
                                disabled: count == 0
                                on_click: cx.listener(|this, _, _, cx| this.change(-1, cx))
                            }
                            Button("increment") {
                                label: "+1";
                                .primary()
                                on_click: cx.listener(|this, _, _, cx| this.change(1, cx))
                            }
                            Button("reset") {
                                label: "Reset";
                                .ghost()
                                on_click: cx.listener(|this, _, _, cx| {
                                    this.count = 0;
                                    cx.notify();
                                })
                            }
                        }
                    }
                }
            }
        }
    }
}
```

The differences:

- `Stat` is a `#[component]`. The optional `hint` prop gets a generated builder method that
  `ui!` calls as `hint: …`.
- `count` is a field on `Counter`, not a hook. The update helper calls `cx.notify()`. React
  re-renders automatically after `setCount`.
- Event handlers are `cx.listener(|this, event, window, cx| …)` closures. They get mutable
  access to the view, so there's no stale-closure problem and no functional `setState`.
- Styling is element methods (`flex gap_2`) and widget props (`.primary()`, `dimmed: true`)
  resolved against the theme, not class names and a stylesheet.

## GUIraffe vs Electron + React: measured

[examples/compare/](../examples/compare/README.md) contains the same app twice, once with
GUIraffe and once with Electron + React 19 (Mantine, TanStack Table and Virtual, Recharts):
a dashboard, a data grid, a Markdown document, a chat, a form and a command palette. The
harness in `bench/electron/` launches each app 20 times per scenario on the same machine
and reports medians. See [bench/electron/README.md](../bench/electron/README.md) for the
method and its caveats.

| | GUIraffe | Electron + React |
|---|---|---|
| Warm start to first frame | 156 ms (first launch of the series: 153 ms) | 346 ms |
| Idle memory (PSS, whole process tree) | 97.6 MiB | 304.1 MiB |
| Peak memory under load (grid fling + chat stream) | 151.2 MiB | 1,002.3 MiB |
| Installed size | 50.4 MiB | 275.6 MiB |
| Frame-interval p99 across scenarios, callback-entry timing | 17.1–17.4 ms | 18.1–21.9 ms |

Sources:

- GUIraffe startup, memory and size come from the release 0.2 run in
  [bench/electron/RESULTS-0.2-guiraffe.md](../bench/electron/RESULTS-0.2-guiraffe.md)
  (raw data in `bench/electron/results/2026-11-21_084034/`). That run measured only
  GUIraffe.
- Electron startup, memory and size come from the full two-app run on 2026-11-20
  (`bench/electron/results/2026-11-20_214939/`). That run is on `release/0.1`
  (commit `539a2ed`) and isn't part of this branch. `bench/electron/RESULTS.md` on this
  branch still says no valid measurement exists yet.
- Frame pacing uses the matched metric, where both apps sample a monotonic clock at
  callback entry (`performance.now()` inside Chromium's rAF, `Instant::now()` inside GPUI's
  `on_next_frame`). The 2026-11-20 Electron frame figures used the rAF timestamp argument
  instead, which is a different sampling point, so they aren't comparable and aren't
  quoted here. The raw data for the callback-entry rerun isn't committed to this branch yet.

Neither "cold" start is a true cold start: the machine had no passwordless sudo to drop the
page cache. Electron's startup marker comes after showing the window and two rAFs. GUIraffe's
comes at its first next-frame callback.
