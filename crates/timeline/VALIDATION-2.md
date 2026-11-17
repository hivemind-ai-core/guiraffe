# Timeline overhaul — 2026-11-17

Branch `feat-timeline-2`, based on release `ca03807`, in `/home/dan/projects/anteater/worktrees/feat-timeline-2`. No merge or push. The design was committed before implementation in spec/16 section D.

Code commits:

- `e2c43db`: composition/editing design.
- `4957456`: shared plot layers/navigation, Gantt/Tracks integration, independent lock styling.
- `29c6969`: pure curve editing and quantized bar model.
- `58d1cbd`: hierarchical dope sheet, native curve/bar surfaces, interaction tests and measurement fix.
- `33ba9ff`: checked segment indices and numerically stable bar interpolation.

## Implemented behavior

`PlotSurface` shares an f64 x axis, optional value range and ordered decoration layers. Rulers support seconds, frames and days; built-ins cover major/minor grids, ticks, playhead/badge, outlined selection, markers, row stripes and explicit background intervals. Custom `Layer` implementations append viewport-coordinate commands. All timeline editors use the shared surface and accept `add_layer(...)`; pan, anchored zoom and fit use its common implementation. Editor-specific key/curve/clip/task content remains in each native view's content pass.

Locks admit or reject editing independently of styling. Global locking leaves appearance unchanged by default, while item locks have small badges; the Gantt/Tracks appearance can be None/Badge/Dimmed/Hatched. Kit icon buttons in label columns toggle task/clip/channel locks. Seeking/navigation remain available while editing is locked.

The dope sheet has nested groups, cached summaries, expand/collapse, mute/lock/visibility buttons, key shapes by interpolation, equal-value hold bars, click/additive/box selection and atomic frame-snapped group moves. Collisions reject a move. Explicit focus opt-in enables select-all, duplicate, nudge and delete; default pointer interaction does not capture keyboard focus. Visible rows and time-sorted glyphs are culled. A native 10,000-channel test verifies offscreen controls disappear and scrolling reaches the last channel. Summary selection addresses real child keys; duplicate operations retain key metadata and respect inherited locks.

CurveView supports multiple colored/visible curves, selection, key insertion/removal, cubic-preserving segment splits, segment removal, interpolation and Free/Aligned/Mirrored/Flat tangents. Selected Bezier keys expose draggable handles, including defaults for zero-length handles. External APIs drive the showcase toolbar; fit/normalize changes the shared viewport without rewriting data. Segment removal joins neighbouring keys as documented in README.

BarCurve/BarCurveView support N bars, bounded values, snap intervals, bipolar presentation, gap-free interpolation across sparse pointer events, changed-range events and sampled Curve import/export. Frame captures share values with copy-on-write edits. Generated tests cover both drag directions across thousands of index pairs, clamping, empty/single models, quantization and extreme finite inputs.

The shared measurement hook now defers notification of the editor entity when bounds change. This fixes a first-interaction axis/fit shift that native selection tests reproduced; it avoids writing the parent shell.

## Passing validation

At code revision `33ba9ff`, on both upstream GPUI and `gpui-fast`:

- `cargo test --locked -p guiraffe-timeline` (with `--features gpui-fast` for fast): **61 passed per backend**, zero failed/ignored: 50 unit/model tests, ten native interaction tests, one snapshot test containing light and dark scenes. Final runs compared goldens with snapshot updates disabled.
- `cargo clippy --locked -p guiraffe-timeline -p guiraffe-showcase --all-targets -- -D warnings` (with `--features gpui-kit/gpui-fast` for fast): passed, including the new example.
- `cargo test --locked -p guiraffe-showcase --test coverage`: one test passed; the new BarCurveView entry is covered. This is not a claim that the entire showcase test suite ran.
- `cargo fmt -p guiraffe-timeline -p guiraffe-showcase -- --check` and `git diff --check`: passed.

Native tests cover click/shift/group drag, box selection and shrinking additive boxes, channel controls, hierarchical collapse, keyboard opt-in, row lock buttons, sparse bar drawing, lock suppression, key/tangent dragging, public curve editing APIs and virtual rows. Pure tests include atomic moves/collisions, duplicate metadata, cubic split shape preservation, tangent defaults, invalid segment indices, interpolation, snap and visible ranges. No unwrap/expect was added to non-test library code.

Four final PNGs under `tests/snapshots/{upstream,gpui-fast}/timeline/{light,dark}.png` were regenerated and individually inspected. Scenes contain all five views, multiple curves, selected keys, hold bars, tangent handles, lock icons and bipolar bars. Inspection corrected selected-key contrast and replaced an opaque selection fill with an outline, so selected content stays visible.

## Rendering evidence and limits

`editing_apis_invalidate_only_the_editor` counts actual shell Render calls, requires a nonzero baseline, verifies changed model values, and checks unchanged shell counts on fast for dope, curve and bar API edits. Native pointer tests exercise the real event path and model outcomes. Native input also rebuilt cached ancestors in this GPUI test environment even with nested cached views; **native-input shell retention is not established**. The isolation result applies to editor notifications/API edits, not every platform-input redraw. This remains a limit against the broadest reading of the requested render-count gate.

The native row-culling proof is for DopeSheet. Gantt/Tracks retain their existing row layout. Custom decoration layers are ordered within their surface pass; item painters remain view-specific. Channel mute/visibility are emitted/readable state for host playback logic. No new 120fps claim, allocation benchmark, desktop visual session or general chart adapter is included. Snapshots are headless-render evidence. Builds used stable Rust 1.98.0 and the shared `feat-chat/target` cache.
