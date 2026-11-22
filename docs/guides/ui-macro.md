# Writing GPUI views with `ui!`

## Start a small desktop tool

From the GUIraffe checkout, run the [counter example](../../examples/getting-started/examples/00_quickstart.rs).

```sh
cargo run -p getting-started-examples --example 00_quickstart --features gpui-fast
```

For your own app, `guiraffe new` is the shortest route because it copies the vendored patch
table and the lockfile. To do it by hand, create a Rust binary in a sibling directory and point it at your GUIraffe
checkout. Adjust the dependency path to match your layout.

```sh
cargo new my-tool
cd my-tool
cp ../guiraffe/Cargo.lock .
```

Add the dependencies and the checkout's patch table to `Cargo.toml`. An app outside the
workspace does not inherit the patches, and does not compile without them.

```toml
[dependencies]
gpui-kit = { version = "=0.9.1", features = ["component", "assets", "gpui-fast"] }
guiraffe-components = { path = "../guiraffe/crates/components" }
guiraffe-ui = { path = "../guiraffe/crates/ui" }

[patch.crates-io]
gpui-fast = { path = "../guiraffe/vendor/gpui-fast" }
gpui-fast-linux = { path = "../guiraffe/vendor/gpui-fast-linux" }
gpui-pre = { path = "../guiraffe/vendor/gpui-pre" }
gpui-base = { path = "../guiraffe/vendor/gpui-base" }
gpui-component = { path = "../guiraffe/vendor/gpui-component" }
mermaid-rs-renderer = { path = "../guiraffe/vendor/mermaid-rs-renderer" }
taffy = { path = "../guiraffe/vendor/taffy" }
```

Copy the counter example into `src/main.rs` and run `cargo run`. The `Counter` struct
owns the app's state. Its `Render` implementation reads that state and builds the view.
`cx.listener` gives a click handler mutable access to the struct, and `cx.notify()` schedules
a redraw after the state changes. In `main`, `guiraffe_components::init` installs the theme and
`Root` paints its background and text color, so wrap each window's view in a `Root`.
Use `#[component]` for reusable pieces that only need props.

For a tool with editable input, filtering and more than one page, run the team tasks demo.

```sh
cargo run -p team-tasks --features gpui-fast
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
