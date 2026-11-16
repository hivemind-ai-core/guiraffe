# App framework: state, router, query, form, machine, commands

## guiraffe-state (implemented, re-frame inspired)

**Taken from re-frame:**
- Events are data, processed run-to-completion in FIFO order.
- Handlers are pure functions of (state, event, coeffects). They return a new state and **effects as data**, and registered effect handlers execute those effects. So a handler is tested by feeding an `Engine` events, with no GPUI and no mocks.
- Coeffects are injected inputs (the clock, configuration, fakes in tests).
- Interceptors observe every event, for logging, devtools and validation.
- Layered subscriptions: `select` is a cheap extractor; `select_memo` adds an input stage plus an expensive computation that runs only when the input changes.

**Changed for Rust and GPUI:**
- Events are typed, one type per feature module, instead of keyword ids. They're checked at compile time and there's no giant enum.
- State is held in an `Arc` with clone-on-write, and must be `Clone + PartialEq`. A no-op `update` therefore isn't a change: it creates no undo step and notifies no view.
- Dispatches in one tick are **batched** into one flush, so a view re-renders at most once.
- Selections are tiny GPUI entities. A view observes only its own selections, which on gpui-fast means fully isolated re-renders.
- Undo/redo is built in, using `Arc` snapshots that are shared rather than deep-copied.
- A cascade limit turns accidental dispatch loops into a reported error instead of a hung UI.

**Next:**
- A subscription registry (dedup by key).
- An `EventLog` interceptor for devtools time-travel.
- Async effects: futures whose results dispatch events.
- Integration with guiraffe-query and the router.

## guiraffe-router
- `#[derive(Route)] enum AppRoute { Home, Project { id: ProjectId, #[search] tab: Tab }, ... }`
  with path patterns via attributes. Matching and formatting are pure and property-tested (round-trips).
- Nested layouts: a route tree where each layout renders an `Outlet`.
  Loaders (via guiraffe-query) and guards (redirect, block-with-confirm, which pairs with window close guards).
- **History per window.** `navigate(window, route)`, back and forward, history persisted with window layout.
  `open_in_new_window(route)` and `WindowKey` derive from routes, so a route can be a singleton window.
- Route transitions use guiraffe-motion presets. Pending UI follows loader state.
- Deep links (`myapp://project/42`) are dispatched to the right window.

## guiraffe-query
- `use_query(key, fetcher)` returns a `Resource<T>` entity. Supports caching, stale time, background refetch,
  retries with backoff, cancellation on drop, invalidation by key prefix, optimistic mutations with rollback,
  and window-focus refetch. The cache policy is pure and tested with a fake clock.

## guiraffe-form
- `Form<T>` with field paths and binding into inputs (`TextInput::new(&state).bind(form.field(|f| &mut f.email))`).
  Validation is sync, async or schema-based. Tracks dirty, touched and errors. Submit runs as a query mutation.

## guiraffe-machine
- Typed statecharts: `machine! { Idle -> Dragging on PointerDown if over_node; ... }`.
  Hierarchical and parallel states, entry/exit actions, guards, delayed transitions on the GPUI executor.
- Pure core: transitions are tested exhaustively, with generated reachability and "every state handles Esc" checks.
- Used internally by flow (select/pan/connect/box-select modes) and overlays (open/closing/closed)
  so the patterns get real use.
- A devtools panel shows the live state and the transition log.

## guiraffe-commands
- A single registry of `Command { id, title, keybinding, when, enabled, run }`.
- Feeds menus (native and in-app), toolbars, Spotlight, context menus and the keymap editor.
  `when` contexts reuse GPUI key contexts.
