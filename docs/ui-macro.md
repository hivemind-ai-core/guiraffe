# Writing GPUI views with `ui!`

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

Run the team tasks demo with `cargo run -p ui-macro-demo --features gpui-fast`.
