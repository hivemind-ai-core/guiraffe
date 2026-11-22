# Known issues

1. ~~SVG icons are blank in offscreen visual snapshots.~~ **Retracted**: this was a GUIraffe bug, not a renderer
   limitation. `Snapshotter` passed an empty asset source, and the foreman's earlier fix had silently failed to apply
   and was never verified. A worker found it while reviewing its own snapshots. The fix is in, and `icon_probe` now runs as a regression test.
   Lesson: verify that every edit actually applied.

2. ✅ FIXED (flow review 2: fit on first layout; gallery golden inspected). ~~**The flow editor doesn't fit its graph to the viewport on first show.** In the gallery, the right-most node is clipped until the user clicks Fit. Owner: flow follow-ups (reviews/flow-review-2.md).

3. **Upstream semver break: gpui-fast 0.1.16 doesn't compile with gpui-base 0.9.1** (conflicting `IntoElement` impls).
   A fresh dependency resolution picks it. GUIraffe's `Cargo.lock` pins 0.1.15, and `guiraffe new` copies that lockfile into
   new apps so they build. Verified: a scaffolded app builds without warnings and launches. Report upstream, and drop the pin when fixed.

- **gpui-fast panics on an invalid key-binding string** (e.g. `"secondary-k"`) via an `unwrap` in binding parsing. Use explicit `ctrl-k` / `cmd-k`. Upstream behaviour (`KeyBinding::new` unwraps and doesn't resolve `secondary-`). **Mitigated:** use `guiraffe_core::keys::{binding, bind_keys}`, which parse for the host OS (`secondary-` → ⌘/Ctrl) and return errors instead of panicking.

- **Native GPUI drag refreshes the entire window on pointer movement.** Both pinned backends bypass cached views during that refresh. A `SortableBoard` uses window-level pointer tracking with a separate `drag_overlay()` for in-window moves. The native `DragSource::attach` adapter and opt-in `new_native_dock` use GPUI’s native drag path. Use `new_dock` with its separately mounted gesture overlay for in-window docking.

### Upstream GPUI invalidates TreeList and PropertyGrid ancestors

On GPUI Pre 0.3.12, keyboard expansion and wheel scrolling update the tree
entity and call `Context::notify()`. The backend routes this through
`App::notify(entity_id)` and `WindowInvalidator::invalidate_view`.
Before drawing, `Window::mark_view_dirty` follows the entity's dispatch path
and marks ancestor views dirty. `AnyView` then skips cached output for each
dirty ancestor. The tree handlers do not call `Window::refresh()`.

A child control notified with `App::notify(child.entity_id())` takes the same
path. The `notifying_only_a_cached_child` test strips away widget state,
focus changes, and input dispatch to exercise that path directly. Running the
repro against upstream GPUI increased both the child and host render counts
from one to two. DataGrid
uses shared state with targeted row/view notifications, and CodeViewer uses
targeted text/overlay notifications. Their strict render-count assertions
also apply on gpui-fast. TreeList and PropertyGrid retain a strict
gpui-fast assertion for expansion and scrolling.

### Desktop notifications and system tray (0.2)

| Feature | Linux (Wayland/niri) | Linux X11 | macOS | Windows |
|---|---|---|---|---|
| System notifications (`overlays::desktop`, notify-rust) | Tested: delivery errors surface as `Failed(Unavailable)` events when no daemon runs; actions/close/replace covered by mock-backend tests (no daemon on the dev machine) | same D-Bus path, untested | **Untested, not compiled here**: delivery only, no action/close events | **Untested, not compiled here**: delivery only |
| Notification windows (`windows::notify`) | Tested: wlr-layer-shell overlay surface, anchored to the corner | Positioned `PopUp` window, untested | **Untested** | **Untested** |
| System tray (`guiraffe-tray`, feature `native`) | Tested: ksni StatusNotifierItem registers with waybar's watcher | same (SNI is display-server independent) | **Untested, not compiled here** (tray-icon 0.26) | **Untested, not compiled here** (tray-icon 0.26) |

- **No cross-target build.** This machine has no macOS/Windows Rust targets or SDKs; the `cfg(macos/windows)` code
  in `overlays/src/desktop/platform.rs` and `tray/src/native.rs` was written against the notify-rust 4.19 and
  tray-icon 0.26 / muda 0.21 sources but has never been compiled. Build it on those platforms before relying on it.
- **macOS/Windows system notifications report nothing back.** notify-rust delivers them but exposes no click,
  action or close callbacks there (`Capabilities::NONE`). GPUI's own `show_system_notification` API could fill
  this gap later.
- **freedesktop: after an action the close reason is not reported.** notify-rust stops listening at the first
  signal, so an `Action` event is terminal and the server's following `NotificationClosed` is not seen.
- **Notification windows on Wayland need wlr-layer-shell.** niri, sway, Hyprland, KDE have it. GNOME/Mutter does
  not: the surface falls back to a regular pop-up window whose position and stacking the compositor decides
  (`PlacementUsed::CompositorPlaced`). Prefer system notifications there.
- **Notification windows on X11/macOS/Windows:** GPUI cannot move a window, so a bottom-corner stack whose height
  changes is closed and re-opened at its new origin (a brief flicker); top corners resize in place. Untested.
- **Card bodies are clamped to two lines** so every card has a fixed, pure-computable height (no measuring
  loop); a one-line body leaves the second line empty.
- **Hover pause in tests:** the GPUI test platform reports the pointer at the window origin, so tests move it
  out first; real platforms report the real pointer.
- **Tray double-click** is synthesized from two activations within 400 ms on SNI and macOS (only Windows reports a
  native double click); the first click is still delivered as `Click`.
- **Minimize-to-tray closes the window** (`HideMode::Close`, re-opened through `WindowManager` with its saved
  bounds): Wayland has no hidden-window state, and niri/sway have no minimized state either. It sets
  `QuitPolicy::Never`; provide a Quit menu item. Without a tray host, closing works normally.
- **Accessibility:** cards expose `Status`/`Alert` roles with a label; the notification surface never takes
  keyboard focus, so its buttons are pointer-only (system notifications are the accessible path).

### Calendar accessibility limits (guiraffe-calendar)

- **Screen-reader verification is pending.** The calendar exposes `Grid`/`GridCell` roles,
  localized date and event-count names, selected/expanded/toggled state, and row/column
  indexes for Month cells. The `grid_exposes_cells_sources_and_labels_to_accessibility` test
  checks these through the inspect API. Nobody has yet checked them with VoiceOver, NVDA or Orca.
- **No live region.** Edit commits, cancellations and pick-ups are emitted as
  `CalendarEvent::Announcement` for the host to announce. GPUI 0.9.1 has no live-region role,
  so the calendar does not speak them by itself.
- **Virtualized set counts are partial.** Week/Day hour rows are virtualized, so only the
  visible hours exist as nodes. The grid reports its row/column counts, but GPUI does not
  expose hidden virtualized rows to assistive technology.
- **Overflow focus.** The `+N more` list is operated with the calendar's roving keyboard
  cursor (Enter opens it, Up/Down choose, Enter selects, Escape closes). Focus stays on the
  calendar grid instead of moving into the list.
