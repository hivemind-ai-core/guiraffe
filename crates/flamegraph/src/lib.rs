//! Indexed flame and icicle graphs over borrowed profile data.
//!
//! A Rust `ui!` example. Create shared entities before rendering when the host owns their state.
//!
//! ```ignore
//! ui! { div { {cx.new(|_| FlameGraph::new(layout.clone()))} } }
//! ```

mod model;
pub use model::*;

mod view;
pub use view::*;

use guiraffe_core::widget_spec;
mod widget_specs;

/// Register the flame and icicle plot for document authoring.
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::init(cx);
    }
    view::init(cx);
    view::document::register(cx);
}
