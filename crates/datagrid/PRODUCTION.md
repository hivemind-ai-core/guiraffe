# DataGrid production readiness

Audit and follow-up on `feat-prod-a` from `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ✅ | [Interaction tests](tests/interaction.rs) mount 10M computed rows × 50 columns and 1M materialised rows. The [scroll suite](../../bench/src/suites.rs) adds 10M-row virtual-window, pan and formatting coverage. |
| §2 Frame and cold mount | ⚠️ | The scroll suites measure model access and formatting. The [headless soak](../../bench/examples/production_soak.rs) times scripted updates and draws, with night-only p99/max gates. Desktop input-to-frame and first useful frame remain open. |
| §2 Background work | ⚠️ | [Grid](src/grid.rs) computes large permutations off-thread with supersession. Background sorting prepares its inverse before publication. Key collection polls cancellation and dropping the core signals cancellation. Stable sort runs between checks and large selection remapping still runs on the UI thread. |
| §3 Memory and allocation | ⚠️ | [Row order](src/order.rs) uses 32-bit forward and inverse indices. [Allocator tests](../../bench/tests/allocator.rs) gate model scrolling, sort recompute and forward/inverse overhead. Forward plus inverse indices use 8 B per row on x86-64 before container headers. The scroll path streams dirty slots without a scratch vector. GPUI warm paint allocation remains open. |
| §4 Model and importer checks | ⚠️ | [Order properties](src/order.rs) cover stable sorting and filtering. [Editing tests](tests/editing.rs) exercise editing transactions. [Undo, selection and clipboard models](src/model/) also have property tests. Ragged TSV storage is now linear in explicit fields. Cargo-fuzz coverage remains open. |
| §4 Edges and concurrency | ⚠️ | [Source tests](src/source.rs) include numeric ordering. [Interaction tests](tests/interaction.rs) cover keyboard and mouse behavior. Added invalid-viewport and exact mixed numeric comparisons, plus a mounted long RTL/CJK/emoji cell test. An interaction regression covers a scroll and selection burst, palette reuse, theme changes and focus. Mounted DPI coverage remains open. |
| §5 Soak, RSS and streaming | ⚠️ | The [headless soak](../../bench/examples/production_soak.rs) runs a structural smoke in the bench tests. Its nightly profile samples percentiles and RSS every 10 s and gates frame time and RSS growth. Inspect/drive replay of edits and full 30-minute evidence remain open. |
| §6 Accessibility | ⚠️ | [Interaction tests](tests/interaction.rs) verify keyboard operation, focus continuity across the scheme switch and virtual row indices. Screen-reader announcements and sustained keyboard operation at scale remain open. |
| §6 Visual snapshots | ✅ | [Visual tests](tests/visual.rs) cover the top, middle and end of the computed 10M-row grid in both themes and both backends. Existing snapshots cover selection and editing. |
| §7 Review and baseline | ⚠️ | Default test batch (component, Bench and Showcase), fast-backend component and Showcase tests, and all-target Clippy on both backends exited 0. The added scale goldens passed separate comparisons on both backends. Independent review and quiet-host baselines remain open. |

## Loaded-host measurements

AMD Ryzen 7 8745HS, Radeon 780M Graphics, 16 logical CPUs, powersave
governor, rustc 1.98.0
with `-C opt-level=3`, 15 iterations, load averages 23.75 / 16.60 / 13.18.
The 1M-row computed integer sort took 254.195 ms median and 348.396 ms
maximum. The inverse took 4.184 ms median and 12.199 ms maximum. These
timings exclude mounting, rendering and publishing the order.

The registered Cargo release benchmarks at `0ec4a28` recorded 2.234 ms
median and 6.052 ms maximum per batch of 32 model viewport jumps, and
210.997 ms median and 620.487 ms maximum per 1M-row sort (15 samples,
load averages 34.52 / 26.06 / 18.31). The initial release headless nightly
run stopped in the first interval at 137.886 ms p99 and 165.159 ms
maximum, with 103,432 KiB RSS. Host load at start was 34.36 / 25.59 /
18.03. This run did not establish 30-minute drift.

After sharing the theme palette and timing mount inside an initialized headless
window, `add4f7c` mounted in 85.846 ms. The first 10 s of nightly sampling
reported 76.584 ms p99 and 105.473 ms maximum before the long-task gate
failed. Starting load was 25.40 / 31.00 / 24.48. In adjacent 60-frame runs
at load 32, the observed median moved from 36.258 to 20.798 ms
and p99 from 55.115 to 41.118 ms after palette reuse. Desktop presentation latency remains
unmeasured.

Run the headless smoke with
`cargo run --release -p guiraffe-bench --example production_soak -- grid`.
Append `--nightly` for the 30-minute threshold run on a quiet Linux host.
