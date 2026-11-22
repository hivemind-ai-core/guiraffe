# Known issues

## 0.2 performance: open at release

0.2 does not meet every budget in spec/23 and plans/PERF-0.2.md. The work was stopped here by user
decision (2026-11-22). Timings below are from release builds on a machine that was not fully idle,
so they are provisional.

| Issue | Measured | Budget | Notes |
|---|---|---|---|
| FlameGraph at mid sizes | p99 14.6 ms at 65k frames, 39 ms at 131k, 96 ms at 262k (upstream); about 18 ms at 262k on gpui-fast | p99 8 ms | 10M frames is fine (1.1 ms): level of detail hides it |
| Tracks at mid sizes | p99 26 ms at 6.4k clips, 53 ms at 12.8k (upstream); 17 and 32 ms on gpui-fast | p99 8 ms | 100k clips is fine (3 ms) |
| Dope sheet in the mixed soak | frames up to 10.5 ms, in 63 of 180 windows (gpui-fast) | p99 8 ms | session-wide p99 not recorded |
| Mermaid mount at 64 nodes | 278-281 ms | 200 ms | 500 nodes mounts in 62 ms |
| Sheet memory, 100k formulas | 119-123 MiB over an empty window | about 68 MiB | |
| CodeViewer, 10M lines | 379 MiB peak over empty, 192 MiB steady | 302 MiB | peak may include the copy made while loading the source |
| Chat, 100k messages | one 22 ms frame while warming up (gpui-fast) | 16 ms | p99 3.7 ms |
| Process RSS over a 30-minute mixed soak | +5.8 % upstream, +5.7 % gpui-fast | 2 % | no leak found, see below |
| Unchanged repaint allocations | DataUi 132, `ui!` 42, command menu 735, devtools 615, router/state/motion 6-18 | 0 | zero is gated only for some components |
| Electron comparison, bench baseline | not measured for 0.2 | | the comparison needs the lock fix on `integrate-b23` |

**RSS growth: no leak found.** `scripts/leak-check.py` (Valgrind, about 6 minutes for both backends)
runs all 22 soak scenarios: no lost memory grows with the frame count and there are no invalid
reads, writes or frees. The live heap of the mixed soak over 9,600 frames changes by +0.1 % upstream
and +0.2 % gpui-fast. Replaying 150,000 frames, RSS from 20 % of the run to the end grows 4.3 % /
2.1 % with default glibc malloc and 2.2 % / 1.9 % with one arena and fixed trim thresholds, so
upstream about half the growth is allocator fragmentation. The remainder (under 2 MiB) is not
attributed, and a leak too slow to show in 9,600 frames is not ruled out.

**Markdown replacement is probably not a stall.** The mixed soak logs 70-73 ms frames when it
replaces a 172 KB document, but it runs background parsing on the timed thread. With real worker
threads the same replacement peaked at 4.7 ms on the main thread (one run, machine under load).

**Miri.** GPUI itself cannot run under Miri. `guiraffe-flow-core` tests report undefined behaviour
under Stacked Borrows inside `sized-chunks` 0.6.5 (via `im` 15.1, which gpui-base and guiraffe-core
use). The Tree Borrows run timed out, so whether it is exploitable or only an aliasing-model
violation is unknown.

## Other issues

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

### GPUI frame buffers keep their peak size (accepted 0.2 exception)

After an unusually large frame, GPUI's per-frame element-state, access-log and tab-stop buffers
keep that capacity until the window closes: 6-10 MB after 10,000 live elements and 64-99 MiB after
100,000. Virtualized components stay far below this. This is the one accepted exception to
"every cache is bounded" (2026-11-22); trimming is planned in plans/ROADMAP-0.3.md §E.

### Selecting text across Chat history

A drag selection in Chat clears when one end scrolls out of the rendered rows.
This prevents selecting and copying text across virtualized history in one drag.

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

### Calendar gaps against spec/27 (guiraffe-calendar)

- **Performance margin is thin.** On an 8-core/16-thread laptop the Month/Week 8 ms p99 gate
  passed in a run at load 4 and failed (about 9-11 ms) as other builds raised the load, although
  the probe's quiet-window rule (load <= CPU count) accepts both. Year misses its 8 ms target
  (p99 about 12-16 ms); it is outside the Month/Week gate.
- **Changed frames are bounded, not zero.** An unchanged calendar in a cached view repaints
  with zero allocations on both backends. A changed frame rebuilds every element of the view
  (about 13,000 allocations for a full Month, mostly GPUI layout, listeners and element state);
  it is not yet split into cached cells. See the budgets in `tests/alloc.rs`.
- **Density** changes only the toolbar gap, not row or chip targets.
- **Tab stops.** Event chips, timed blocks and `+N more` are reached with the roving cursor;
  they are not separate Tab stops when `focusable` is set.
- **Resize handles** of timed events appear only on the selected event (select, then drag).
- **Edge auto-scroll** steps once per pointer move; a pointer held still at the edge does not
  keep scrolling. Pointer-capture loss and theme changes do not cancel a drag.
- **DST fold.** The two occurrences of a repeated local hour share wall-clock rows and are
  shown side by side in lanes, not as two chronological rows.
- **Month capacity** comes from the fixed cell height, not from measured host height.
- **Multi-day all-day items** repeat a chip per day without continuation cues.
- **Localization.** View-switcher labels are fixed at construction; `set_locale` does not
  relabel them.
- **Agenda/schedule view** is a follow-up, outside spec/27.

### gpui-fast rebuilds the scene for every rounded overflow clip

When any element both clips (both axes) and has rounded corners,
`fast::rounded_clip_scene::compile` copies the frame into new zero-capacity scenes on every
changed draw. The calendar's month chips made that about 10 MB per measured frame, and an extra
glyph could move it across a capacity doubling. The calendar now clips rectangularly inside
rounded shapes. Other controls that use `rounded` with `overflow_hidden` still pay this cost;
reusing the buffers would be a fix in the vendored backend.
