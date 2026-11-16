//! GUIraffe showcase: every component, live, searchable, in light and dark.
//!
//! Run with `cargo run -p guiraffe-showcase` (add `--features gpui-fast` for the recommended
//! backend). `tests/coverage.rs` keeps the catalog complete.

pub mod catalog;
pub mod demos;
pub mod shell;

/// Initialize every crate the showcase demonstrates (idempotent).
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    guiraffe_layout::init(cx);
}
