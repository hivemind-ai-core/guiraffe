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

- **Native GPUI drag refreshes the entire window on pointer movement.** Both pinned backends bypass cached views during that refresh. A `SortableBoard` uses window-level pointer tracking with a separate `drag_overlay()` for in-window moves. The native `DragSource::attach` adapter and Kit's dock tab and resize gestures still use GPUI's native drag path.
