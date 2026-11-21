//! Virtualized tree views with a shared visible-row and navigation model.
mod geometry;
mod model;
mod property;
mod rows;
mod search;
pub use model::*;
pub use property::*;

mod view;
pub use view::*;

use guiraffe_core::widget_spec;
mod widget_specs;

/// Register the tree list and property grid for document authoring.
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::init(cx);
    }
    view::document::register(cx);
}
