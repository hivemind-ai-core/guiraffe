# Devtools handoff — 2026-11-16

Implementation source revision: `52b92c8ce463a2b9f6f5d638f6bbfa40aa4e315c` on `feat-devtools`.
Initial implementation: `ac3de85`. No merges were performed.

The devtools crate is implemented and its scoped gates pass. Overall task acceptance is
incomplete: gallery ownership approval remains unanswered and an absolute zero-render
native-pointer guarantee requires a GPUI backend change. This handoff preserves a tested,
reviewable gallery patch rather than editing another worker's crate without approval.

## Delivered

- `guiraffe-devtools`, no default features; no normal dependencies with `devtools` disabled.
- Per-window overlay, Ctrl-Shift-I in GrDevtools, optional root focus for initially unfocused
  windows, toolbar-ready handle and independent leaf panel.
- Explicit `track(id, output)` instrumentation: actual render counters, inherited
  StyleContext at layout, screen bounds at prepaint, 180 ms highlights and hover inspector.
- Real GPUI draw timings and layout/prepaint/paint splits, rolling 240-frame histogram,
  nearest-rank p50/p99, and a pause/resume control with trace-cursor reset.
- Store interceptor with bounded Arc snapshots, paginated event log, before/after JSON
  pointer diffs, and Before/After snapshot jumps through a non-undoable Store event.
- Relative motion 0.1x playback, restoring the previous `MotionPrefs.time_scale` on toggle.
- Eight light/dark goldens, separate for upstream and gpui-fast; all eight inspected in a
  contact sheet. Populated profiler goldens use fixed samples to avoid clock noise.
- Integration and performance limits documented in README.md.

## Validation

All commands below passed against the implementation source revision (no snapshot update
flag in the final comparison runs):

| Check | Result |
| --- | --- |
| `cargo test -p guiraffe-devtools --features devtools` | 8 tests and 1 compiling doctest passed |
| `cargo test -p guiraffe-devtools --features devtools,gpui-kit/gpui-fast` | 8 tests and 1 compiling doctest passed |
| `cargo clippy -p guiraffe-devtools --all-targets --features devtools -- -D warnings` | Passed |
| `cargo clippy -p guiraffe-devtools --all-targets --features devtools,gpui-kit/gpui-fast -- -D warnings` | Passed |
| `cargo fmt -p guiraffe-devtools -- --check` | Passed |
| `cargo check -p guiraffe-devtools --no-default-features --lib` | Passed |
| `cargo tree -p guiraffe-devtools --edges normal --no-default-features` | Only guiraffe-devtools; no normal dependencies |
| `git apply --check crates/devtools/integration/gallery.patch` | Passed; patch is unapplied |

The GPUI interaction test exercises the actual default shortcut, highlights and inspector
controls, motion speed and restoration, a Before button that changes a real Store, actual
GPUI frame samples, pause/resume sampling, and no scheduled frames after closing. On
`gpui_kit::GPUI_FAST`, five telemetry frames and pause/resume leave both the inspected app
and its ancestor at exactly their prior render counts. An actual app notification increments
the tracked render counter, showing that the instrumentation still observes real work.

`cargo fmt --all -- --check` reports pre-existing formatting in these untouched files:
`crates/cli/src/scaffold.rs`, `crates/components/tests/context.rs`,
`crates/core/src/theme.rs`, `crates/test/src/snapshot.rs`, and
`crates/test/tests/snapshot.rs`. These were not reformatted under the crate ownership rule.
Raw logs are retained in `validation/`.

## Gallery proposal and ownership blocker

`integration/gallery.patch` contains the optional gallery dependency/feature, initialization,
a labeled Devtools toolbar button, an instrumented gallery shell, router Store inspection,
initial keyboard focus, and a toolbar/shortcut interaction test. It changes only gallery
Cargo.toml, src/lib.rs, src/shell.rs and a new tests/devtools.rs.

The patch was implemented in `/tmp/guiraffe-devtools-gallery-proposal` for validation without
editing this worktree's protected gallery files. Its dependencies point at this checkout,
and its lockfile started from this checkout's lockfile. The proposal passes an upstream
cargo check plus its toolbar/shortcut test on both backends (one test on each). This is
proposal validation, not a claim that the gallery feature is integrated in this checkout.

AGENTS.md says: "Don't edit other crates' files except to add workspace deps; ask the foreman."
Question `msg_16159475cfd8` requested permission for the gallery integration, and escalation
`msg_258ce21c8a2a` reported the unanswered scope question. No answer had arrived at handoff.
The foreman can approve/apply the patch and then run the gallery integration gates.

## Known limits requiring explicit review

1. **Native pointer input is not zero-render on pinned gpui-fast 0.1.15.** Its platform input
   callback invokes typed `WindowHandle::update` (`window.rs:2269`), which calls `view.update`
   (`window.rs:7788`). This writes the root even for an empty-background click. The regression
   test measures that native baseline and asserts tools controls add no app renders beyond
   it. The strict zero-render assertion remains for tools-driven telemetry and pause/resume.
   Native input's absolute zero-render requirement remains unfulfilled; escalation
   `msg_5ea39be1a753` records this backend boundary. Motion configuration and state jumps
   intentionally update the app, as requested.
2. Inspection is explicitly instrumented: arbitrary internal GPUI/Controls views are not
   automatically discovered. Hosts must supply unique ids and untrack them on unmount.
   Rectangle hit testing selects the smallest containing tracked rectangle; it is not
   GPUI's full occlusion/clip-aware element tree inspector.
3. Frame statistics measure CPU draw work, include tools overhead, and exclude vsync and
   GPU presentation. Tracing uses GPUI's internal collector; an additional lease mutex is
   used only on open/pause/close, never on application rendering or event recording.
4. Capturing state is Arc-only; restoration clones S because Store's public `Fx::set` takes
   owned S. Use Arc/persistent state fields for cheap restoration. Effects are not replayed;
   existing queued events keep FIFO order. Arrays are atomic in JSON diffs.
5. Long event logs are paginated eight at a time, and the view counter list displays its
   first twelve registrations. State diff descriptions are user-selected diagnostic data.
