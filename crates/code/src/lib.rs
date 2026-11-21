#![doc = include_str!("../README.md")]
//! Read-only code surfaces. Source and analysis are immutable snapshots; rendering and
//! interactive overlays live in independent leaf entities. See the stress example.
//!
//! With a `CodeHandle` named `handle`, mount a code view with `ui!`.
//!
//! ```ignore
//! ui! { CodeViewer(&handle) }
//! ```
mod data_ui;
use guiraffe_core::widget_spec;
mod decorations;
mod document;
mod highlight;
mod model;
mod widget_specs;
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
    data_ui::register(cx);
    if !cx.has_global::<gpui_kit::component::Theme>() {
        gpui_kit::component::init(cx);
    }
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::apply(Default::default(), cx);
    }
}

/// Long logical lines are previewed up to this many UTF-8 bytes; source copy stays complete.
pub use paint::MAX_SHAPED_LINE_BYTES;
