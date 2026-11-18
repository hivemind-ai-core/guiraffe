# DataGrid production readiness

Initial audit at `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ⚠️ | [Bench suites](../../bench/src/suites.rs) start at 1M computed rows × 50 columns. Exercise 10M computed rows and 1M materialised rows through a mounted grid. |
| §2 Frame and cold mount | ⚠️ | The scroll suite measures model access and formatting. Measure input to frame, first useful frame and the maximum UI task at scale. |
| §2 Background work | ⚠️ | [Grid](src/grid.rs) computes large permutations off-thread with supersession. Stable sort runs between cancellation checks, and publication builds the inverse on the UI thread when rebasing interaction state. Verify responsiveness, drop cancellation and progress. |
| §3 Memory and allocation | ⚠️ | [Row order](src/order.rs) uses 32-bit forward and inverse indices. [Allocator tests](../../bench/tests/allocator.rs) gate model scrolling and sort recompute. Test retained capacities and warm paint/scroll through GPUI. |
| §4 Model and importer checks | ⚠️ | [Order properties](src/order.rs) cover stable sorting and filtering. [Editing tests](tests/editing.rs) exercise editing transactions. Add random transaction traces and bounded TSV fuzzing. |
| §4 Edges and concurrency | ⚠️ | [Source tests](src/source.rs) include numeric ordering. [Interaction tests](tests/interaction.rs) cover keyboard and mouse behavior. Add scale runs with zero viewport, long multilingual text, DPI/theme changes and input bursts. |
| §5 Soak, RSS and streaming | ⚠️ | Add short CI and 30-minute inspect/drive replay with frame percentiles, RSS and drift checks. |
| §6 Accessibility and snapshots | ⚠️ | [Visual tests](tests/visual.rs) cover cursor and edit state in both themes/backends. Add scale snapshots and verify focus continuity and accessible announcements and virtual indices. |
| §7 Review and baseline | ⚠️ | Independent review and quiet-host baselines pending. |
