# App framework: state, router, query, form, machine, commands

## guiraffe-state
- `Store<T>` is a GPUI entity. Writes go through `store.update(cx, |s| ..)` and actions.
- **Selectors**: `store.select(cx, |s| s.items.len())` subscribes the calling view to a projection.
  The view is notified only when the projection changes (`PartialEq`). This is the main defence against
  unnecessary re-renders in app code.
- Derived stores (`computed`). Middleware: undo/redo (structural sharing), persist (through the core persist
  store), logger, devtools time-travel.
- Per-window or app-wide scopes. Tests use `Store::test(..)` and assert notify counts.

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
