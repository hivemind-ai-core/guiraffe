//! GUIraffe showcase: every component, live, searchable, in light and dark.
//!
//! Run with `cargo run -p guiraffe-showcase` (add `--features gpui-fast` for the recommended
//! backend). `tests/coverage.rs` keeps the catalog complete.

pub mod catalog;
pub mod demos;
pub mod events;
pub mod knobs;
pub mod recipes;
pub mod shell;

/// The showcase window's root view: the shell inside an overlay root, so toasts, modals
/// and the command palette have a layer to open in.
pub fn window_root(
    start: Option<&str>,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Entity<gpui_kit::base::Root> {
    use gpui_kit::AppContext as _;
    let view = cx.new(|cx| shell::Showcase::new(start, window, cx));
    guiraffe_overlays::root(view, window, cx)
}

/// Showcase root for snapshots with a stable header across catalog additions.
pub fn snapshot_window_root(
    start: Option<&str>,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Entity<gpui_kit::base::Root> {
    use gpui_kit::AppContext as _;
    let view = cx.new(|cx| shell::Showcase::new(start, window, cx).with_static_header());
    guiraffe_overlays::root(view, window, cx)
}

/// Initialize every crate the showcase demonstrates (idempotent).
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    guiraffe_layout::init(cx);
    guiraffe_overlays::init(cx);
    guiraffe_chat::init(cx);
    guiraffe_inputs::init(cx);
    guiraffe_timeline::init(cx);
    guiraffe_datagrid::init(cx);
    // Chart overlays live in a window-root layer so hover never re-renders the host.
    guiraffe_charts::init(cx);
    guiraffe_flamegraph::init(cx);
    shell::init(cx);
}
