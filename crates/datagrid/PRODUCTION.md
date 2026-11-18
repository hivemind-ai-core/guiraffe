# DataGrid production readiness

Audit and follow-up on `feat-prod-a` from `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ⚠️ | [Interaction tests](tests/interaction.rs) mount 10M computed rows × 50 columns and 1M materialised rows. The [scroll suite](../../bench/src/suites.rs) adds 10M-row virtual-window, pan and formatting coverage. |
| §2 Frame and cold mount | ⚠️ | The scroll suites measure model access and formatting. The [headless soak](../../bench/examples/production_soak.rs) times scripted updates and draws, with night-only p99/max gates. Desktop input-to-frame and first useful frame remain open. |
| §2 Background work | ⚠️ | [Grid](src/grid.rs) computes large permutations off-thread with supersession. Background sorting prepares its inverse before publication. Key collection polls cancellation and dropping the core signals cancellation. Stable sort runs between checks and large selection remapping still runs on the UI thread. |
| §3 Memory and allocation | ⚠️ | [Row order](src/order.rs) uses 32-bit forward and inverse indices. [Allocator tests](../../bench/tests/allocator.rs) gate model scrolling, sort recompute and forward/inverse overhead. Forward plus inverse indices use 8 B per row on x86-64 before container headers. GPUI warm paint allocation remains open. |
| §4 Model and importer checks | ⚠️ | [Order properties](src/order.rs) cover stable sorting and filtering. [Editing tests](tests/editing.rs) exercise editing transactions. [Undo, selection and clipboard models](src/model/) also have property tests. Ragged TSV storage is now linear in explicit fields. Cargo-fuzz coverage remains open. |
| §4 Edges and concurrency | ⚠️ | [Source tests](src/source.rs) include numeric ordering. [Interaction tests](tests/interaction.rs) cover keyboard and mouse behavior. Added invalid-viewport and exact mixed numeric comparisons, plus a mounted long RTL/CJK/emoji cell test. An interaction regression covers a scroll and selection burst, palette reuse, theme changes and focus. Mounted DPI coverage remains open. |
| §5 Soak, RSS and streaming | ⚠️ | The [headless soak](../../bench/examples/production_soak.rs) has short and nightly profiles, 10 s percentile/RSS samples and a 2% drift gate. Inspect/drive replay of edits and full 30-minute evidence remain open. |
| §6 Accessibility and snapshots | ⚠️ | [Visual tests](tests/visual.rs) cover cursor and edit state in both themes/backends. Add scale snapshots and verify focus continuity and accessible announcements and virtual indices. |
| §7 Review and baseline | ⚠️ | Independent review and quiet-host baselines pending. |

## Loaded-host measurements

AMD Ryzen 7 8745HS, 16 logical CPUs, powersave governor, rustc 1.98.0
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

Run the headless smoke with
`cargo run --release -p guiraffe-bench --example production_soak -- grid`.
Append `--nightly` for the 30-minute threshold run on a quiet Linux host.
