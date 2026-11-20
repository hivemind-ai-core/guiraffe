# Writing GPUI views with `ui!`

## Start a small desktop tool

From the GUIraffe checkout, run the [counter example](../examples/ui-macro-demo/examples/quickstart.rs).

```sh
cargo run -p ui-macro-demo --example quickstart --features gpui-fast
```

For your own app, create a Rust binary in a sibling directory and point it at your GUIraffe
checkout. Adjust the dependency path to match your layout.

```sh
cargo new my-tool
cd my-tool
```

Add the dependencies to `Cargo.toml`.

```toml
[dependencies]
gpui-kit = { version = "=0.9.1", features = ["component", "assets", "gpui-fast"] }
guiraffe-ui = { path = "../anteater/crates/ui" }
```

Copy the counter example into `src/main.rs` and run `cargo run`. The `Counter` struct
owns the app's state. Its `Render` implementation reads that state and builds the view.
`cx.listener` gives a click handler mutable access to the struct, and `cx.notify()` schedules
a redraw after the state changes. Use `#[component]` for reusable pieces that only need props.

For a tool with editable input, filtering and more than one page, run the team tasks demo.

```sh
cargo run -p ui-macro-demo --features gpui-fast
```

## Macro syntax

Import `guiraffe_ui::{component, ui}` and the builders and traits you'd normally use with
GPUI. Set a property with `name: value`, invoke a no-argument method with a flag, or call
any builder method with `.method(args)`. Text literals and expressions in braces are children.
Use commas or semicolons when a Rust expression could consume the next DSL token.

```rust
use gpui_kit::{IntoElement, div, prelude::*};
use guiraffe_ui::{component, ui};

#[component]
fn TaskLabel(title: &'static str, #[prop(default)] done: bool) -> impl IntoElement {
    ui! {
        div {
            flex gap_2;
            {title}
            if done { "Complete" }
        }
    }
}

let task = ui! { TaskLabel("Review keyboard navigation") { done: true } };
```

The attribute generates `TaskLabel::new(title)`, a `done(bool)` builder, and GPUI's
`RenderOnce` implementation. Optional props use their type's `Default` implementation.
Component prop types support `Clone` and `PartialEq`.

Conditional branches and match arms with the same constructor keep their concrete type.
Different constructors use GPUI's `IntoElement::into_any_element`. Multiple roots
produce a `Vec<AnyElement>` for a parent's `.children(…)`.

```rust,ignore
ui! {
    div {
        flex flex_col
        for task in tasks key task.id {
            TaskLabel(task.title) { done: task.done }
        }
        view(task_details.clone())
        cached(StyleRefinement::default().w(px(320.)).h(px(100.))) {
            CompletionChart()
        }
    }
}
```

Use stable, unique row keys so GPUI keeps element state attached to the task when the list
changes order. `view(entity)` embeds an existing entity. The backend controls how much of the
tree draws on a notification. `cached(style)` uses a retained GPUI view with a repeatable
`move` closure. Give it definite dimensions and put changing inputs in their own child entity,
since a parent notification can reuse the cached paint.

Each `cached` call site gets a separate retained view under each key. The macro rejects
`cached` inside an unkeyed `for`. Render repeated instances of a `#[component]` containing
`cached` in distinct keyed scopes as well. Inside a loop, use `for … key …`; for individually
written siblings, give each an enclosing element with a distinct GPUI ID.
