//! The GUIraffe side of the Electron comparison (plans/ELECTRON-COMPARISON.md); the binary
//! is `src/main.rs`. Contract shared with the Electron app: `examples/compare/README.md`.

pub mod bench;
pub mod data;
pub mod pages;
pub mod shell;
pub mod validate;

/// Initialize every crate the app uses (idempotent).
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    guiraffe_overlays::init(cx);
    guiraffe_chat::init(cx);
    guiraffe_datagrid::init(cx);
    shell::init(cx);
}
