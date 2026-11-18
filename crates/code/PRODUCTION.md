# CodeViewer production readiness

Audit and follow-up on `feat-prod-a` from `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ✅ | [Mounted tests](tests/interaction.rs) cover 10M lines and a 1 MB multilingual line. [Bench suites](../../bench/src/suites.rs) cover 10M-line model scroll and index construction. |
| §2 Frame and cold mount | ⚠️ | [Paint](src/paint.rs) bounds visible rows and shaped bytes per source line. The [headless soak](../../bench/examples/production_soak.rs) times updates and draws, with night-only p99/max gates. Desktop input-to-frame and first useful frame remain open. |
| §2 Background work | ⚠️ | [Surface](src/surface.rs) dispatches analysis and indexing off-thread and restricts publication to the correct revision. Supersession and drop stop indexing at checkpoints and skip later highlight phases. An in-progress parser/style call still runs to its next phase boundary. Progress reporting remains open. |
| §3 Memory and allocation | ⚠️ | [Document](src/document.rs) shares source bytes and retains a line-start index and hash per line. Retained index metadata takes 16 B per line on x86-64, excluding shared source and the fixed minimap summary. [Allocator tests](../../bench/tests/allocator.rs) cover warmed model scrolling and exact index allocation. GPUI paint allocation remains open. |
| §4 Model and parser checks | ⚠️ | [Model tests](src/model.rs) and [document tests](src/document.rs) cover row mapping and positions. A split-line reference property now exercises UTF-8 and CRLF offsets. The [line-index fuzz target](../../fuzz/fuzz_targets/code_line_index.rs) compares offsets and UTF-8 columns with a split-line reference in the [fuzz smoke workflow](../../.github/workflows/model-fuzz.yml). Fold update sequences compare visible rows with a naive filter. Bounded malformed syntax analysis remains open. |
| §4 Edges and concurrency | ⚠️ | [Interaction tests](tests/interaction.rs) cover rapid revision publication and viewport interactions. The long-line test includes RTL, CJK and emoji and verifies full copying. The viewport model handles zero and extreme finite dimensions. A mounted scroll burst and theme change preserve selection and keyboard copy. Mounted DPI and scale input bursts remain open. |
| §5 Soak, RSS and streaming | ⚠️ | The [headless soak](../../bench/examples/production_soak.rs) runs a structural smoke in the bench tests. Its nightly profile samples percentiles and RSS every 10 s and gates frame time and RSS growth. Inspect/drive replay of source updates and full 30-minute evidence remain open. |
| §6 Accessibility | ⚠️ | [Interaction tests](tests/interaction.rs) cover keyboard navigation, selection and copy. The 10M-line test drives repeated page movement in both directions. Screen-reader announcements and virtual indices remain open. |
| §6 Visual snapshots | ✅ | [Snapshots](tests/snapshots.rs) cover the top, middle and end of the 10M-line viewer in both themes and both backends. |
| §7 Review and baseline | ⚠️ | Default test batch (component, Bench and Showcase), fast-backend component and Showcase tests, and all-target Clippy on both backends exited 0. The added scale goldens passed separate comparisons on both backends. Independent review and quiet-host baselines remain open. Historical timings are qualified in [VALIDATION.md](VALIDATION.md). |

## Loaded-host measurements

AMD Ryzen 7 8745HS, Radeon 780M Graphics, 16 logical CPUs, powersave
governor, rustc 1.98.0
with `-C opt-level=3`, 15 iterations, load averages 13.57 / 12.24 / 11.50.
Constructing the index and hashes for a shared 10M-line source took
310.132 ms median and 471.783 ms maximum. This excludes source creation,
syntax analysis and the first rendered frame.

The registered Cargo release benchmark at `0ec4a28` recorded 13.878 µs
median and 84.531 µs maximum per batch of 32 model viewport jumps, and
666.063 ms median and 1,133.776 ms maximum per 10M-line index build
(15 samples, load averages 34.96 / 26.29 / 18.43). The release headless
short run reached 3.952 ms p99 over 60 draw iterations. The nightly run
stopped in its second interval at 8.922 ms p99 (8 ms budget), with a
13.363 ms maximum and 264,104 KiB RSS. This run did not establish 30-minute drift.

With a window initialized before timing, `add4f7c` mounted the
pre-indexed document in 60.657 ms. The nightly run stopped after 200 s at
10.119 ms p99 and 14.549 ms maximum in its failing interval. Starting
load was 26.88 / 30.91 / 24.59. RSS fell from 273,212 KiB at 10 s to
65,092 KiB at failure under host memory pressure. A full steady-state
30-minute drift result remains open.

An instrumented release headless run used the bench counting allocator on
the calling thread for each of 60 warmed scroll, selection and draw
iterations. Median traffic was 302 allocation operations and 194,468
requested bytes per iteration. The measurement includes GPUI work and
measures allocation traffic. It leaves the zero-allocation UI criterion open.

Run the headless smoke with
`cargo run --release -p guiraffe-bench --example production_soak -- code`.
Append `--nightly` for the 30-minute threshold run on a quiet Linux host.
