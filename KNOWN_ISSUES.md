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
