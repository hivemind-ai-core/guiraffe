# `ui!`: a declarative view DSL

Goal: write views the way JSX/QML lets you — a tree that reads like the UI — while compiling to the
exact builder calls a person would write by hand (zero runtime cost, full type checking, normal
interop). Crate: `crates/macros` (`guiraffe-macros`, proc-macro) re-exported as `guiraffe_ui::ui!`
and by the umbrella prelude. Owner: codex. Reviewer/gate: foreman.

## 1. Syntax

```rust
ui! {
    Stack {
        gap: Size::Md
        Title("Settings") { order: 2 }
        Text { "Signed in as " {user.name} }
        Group {
            Button("save") { label: "Save"  variant: Variant::Filled  on_click: move |_, _, cx| save(cx) }
            Button("cancel") { label: "Cancel"  disabled: busy }
        }
        if let Some(err) = &error { Alert { color: ColorName::Red  {err.clone()} } }
        for row in &rows key row.id { RowView(row) }
        match tab { Tab::A => Text { "A" }, Tab::B => Text { "B" } }
        div { flex  gap_2  p: px(8.)  .when(dense, |d| d.p(px(2.)))  "raw div" }
        {already_built_element}
        view(self.chart.clone())
        cached(StyleRefinement::default().w(px(480.)).h(px(240.))) { HeavyTable(&data) }
    }
}
```

Grammar (whitespace/newline separated; optional `,` or `;` between items are accepted and ignored):

| Item | Meaning | Expansion |
|---|---|---|
| `Path` / `Path(args)` (first segment's last ident starts uppercase) | element | `Path::new(args)` |
| `path(args)` lowercase function followed by `{` or `(` | element from a constructor fn (`div`, `h_flex`, `svg`) | `path(args)` |
| `… { block }` after an element | its props/methods/children | applied in order |
| `name: expr` | prop | `.name(expr)` |
| bare lowercase `name` (not followed by `(`/`{`/`:`) | flag | `.name()` |
| `.method(args)` / `.method::<T>(args)` | raw builder call, verbatim | `.method(args)` |
| `"literal"` | text child | `.child("literal")` |
| `{ expr }` | any `IntoElement` child | `.child(expr)` |
| `if c { … } else if … { … } else { … }`, `if let` | conditional children | `.children(Option)`/branches unified (§3) |
| `match e { pat => item, … }` | one child per arm | unified (§3) |
| `for pat in iter [key expr] { … }` | repeated children | `.children(iter.map(..))` |
| `view(entity)` | embed a child `Entity<V: Render>` | `.child(entity)` |
| `cached(style) { … }` | cached boundary: body goes into a generated view or `AnyView::cached` | §4 |

Values are full Rust expressions parsed without eager struct literals (so `value Child {}` ends the
value at `value`). Children keep source order relative to props: a prop after a child applies to the
same element (builder order is preserved exactly; elements whose builder requires props first are
the author's concern, same as handwritten code).

The root may be a single element (returns that type, `impl IntoElement`) or several items, which
are wrapped in a `Fragment` (a zero-cost `div()`-less helper that flattens into the parent via
`.children`; if GPUI can't express that, the macro returns `Vec<AnyElement>` and documents it).

## 2. Diagnostics (part of the deliverable)
- Every expansion carries the source span, so type errors point at the user's tokens
  (`label: 3` → "expected `impl Into<SharedString>`" on `3`).
- Parser errors are specific: unknown item, missing `}`, prop without value, `key` outside `for`,
  `else` without `if`. Use `syn::Error::new_spanned` and `trybuild` UI tests for each.
- Unknown prop names surface as rustc's "no method named `x`" on the prop ident, with a hint
  note listing valid props when the element type implements `guiraffe_core::props::Describe`
  (catalogue hook; optional in v1, see §6).

## 3. Control flow and type unification
Branches producing different element types are unified with `.into_any_element()` only when
needed (detected syntactically: different element heads); otherwise the concrete type is kept.
`if` without `else` becomes an `Option` child. `for` uses `.children(iter.into_iter().map(..))`.
No allocation beyond what the equivalent handwritten builder does (verify with the allocator gate
on a bench: `ui!` vs handwritten tree of 1k nodes ⇒ identical allocation counts).

## 4. Retained-mode / fine-grained re-render features
These make the spec/00 §6 performance rules easy to follow in the DSL:
1. **`key` on `for`** → each item gets `ElementId` = (parent id, key) so state (hover, focus,
   scroll) follows the item across reorders. Required for stateful children; lint (compile error)
   when a `for` body contains an interactive element with an id but no `key`.
2. **`view(entity)`** embeds a child view: the child re-renders alone when it notifies.
3. **`cached(style) { … }`** wraps the body in `AnyView::cached(style)` via a generated
   view entity keyed by call site; useful for heavy subtrees under frequently re-rendering parents.
   Tests prove with `RenderCounter` that a parent notify doesn't re-render the cached body.
4. **`#[component]`** attribute (same crate): `#[component] fn UserCard(name: SharedString, #[prop(default)] compact: bool) -> impl IntoElement { ui!{…} }`
   generates `struct UserCard { … }` + `UserCard::new(required…)` + builder methods for optional
   props + `RenderOnce`, so user components are used in `ui!` exactly like built-ins.
   Generated props struct derives `Clone, PartialEq`. (Later: `#[component(stateful)]` →
   Control with an entity, out of scope v1.)

## 5. Tests and demo (definition of done)
- Parser unit tests (token stream → AST) and expansion snapshot tests (`ui!` → pretty-printed
  Rust, insta or golden files) for every grammar row.
- `trybuild` compile-fail tests for each diagnostic in §2 with checked `.stderr`.
- GPUI `TestAppContext` tests: rendered `ui!` tree equals the handwritten builder tree
  (debug bounds equal for a representative page); `key` preserves per-item state across a reorder;
  `cached` and `view` render-isolation via `RenderCounter` on both backends.
- Allocation parity bench (§3) in `guiraffe-bench` with an allocator test.
- **Demo app** `examples/ui-macro-demo` (binary): a small "team tasks" app written entirely with
  `ui!` and `#[component]` — sidebar navigation, a filterable task list (`for … key`), a task
  detail form (inputs, validation state), a stats card with a cached chart, theme toggle.
  Headless snapshot goldens of its pages, light + dark, both backends.
- One showcase entry ("ui! macro") showing source next to the rendered result.
- Docs: crate rustdoc with examples (doctests compile), a `docs/ui-macro.md` guide, and migrate
  one existing showcase demo to `ui!` as a real-world proof (keep its goldens identical).

## 6. Out of scope for v1 (note in ROADMAP)
Hot reload of `ui!` bodies, catalogue-validated prop hints, `#[component(stateful)]`, IDE plugin.

### Grammar decisions

The final segment of a qualified path determines whether to call `::new`. A bare lowercase
name is a flag inside a builder body. Use `name()` or `name { … }` for a lowercase constructor.
Separate a flag from a following braced expression child with a comma or semicolon.
Place a comma or semicolon before a raw builder call after a prop if Rust could parse the call as
a postfix expression on the prop value. Parenthesize struct literals in prop values and control
headers. Matching Rust delimiters are checked by the compiler before the macro can parse a body.

Multiple roots and root-level spliced control flow return `Vec<AnyElement>` for use with
`.children(…)`. A multi-child keyed row or cached body has an implicit `div()` layout root.
A single-child keyed row retains the child's layout and scopes its element IDs under the key.
The unkeyed-loop lint checks explicit IDs and constructors of known interactive controls.
Rust expressions, custom constructors and entities can hide state that this syntax check cannot
see. Supply a key for those rows as well.

`cached` takes a GPUI `StyleRefinement` with definite dimensions. Its body is an owned
repeatable closure. Capture stable inputs and let an entity inside the boundary own changing data.
Parent notifications can reuse a cached paint, so a change to a captured value alone does not
invalidate the previous paint. Place repeated caches under their keyed rows.
