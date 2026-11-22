# guiraffe-overlays

Window-local overlays on `gpui-kit =0.9.1`. Call `init(cx)` before opening windows.
For managed windows, return the idempotent `root(content, window, cx)` helper from
`WindowManager::open`; it also caches application content during leaf updates.

- `modals::{open, confirm, close, close_all, register_context, open_context}`:
  independently identified modal layers, context factories and awaitable confirms.
  `close` removes any owned ID and re-chains focus, without popping native dialogs.
- `Modal` is the managed builder. Existing repository builder call sites compile through inference; explicit
  `gpui_kit::component::dialog::Dialog` annotations must become `Modal`. The adapter
  uses Base Dialog focus/actions and Controls title/content/footer/close parts.
  GUIraffe owns IDs, focus restoration and guiraffe-motion Presence transitions.
- `Drawer`: four-sided Controls Sheet. One Sheet per window; open it before modals.
  It can subsequently be removed by ID from beneath those modal layers.
- `notifications::{show, update, hide, clean, set_limit}`: pure FIFO scheduling,
  six positions, progress and hover pause, with native notification presentation.
  Managed delivery is always InApp. Unmanaged notes reduce available admission
  capacity; hidden work queues without consuming its lifetime. Updates preserve
  native entity/order and hover state. Timers stop when all timed notes pause.
- `spotlight::{SpotlightState, Spotlight}`: the command bar. `SpotlightState` (Control)
  holds `SpotlightAction`s (title, description, group, keywords, icon, shortcut chips,
  disabled), pinned ids and recents, an optional filter override and labels; it emits
  `SpotlightEvent::{Opened, Closed, QueryChanged, SelectionChanged, Activated}`.
  `SpotlightState::open` shows it over the theme scrim in the upper third (Pop
  transition, instant with reduced motion) and restores focus on close; `Spotlight::new`
  embeds the same panel inline. Fuzzy matches are highlighted (color, weight,
  underline). The result list is virtualised with exact row offsets; the pure
  `SpotlightModel` prepares text once, narrows on extending queries and allocates
  nothing per keystroke (10k actions: ~22 ms mount, ≤ 6.5 ms worst keystroke in
  release, gated by `tests/spotlight_scale.rs`). `load(future)` installs an async
  source and prepares its index on a background thread. Keys: ↑/↓, Ctrl+N/P,
  PageUp/PageDown, Tab/Shift+Tab (groups), Enter; Escape and outside press close.
- `search`: the shared fuzzy scorer (`fuzzy_score`, `match_ranges`, `SearchIndex`),
  property-tested against an exhaustive reference.
- `FloatingTooltip`: controlled, measured popup using the pure flip/shift/arrow
  solver. Shift adjusts only the cross axis, retaining the main-axis anchor gap.
  `Tooltip`, `Popover`, `HoverCard` and menu exports keep Controls' native behavior.
- `LoadingOverlay`: pointer occlusion plus a window-scoped keyboard interceptor.
  Its element state survives cached frames and releases the interceptor on unmount;
  background focus is retained so input resumes when the overlay disappears.
- `Affix`: six token-spaced anchors, with caller style overrides.
- `desktop::DesktopNotification` (`desktop` feature): use `show_async(cx)` and await
  the returned task. Blocking `show` remains an explicit compatibility boundary.

Key contexts: `GrModalEscapeDisabled > Sheet`, `GrSpotlight`, and
`GrSpotlight > Input`. Applications may override their bindings.

Native and managed modal storage is independent. A focused native dialog covers
managed layers; opening a managed modal focuses and presents it above an existing
native one. ID-based managed operations never close an unrelated native dialog.
Direct native Sheet operations bypass Drawer IDs, so use the managed service for
owned sheets. Prefer WeakEntity captures in context factories; window-close
cleanup also breaks accidental strong Root/host cycles.

```sh
cargo run -p guiraffe-overlays --example overlays
cargo run -p guiraffe-overlays --example floating_tooltip
rustc --edition=2024 -O crates/overlays/examples/search_bench.rs -o target/search-bench
target/search-bench
```
