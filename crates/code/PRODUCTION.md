# CodeViewer production readiness

Initial audit at `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ⚠️ | [Bench suites](../../bench/src/suites.rs) start at 1M lines. Exercise 10M lines and a 1 MB line through a mounted viewer. |
| §2 Frame and cold mount | ⚠️ | [Paint](src/paint.rs) bounds visible rows and shaped bytes per source line. Measure input to frame, first useful frame and maximum UI task at scale. |
| §2 Background work | ⚠️ | [Surface](src/surface.rs) dispatches analysis and indexing off-thread and restricts publication to the correct revision. Verify compute cancellation on supersede/drop and progress. |
| §3 Memory and allocation | ⚠️ | [Document](src/document.rs) shares source bytes and retains a line-start index and hash per line. Measure retained capacity, cache bounds, warm paint allocations and recompute allocations. |
| §4 Model and parser checks | ⚠️ | [Model tests](src/model.rs) and [document tests](src/document.rs) cover row mapping and positions. Extend reference-model checks over edits to view state and verify bounded analysis on malformed source. |
| §4 Edges and concurrency | ⚠️ | [Interaction tests](tests/interaction.rs) cover rapid revision publication and viewport interactions. Add zero viewport, long multilingual text, DPI/theme changes and large bursts at scale. |
| §5 Soak, RSS and streaming | ⚠️ | Add short CI and 30-minute inspect/drive replay with source updates, frame percentiles, RSS and drift checks. |
| §6 Accessibility and snapshots | ⚠️ | [Snapshots](tests/snapshots.rs) cover both themes/backends. Add scale snapshots and verify keyboard-only navigation, selection announcements and virtual indices. |
| §7 Review and baseline | ⚠️ | Independent review and quiet-host baselines pending. Historical timings are qualified in [VALIDATION.md](VALIDATION.md). |
