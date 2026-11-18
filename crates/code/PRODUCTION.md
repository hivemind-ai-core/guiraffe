# CodeViewer production readiness

Audit and follow-up on `feat-prod-a` from `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ⚠️ | [Mounted tests](tests/interaction.rs) cover 10M lines and a 1 MB multilingual line. [Bench suites](../../bench/src/suites.rs) cover 10M-line model scroll and index construction. |
| §2 Frame and cold mount | ⚠️ | [Paint](src/paint.rs) bounds visible rows and shaped bytes per source line. The [headless soak](../../bench/examples/production_soak.rs) times updates and draws, with night-only p99/max gates. Desktop input-to-frame and first useful frame remain open. |
| §2 Background work | ⚠️ | [Surface](src/surface.rs) dispatches analysis and indexing off-thread and restricts publication to the correct revision. Verify compute cancellation on supersede/drop and progress. |
| §3 Memory and allocation | ⚠️ | [Document](src/document.rs) shares source bytes and retains a line-start index and hash per line. Retained index metadata takes 16 B per line on x86-64, excluding shared source and the fixed minimap summary. [Allocator tests](../../bench/tests/allocator.rs) cover warmed model scrolling and exact index allocation. GPUI paint allocation remains open. |
| §4 Model and parser checks | ⚠️ | [Model tests](src/model.rs) and [document tests](src/document.rs) cover row mapping and positions. A split-line reference property now exercises UTF-8 and CRLF offsets. Bounded malformed-source analysis and additional view-state sequence coverage remain open. |
| §4 Edges and concurrency | ⚠️ | [Interaction tests](tests/interaction.rs) cover rapid revision publication and viewport interactions. The long-line test includes RTL, CJK and emoji and verifies full copying. Zero viewport, DPI and theme changes during interaction and scale input bursts remain open. |
| §5 Soak, RSS and streaming | ⚠️ | The [headless soak](../../bench/examples/production_soak.rs) has short and nightly profiles, 10 s percentile/RSS samples and a 2% drift gate. Inspect/drive replay of source updates and full 30-minute evidence remain open. |
| §6 Accessibility and snapshots | ⚠️ | [Snapshots](tests/snapshots.rs) cover both themes/backends. Add scale snapshots and verify keyboard-only navigation, selection announcements and virtual indices. |
| §7 Review and baseline | ⚠️ | Independent review and quiet-host baselines pending. Historical timings are qualified in [VALIDATION.md](VALIDATION.md). |

## Exploratory optimized model measurements

AMD Ryzen 7 8745HS, 16 logical CPUs, powersave governor, rustc 1.98.0
with `-C opt-level=3`, 15 iterations, load averages 13.57 / 12.24 / 11.50.
Constructing the index and hashes for a shared 10M-line source took
310.132 ms median and 471.783 ms maximum. This excludes source creation,
syntax analysis and the first rendered frame.

Run the headless smoke with
`cargo run --release -p guiraffe-bench --example production_soak -- code`.
Append `--nightly` for the 30-minute threshold run on a quiet Linux host.
