#![doc = include_str!("../README.md")]
//! Read-only code surfaces. Source and analysis are immutable snapshots; rendering and
//! interactive overlays live in independent leaf entities. See the stress example.
mod decorations;
mod document;
mod highlight;
mod model;
pub use decorations::*;
pub use document::*;
pub use highlight::*;
pub use model::*;

mod markdown;
pub use markdown::code_fences;
mod paint;
mod surface;
pub use surface::*;
/// Idempotently initialize theme and the Controls bridge.
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<gpui_kit::component::Theme>() {
        gpui_kit::component::init(cx);
    }
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::apply(Default::default(), cx);
    }
}

/// Long logical lines are previewed up to this many UTF-8 bytes; source copy stays complete.
pub use paint::MAX_SHAPED_LINE_BYTES;
