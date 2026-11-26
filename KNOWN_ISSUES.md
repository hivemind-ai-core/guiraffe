# Known issues

## Platforms

- Only Linux is tested, mainly Wayland. The macOS and Windows code for notifications and the tray
  has never been compiled.
- No component has been checked with a screen reader.
- The crates aren't published. Apps use git or path dependencies and must repeat the
  `[patch.crates-io]` table, because Cargo doesn't pass patches on from dependencies.

## Pinned gpui-fast

gpui-fast 0.1.16 doesn't compile with gpui-kit 0.9.1, and a fresh dependency resolution picks it.
The workspace pins 0.1.15. Apps copy the lockfile (`guiraffe new` does it), and `guiraffe doctor` checks the pin.

## Dependency advisories

`cargo deny` excepts two advisories that have no fixed release. Any other advisory fails CI.

- [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543) and
  [RUSTSEC-2026-0555](https://rustsec.org/advisories/RUSTSEC-2026-0555): `chrono` and `chrono-tz`,
  used by the calendar and date inputs, are unmaintained.

Miri passes on the focused tests of every `imbl` consumer. A full Miri run is not clean yet:
`guiraffe-core` reports leaks after its assertions pass, the sheet proptests call `getcwd` under
isolation, and the full sheet and UI runs did not finish.

## Patched frames

gpui-fast patches a frame in place when one cached view is built again in the same shape (see
`vendor/README.md`). A patched row edit allocates 139 times, against 124 when the same row is
drawn again unchanged. The 15 extra allocations per edit are flat in the number of rows, but they
are not yet down to the frame's baseline. Follow-up in ROADMAP 0.3 #19, which trims GPUI's
per-frame high-water buffers. On fallback (another shape, such as a label shaped to another number
of glyphs), the view is built once for the patch attempt and again for the splice.

## Command ribbon

Implemented in `guiraffe-commands::ribbon` (ROADMAP 0.3 #13). Not yet done:

- Placement (top/bottom) and open/close animation props from the design are not implemented; the
  host places `CommandRibbon` in its layout.
- `ui!`'s `ribbon: "format/b"` is not wired, and a DataUi `command-ribbon` mounts over the
  application registry with an empty layout: documents cannot describe ribbon layouts yet.
  `control_menus` builds entries from a window's action bus registrations instead.
- Shortcut and chord overrides persist through `ribbon::OverrideFile`; there is no keybinding
  editor page yet.
- Spotlight cannot prompt for parameters, so commands with parameters run only from the ribbon
  or `Commands::execute_with`.

## Performance

The budgets and test conditions are in [performance](docs/performance.md). Open in 0.2:

| Issue | Measured | Budget |
|---|---|---|
| Flame graph at middle sizes | p99 14.6 ms at 65k frames, 39 at 131k, 96 at 262k upstream; about 18 at 262k on gpui-fast | 8 ms |
| Tracks at middle sizes | p99 26 ms at 6.4k clips, 53 at 12.8k upstream; 17 and 32 on gpui-fast | 8 ms |
| Dope sheet in the mixed soak | frames up to 10.5 ms on gpui-fast | 8 ms |
| Calendar year view | p99 12-16 ms. Month and week can exceed 8 ms on a busy machine | 8 ms |
| Mermaid mount at 64 nodes | 278-281 ms | 200 ms |
| Sheet memory, 100k formulas (fixed in 0.3) | 119-123 MiB over an empty window | about 68 MiB |
| CodeViewer loading 10M lines (fixed in 0.3 for streaming loads) | 379 MiB peak, 192 MiB afterwards | 302 MiB |
| Chat, 100k messages | one 22 ms frame while warming up on gpui-fast | 16 ms |
| Process memory over a 30-minute soak | +5.8 % upstream, +5.7 % gpui-fast | 2 % |
| Unchanged repaint allocations | DataUi 132, `ui!` 42, command menu 735, devtools 615, router, state and motion 6-18 | 0 |
| Label edit among 3,000 live rows | 19-46 ms | work independent of row count |

- **Mixed-soak RSS growth is predominantly retained allocator space in the upstream
  reproduction.** At peak, the default run gained 4,604 KiB RSS, including 4,600 KiB in
  the process heap. Glibc reported 4,777 KiB more free space and 245 KiB less in-use
  space in its arenas. A single trimmed arena still grew, with both free and in-use
  space contributing. See the [30-minute measurements](docs/performance.md#mixed-soak-rss-attribution).
- **Markdown replacement looks slow in the soak log (about 70 ms)** because the soak runs
  background parsing on the timed thread. With real threads one run peaked at 4.7 ms.
- **GPUI frame buffers keep their peak size** until the window closes: 6-10 MB after 10,000 live
  elements, 64-99 MiB after 100,000. Virtualized components stay far below this.
- **gpui-fast rebuilds its scene for rounded overflow clips** on every changed frame. Clip
  rectangularly inside rounded shapes where it matters.

## Store binding (provisional)

- On upstream GPUI, a bound control's edit also re-renders its ancestors and their unbound
  children. Only gpui-fast isolates the render. Store-side work is isolated on both.
- A bound control's parent re-render always re-renders the bound control in the same frame,
  because its unbound props may have changed.
- Two-way binding of entity-backed inputs (`InputState`) needs an adapter. Stateful component
  ids must be unique per window: the bus rejects duplicates in debug builds.

## GPUI behavior

- **An invalid key-binding string panics** inside GPUI, and `secondary-` isn't resolved. Use
  `guiraffe_core::keys::{binding, bind_keys}`, which return errors.
- **Native drag refreshes the whole window** on pointer movement. `SortableBoard` and `new_dock`
  use their own overlays. Prefer them to `new_native_dock`.
- **On upstream GPUI a child's notify re-renders its ancestors**, so trees and property grids only
  guarantee render isolation on gpui-fast.

## Components

### Chat

- A drag selection clears when one end scrolls out of the rendered rows, so you can't select
  across virtualized history in one drag.

### Calendar

- Edits emit `CalendarEvent::Announcement`. GPUI has no live region, so the host must announce.
- Hidden virtualized hours aren't exposed to assistive technology.
- Chips are reached with the roving cursor rather than Tab, and resize handles show only on the
  selected event.
- Edge auto-scroll steps once per pointer move. Losing pointer capture doesn't cancel a drag.
- A repeated daylight-saving hour is shown in lanes, not as two rows.
- Multi-day items have no continuation cue, density only changes the toolbar, and `set_locale`
  doesn't relabel the view switcher.

### Notifications and tray

- freedesktop reports no close reason after an action.
- GNOME has no layer shell, so it decides where notification windows go. Prefer system
  notifications there.
- Notification windows are pointer-only, clamp their body to two lines, and on X11 may flicker
  when a bottom-corner stack resizes.
- A tray double-click also delivers its first click as `Click`.
- Minimize-to-tray closes and later reopens the window, because Wayland has no hidden state.
  Provide a Quit item.
