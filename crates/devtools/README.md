# GUIraffe Devtools

Opt-in tools for a development build. The crate has **no dependencies or runtime code**
without its `devtools` feature. In an application, prefer an optional dependency:

```toml
[dependencies]
guiraffe-devtools = { path = "../devtools", optional = true, features = ["devtools"] }

[features]
devtools = ["dep:guiraffe-devtools"]
gpui-fast = ["gpui-kit/gpui-fast", "guiraffe-devtools?/gpui-fast"]
```

Call `guiraffe_devtools::init(cx)`, create one `Devtools::new(cx)` per window, then mount
`guiraffe_devtools::root(app_view, &tools, cx)` as the root view. Ctrl-Shift-I toggles it
within the `GrDevtools` key context. When opening a window without a focused child, call
`tools.focus(window, cx)` once to establish the keyboard target. `tools.toggle(cx)` supports a menu/toolbar action.

Pass a clone of the handle to views to inspect. At the end of each view's `render`, return
`tools.track("unique-view-id", element)`. Call this once per actual render, rather than
wrapping the same view again in its parent. The wrapper delegates layout and paint,
records inherited `StyleContext` during layout, and records screen bounds during prepaint.
Use `tools.untrack(id)` on unmount. No application entity or frequently changing Global
is read or written to collect telemetry. Uninstrumented GPUI/Controls internals aren't
included. Reused ids conflate counters, so ids must be unique per mounted view.

The Highlights control flashes each tracked view's bounds for 180 ms after render;
its counter is listed in the panel. Inspect selects the smallest tracked rectangle beneath
the pointer, displays bounds, density, control size, surface, emphasis and the supplied id.
The outline canvas has no hitbox and lets pointer input reach the application.

Frame times come from GPUI's `profiler::FrameTimingCollector`, filtered by window id:
actual `Window::draw` CPU duration and GPUI's request-layout/prepaint/paint splits.
The histogram and nearest-rank p50/p99 use the latest 240 draws while tools are open.
Measurements include the tools' own drawing cost; they exclude waiting for vsync and GPU
presentation. The tools leaf schedules frames while open to collect telemetry and update
hover/flashes, and stops when closed or paused. Upstream GPUI's collector uses its own
internal synchronization. No synchronization is added to application rendering. A cold-path tracing lease uses a mutex
only when opening, pausing or closing the tools, preserving external profilers' prior setting.

Connect any `Store<S>` using `inspect_store(name, &store, capacity, describe, cx)` and
`tools.add_store(adapter, cx)`. The `describe` callback returns JSON for a diagnostic view;
it can omit secrets and expensive state. Snapshots are retained as Arc clones in a bounded
log. Diffs are computed only for the current page (eight events), with Older/Latest controls
for navigation. Objects use JSON pointer paths; arrays are atomic. Missing values differ
from JSON null. Before/After queue a normal non-undoable restore event, so store selections
update normally; effects are not replayed. Existing queued events retain FIFO ordering.
Store's public `Fx::set` takes an owned `S`, so restoring clones `S`; use Arc/persistent
fields to keep this cheap. Dropping the Store is safe (the adapter holds a weak handle).

Motion 0.1× multiplies `MotionPrefs.time_scale` by ten (that field is a duration multiplier),
and toggling back restores its previous value. Reduced-motion settings remain respected.
Changing this configuration intentionally refreshes animation consumers; profiling,
inspection, toggling and state-log browsing only notify the independent tools leaf.

`set_paused(true, cx)` freezes frame collection and highlights for repeatable captures.
Tests use `guiraffe-test::snapshot` with separate upstream/gpui-fast light/dark goldens,
real GPUI keyboard interactions and a gpui-fast render-count assertion for both the
inspected app and an ancestor. The polling guarantee applies to gpui-fast: upstream GPUI still
re-renders ancestors on child notifications by design. Native pointer input in the pinned
gpui-fast 0.1.15 backend updates the typed window root and rebuilds the app even for a
background click; the controls test measures that baseline separately and asserts tools
controls add no renders beyond it. An absolute zero-render guarantee for native pointer
input needs a GPUI backend change, outside this crate's ownership.

Run the gates with:

```sh
cargo fmt --all -- --check
cargo test -p guiraffe-devtools --features devtools
cargo test -p guiraffe-devtools --features devtools,gpui-kit/gpui-fast
cargo clippy -p guiraffe-devtools --all-targets --features devtools -- -D warnings
cargo clippy -p guiraffe-devtools --all-targets --features devtools,gpui-kit/gpui-fast -- -D warnings
```
