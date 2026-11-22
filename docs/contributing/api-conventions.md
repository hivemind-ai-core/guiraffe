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

- **Widgets have no outer spacing.** Layouts own the gaps and padding between things.
- **Defaults come from the inherited `StyleContext`** (density, control size, surface, emphasis), which layouts
  and containers set from theme *recipes*. Per-instance props such as `.size()` and `.variant()` are **escape hatches**.


- No color or pixel constants in widget code. Everything comes from `GuiraffeTheme`: palettes,
  `Size` scales, the `Variant` resolver, `Surfaces`, shadow and motion tokens.
- Widgets implement `Styled` so callers can always override styles last.
- Per-component theme defaults (`theme.defaults::<Button>()`) are data and can be serialized.
- Light and dark schemes are both first-class. Every widget has a visual snapshot in both (§10).

## 6. Rendering performance rules (must-follow)

**Measured** (guiraffe-motion's render-isolation test, a single animating leaf for 12 frames):
**upstream GPUI re-rendered the parent and sibling views 12 times each; gpui-fast re-rendered them 0 times.**
On upstream, notifying a child re-renders its ancestors and their uncached children. `AnyView::cached` only helps for
views with a definite size. gpui-fast records what each view read and reuses everything else.

**Backend policy:**
- Library crates stay backend-neutral. Each one exposes a `gpui-fast` feature that forwards `gpui-kit/gpui-fast`.
- **Apps, templates and the gallery enable gpui-fast by default.**
- CI runs the whole workspace on both backends. As of 2026-11-16 all 176 tests pass on gpui-fast.

These rules make the most of both backends:

1. **Granular views.** State that changes often (hover, an animating value, a streaming message,
   the toast list, the flow viewport) lives in a *small* entity whose view renders only what depends on it.
   Parents hold `Entity<Child>` handles, not the child's data.
2. **Notify narrowly, and notify *purely*.** To re-render another view, use `cx.notify(entity.entity_id())`,
   **not** `entity.update(cx, |_, cx| cx.notify())`. On gpui-fast an `update` counts as a *write*, which invalidates every
   view that rendered that entity, including its ancestors. This was measured in `guiraffe-ui`: the root re-rendered on every patch until the fix.
   Use `update` only when you really mutate the entity.
    Call `cx.notify()` only on the entity whose output changed, and only when it
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

## 12. Interactive controls: feedback, focus, accessibility

1. **Build on Kit controls.** Clickable/editable GUIraffe widgets wrap or compose gpui-kit (GPUI Controls) primitives where one exists (Button, Input, Checkbox, Switch, Select, …). A bare `div().on_click` is not a button.
2. **Full state feedback, Kit conventions.** Every interactive element shows hover, active/pressed, focus-visible, disabled (no hover/press, `not-allowed` cursor, dimmed, events suppressed), and loading/selected where applicable, using the same tokens and timings as Kit controls. Cursor reflects affordance (pointer, text, resize, grab).
3. **Focus is opt-in for non-text controls.** Pointer clicks on buttons, segments, chips, icons etc. must not take or keep keyboard focus by default. Text inputs (keyboard-grabbing controls) take focus on click. Focusability is enabled per control (`.focusable(true)`) or for a subtree by a form wrapper (`Form`/`FocusScope`) which opts all children into Tab navigation and focus rings. Focus rings show only for keyboard focus (focus-visible).
4. **Accessibility.** Every control exposes role, accessible name/label, state (checked, selected, expanded, disabled, value/range) and keyboard operation (Space/Enter activate, arrows within groups, Escape dismisses) through GPUI's accessibility support where available; gaps are tracked in KNOWN_ISSUES. Never convey state by color alone.
5. **Tests.** Each control has interaction tests for: click without focus capture, disabled suppression, keyboard activation inside a form scope, and (once supported) its accessibility node.

### 12.1 State styling is semantic, not a fixed tweak

"Dimmed", "disabled", "hover", "pressed", "selected" and "focus ring" are **theme tokens resolved per scheme and theme**, never hard-coded opacity or a fixed shade:
- Resolved by `GuiraffeTheme` for the active scheme (light: darken/desaturate toward text; dark: lighten/reduce contrast toward surface), primary color and variant — e.g. `theme.state(StateKind::Disabled, variant)` → fg/bg/border.
- **Contrast floors (WCAG 2.2):** dimmed/secondary text ≥ 4.5:1 against its actual background; disabled content distinguishable yet clearly inactive (target ≥ 3:1 for control boundaries, disabled text exempt by WCAG but kept legible ≥ 3:1 in our themes); focus indicator ≥ 3:1 against adjacent colors. Tests compute contrast for every token in both schemes and every primary color.
- **Respect OS/user preferences:** high-contrast mode (stronger borders, no low-contrast dimming), reduced motion (state transitions instant), and forced colors where the platform reports them; a theme `contrast` setting (standard/high) feeds the same resolver.
- Never convey state by opacity or color alone: disabled also changes cursor, suppresses interaction and is exposed to accessibility; selected also has a shape/weight cue.
- Overlays on top of content (LoadingOverlay, modals) dim via a scheme-aware scrim token, not raw black alpha.

### 12.2 Focus rings never change layout
- The focus indicator is **separate from the control's own border**: a control can have a border *and* a focus ring at the same time.
- The ring is an **overlay outline**, painted outside (or inset over) the control's box, **not part of layout**: no size, padding or position change between focused/unfocused; nothing around it moves.
- One shared helper renders it (absolute-positioned, `inset: -offset`, `pointer-events: none`, theme `state(FocusRing)` color, width 2px, offset 2px, radius = control radius + offset), so every control looks and behaves the same.
- State borders in general (hover/selected/error) must also not change size: border width is constant across states (color changes only); a state that needs a thicker line uses an overlay.
- Test: a control's laid-out bounds and its siblings' bounds are identical focused vs. unfocused and across hover/selected states.
