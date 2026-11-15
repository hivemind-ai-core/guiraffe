# GUIraffe API conventions

Every crate follows these rules, so the toolkit feels like one library. Reviews check them.
When a rule doesn't fit, change this file first, then the code.

## 1. Three kinds of building blocks

| Kind | What it is | GPUI shape | Example |
|---|---|---|---|
| **Component** | stateless, cheap, built every render | `#[derive(IntoElement)] struct X` + `impl RenderOnce` | `Badge`, `Card`, `Stack`, `ThemeIcon` |
| **Control** | holds interaction state that outlives a frame | `Entity<XState>` (owns state, `EventEmitter<XEvent>`) + `X::new(&state)` element | `SegmentedControl`, `TagsInput`, `Splitter`, `FlowCanvas` |
| **Service** | app- or window-wide manager | `Global` for configuration + `Entity` for changing state, reached through free functions | `WindowManager`, `modals`, `notifications`, `Spotlight` |

This is the same split gpui-component uses (`InputState` / `Input`), so Controls widgets and ours combine naturally.
A Component never hides an `Entity` inside itself. When something needs state, it is a Control.

## 2. Construction and props

- `X::new(id)` for anything interactive (`impl Into<ElementId>`), `X::new()` otherwise. A Control takes its state: `X::new(&state)`.
- Builder methods take `self` and return `Self`, named after Mantine's props: `.size(Size::Md)`,
  `.variant(Variant::Light)`, `.color(ColorName::Teal)`, `.radius(Size::Xl)`, `.disabled(true)`, `.loading(true)`.
- **Shared prop traits** in `guiraffe-core::props`, implemented the same way everywhere. gpui-component's
  traits are reused where they exist (`Sizable`, `Disableable`, `Selectable`) and ours fill the gaps:
  `WithVariant`, `WithColor`, `WithRadius`, `WithTransition`, `TestId`.
- **Controlled and uncontrolled** state works as in Mantine: `.value(v)` together with `.on_change(..)` means
  the caller owns the value. `.default_value(v)` means the Control owns it. Never both.
- Every visual prop has a theme default (`theme.defaults::<X>()`, §5). Leaving a prop unset means "use the theme".
- **Props are data.** Each widget's props live in a `XProps` struct deriving
  `Clone, PartialEq, serde, schemars`. The builder methods, the data-driven UI (IR) and codegen all go through
  that one struct. A derive macro (`guiraffe-macros`, `#[derive(Props)]`) generates the builder methods,
  the JSON Schema entry and the IR registration, so the three can't drift apart.

## 3. Events and callbacks

- Components take closures: `on_click`, `on_change`, `on_close`, `on_open_change`, `on_submit`,
  with the signature `impl Fn(&Payload, &mut Window, &mut App) + 'static`.
- Controls emit typed events (`cx.emit(XEvent::Changed(..))`) that callers subscribe to,
  and also offer the same closures for convenience.
- Names match across crates: `open` / `on_open_change` for every popover-like widget,
  `value` / `on_change` for every input.
- Keyboard: each crate registers its actions and key bindings in `init(cx)` under a documented
  key context (`GrModal`, `GrSpotlight`, `GrFlow`, ...). Apps can override all of them.

## 4. Initialization and features

- Every crate has `pub fn init(cx: &mut App)`. It is idempotent and registers actions, key bindings and globals.
  The umbrella crate's `guiraffe::init(cx)` calls them all, and core's init runs first.
- Optional dependencies sit behind cargo features (`markdown/mermaid`, `overlays/desktop`, `e2e`, `devtools`).
- No crate reads environment variables or files implicitly. Paths come in through config structs.

## 5. Styling

- No color or pixel constants in widget code. Everything comes from `GuiraffeTheme`: palettes,
  `Size` scales, the `Variant` resolver, `Surfaces`, shadow and motion tokens.
- Widgets implement `Styled` so callers can always override styles last.
- Per-component theme defaults (`theme.defaults::<Button>()`) are data and can be serialized.
- Light and dark schemes are both first-class. Every widget has a visual snapshot in both (§10).

## 6. Rendering performance rules (must-follow)

GPUI re-renders a view when its entity is notified. gpui-fast also reuses a view's last frame
when nothing it read has changed. Both reward the same structure:

1. **Granular views.** State that changes often (hover, an animating value, a streaming message,
   the toast list, the flow viewport) lives in a *small* entity whose view renders only what depends on it.
   Parents hold `Entity<Child>` handles, not the child's data.
2. **Notify narrowly.** Call `cx.notify()` only on the entity whose output changed, and only when it
   actually changed: compare before notifying, and skip it when an update is a no-op. Never notify inside `render`.
3. **No fast-changing state in widely read globals.** Reading a global in `render` makes the view depend on it.
   Configuration (theme, keymaps) may be global. Live state (open overlays, window bounds, notifications)
   goes in entities observed only by the views that show it.
   *Known debt:* `WindowManager` keeps its layout in its Global. It will be split into an entity before gpui-fast is adopted.
4. **Cache heavy subtrees** with `AnyView::cached(style)` when the parent re-renders often.
   This works on upstream GPUI too.
5. **Animate in leaves.** An animation drives one leaf view through `request_animation_frame`,
   and only while it is moving. It costs nothing at rest and never re-renders the page.
6. **Virtualize** anything that can exceed a screenful (`uniform_list`, `list`, our virtual grid).
7. **No per-frame allocation or O(n) scans** in render or paint for large collections.
   Use indexes, version counters and reused buffers.
8. **Backend parity.** CI builds and tests on upstream GPUI and on `gpui-fast` (gpui-kit `gpui-fast` feature, ≥ 0.8).
   A render-count test helper (`guiraffe-test::RenderCounter`) asserts that unrelated changes don't re-render a view.
   Every Control has at least one such test.

## 7. Errors and robustness

- No `unwrap`, `expect`, or panicking indexing in library code. Clippy enforces `unwrap_used`.
- Fallible APIs return `Result<T, guiraffe_x::Error>` (thiserror). Bad props are clamped to valid ranges
  rather than panicking, with a `debug_assert!` so developers still notice.
- Background work (parsing, layout, IO) runs off the main thread. Panics in third-party parsers are caught
  at the crate boundary and shown as an error state.

## 8. Motion

- Every widget that appears or disappears takes `.transition(Transition)`, with the theme default as fallback.
  Durations and easings come from motion tokens, and reduced motion disables them globally.
- Exit animations go through `Presence`. Widgets never delay their own removal by hand.

## 9. Persistence

One `guiraffe-core::persist` store (atomic writes, versioned, sets corrupt files aside) holds window layout,
splitter sizes, collapsed panels and recent spotlight items. Each widget opts in with `.persist_key("...")`.

## 10. Definition of done (per widget)

1. Rustdoc with a runnable example.
2. Unit tests for the pure logic.
3. Interaction tests (`guiraffe-test`).
4. A render-count test (Controls).
5. A gallery story covering variants, sizes and states.
6. Visual snapshots in light and dark.
7. `XProps` registered for the IR, with a Mocket mapping entry if Mocket has an equivalent.
8. clippy and fmt clean.
