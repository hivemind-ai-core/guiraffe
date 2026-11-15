# guiraffe-overlays

Overlays built on `gpui-kit =0.9.1`, with pure logic in `position`, `search`,
`stack`, and `toast`. See [INVENTORY.md](INVENTORY.md) for the upstream audit.

Call `guiraffe_overlays::init(cx)` before creating windows. Kit's `open_window`
mounts its Root automatically. For `guiraffe-windows`, use:

```rust,ignore
WindowManager::open(key, OpenMode::Reuse, options, |window, cx| {
    let content = cx.new(|cx| MyView::new(window, cx));
    guiraffe_overlays::root(content, window, cx)
}, cx)?;
```

`root` returns an existing Root unchanged, so wrapping is idempotent. State and
pending confirmation futures belong to each window and are released with it.
Toast progress ticks notify individual native card entities, without notifying the
overlay Root plugin. Native Controls owns toast insertion/removal and transitions.

- `OverlayHost::open`: stacked dialogs with managed IDs and policy.
- `OverlayHost::confirm`: returns `(id, result)`; await `result` for a boolean.
  Cancel, programmatic close, or window destruction resolves `false`.
- `register_context` / `open_context`: window-local named modal factories.
- `Drawer::open`: Controls Sheet on any of four sides, with token sizing.
- `OverlayHost::notify`: insert or replace a `ToastId`; limit, FIFO queue,
  six positions, hover pause, progress, persistent or timed dismissal.
- `Spotlight::new` / `Spotlight::open`: fuzzy filtering, arrows, Enter, pointer
  selection, disabled commands and keyword matching.
- `LoadingOverlay`, `Affix`: local container overlays using theme tokens.
- `Popover`, `HoverCard`, `Tooltip`, `ContextMenu`, `ContextMenuExt`,
  `PopupMenu`, `DropdownMenu`: Controls primitives with the shared theme bridge.
- `desktop::DesktopNotification` (`desktop` feature): optional notify-rust
  delivery; run its blocking `show` on a background executor. Errors are returned.

Managed layers must be opened and closed through this crate. Native Controls
window methods remain available for independent usage but bypass managed IDs.
Only the top managed overlay can close: this preserves Controls' focus chain.
Controls owns focus trapping/restoration, scroll/pointer occlusion, and transitions.
All managed layers are modal and retain Controls' intrinsic scroll/pointer
occlusion. A Drawer opens only
when the managed stack is empty (Controls supports one sheet); dialogs may then
stack above it. Builder close callbacks are reserved for manager bookkeeping.

The standalone placement solver returns coordinates and a cross-axis arrow offset;
Controls wrappers retain their own native positioning. Desktop delivery is tested
for construction/escaping, without requiring a running notification daemon.

Run the managed-window example:

```sh
cargo run -p guiraffe-overlays --example overlays
```

`modals::{open, confirm, close, close_all, register_context, open_context}` and
`notifications::{show, update, hide, clean, set_limit}` provide the free-function
service API; associated `OverlayHost` functions remain available.

The managed visible-toast limit is clamped to Controls' theme `notification.max_items`;
queued lifetimes start only when promoted. The root helper caches application
content so progress redraws do not rerender the page. Use it for both ordinary
and `WindowManager` windows when that isolation is required.

Key contexts: `GrModalEscapeDisabled > Sheet` suppresses only the Escape key,
preserving pointer close; `GrSpotlight` and `GrSpotlight > Input` bind Up/Down/Enter.
Applications can override these bindings through GPUI's keymap.
