# DataGrid production readiness

Audit and follow-up on `feat-prod-a` from `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ✅ | [Interaction tests](tests/interaction.rs) mount 10M computed rows × 50 columns and 1M materialised rows. The [scroll suite](../../bench/src/suites.rs) adds 10M-row virtual-window, pan and formatting coverage. |
| §2 Frame and cold mount | ⚠️ | The scroll suites measure model access and formatting. The [headless soak](../../bench/examples/production_soak.rs) times scripted updates and draws, with night-only p99/max gates. [Mounted complexity checks](tests/interaction.rs) compare work at 100k and 10M rows. Quiet-host run pending for the frame and mount budgets. Desktop presentation latency remains open. |
| §2 Background work | ⚠️ | [Grid](src/grid.rs) computes large permutations off-thread with supersession. Background sorting prepares its inverse before publication. Key collection, sorting runs, merge passes and the inverse build poll cancellation. Dropping the core signals cancellation. The [select-all filter test](tests/interaction.rs) verifies compact whole-source selections. Other large selection remaps still run on the UI thread. |
| §3 Memory and allocation | ⚠️ | [Row order](src/order.rs) uses 32-bit forward and inverse indices. [Allocator tests](../../bench/tests/allocator.rs) gate model scrolling, sort recompute and forward/inverse overhead. Forward plus inverse indices use 8 B per row on x86-64 before container headers. The scroll path streams dirty slots without a scratch vector. The [mounted allocation check](../../bench/tests/allocator.rs) gates viewport-scale requests. The zero-allocation GPUI paint target remains open. |
| §4 Model and importer checks | ✅ | [Order properties](src/order.rs) cover stable sorting and filtering. [Editing tests](tests/editing.rs) exercise editing transactions. [Undo, selection and clipboard models](src/model/) also have property tests. Ragged TSV storage is linear in explicit fields. The [clipboard fuzz target](../../fuzz/fuzz_targets/clipboard_tsv.rs) checks malformed input and canonical round trips in the [fuzz smoke workflow](../../.github/workflows/model-fuzz.yml). The ordering properties also compare a sequence of filter and multi-key sort updates and their inverse against a reference. |
| §4 Edges and concurrency | ⚠️ | [Source tests](src/source.rs) include numeric ordering. [Interaction tests](tests/interaction.rs) cover keyboard and mouse behavior. Added invalid-viewport and exact mixed numeric comparisons, plus a mounted long RTL/CJK/emoji cell test. An interaction regression covers a scroll and selection burst, palette reuse, theme changes and focus. Mounted DPI coverage remains open. |
| §5 Soak, RSS and streaming | ⚠️ | The [headless soak](../../bench/examples/production_soak.rs) runs a structural smoke in the bench tests. Its nightly profile samples percentiles and RSS every 10 s and gates frame time and RSS growth. Inspect/drive replay of edits and full 30-minute evidence remain open. |
| §6 Accessibility | ⚠️ | [Interaction tests](tests/interaction.rs) drive a 10M-row jump and repeated keyboard movement, then verify bounded virtual row nodes and their labels. Screen-reader announcements and extended keyboard replay remain open. |
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

An instrumented release headless run used the bench counting allocator on
the calling thread for each of 60 warmed scroll, selection and draw
iterations. Median traffic was 23,520 allocation operations and 14,674,820
requested bytes per iteration. The measurement includes GPUI work and
measures allocation traffic. It leaves the zero-allocation UI criterion open.

Run the headless smoke with
`cargo run --release -p guiraffe-bench --example production_soak -- grid`.
Append `--nightly` for the 30-minute threshold run on a quiet Linux host.

## Follow-up profiling

The before source was `7859897`. The after source includes the selection
notification change in `2856359`. The same headless fixture ran with 60 warmed
samples per build. Both builds used `-C opt-level=3` for the component and
harness, with shared cached dependencies. One-minute host loads ranged from
108 to 114 with concurrent builds.

| Grid metric | Before | After |
|---|---:|---:|
| Headless p50 (ms) | 156.357 | 126.911 |
| Headless p99 and max (ms) | 258.512 | 258.258 |
| Median allocations per frame | 23,522 | 23,521 |
| Median requested bytes per frame | 14,674,472 | 14,674,440 |

The observed p99 breached the budget. The average update and explicit
draw stages took 62.051 ms and 75.435 ms after the change. These stages
include backend work. The scripted viewport jumps recycle most row slots.

A separate 15-sample optimized model run measured 1M-row sorting at
822.076 ms median before and 1,090.147 ms after adding cancellation
checkpoints. Starting one-minute loads were 90.58 and 90.32 respectively.
Counting allocations for the same input yielded four allocations and
56,000,016 requested bytes in both builds.
The [model checks](src/order.rs) exercise cancellation during a merge pass
and verify the stable permutation against a reference.

## Deterministic mounted frame checks

The [CI instruction check](../../.github/workflows/ci.yml) caps the growth in
instructions from 100k to 10M items at 2%.
The release iai-callgrind gate measured 37,196,952 instructions at 100k
rows and 37,185,423 instructions at 10M rows.

The [instruction suite](../../bench/benches/production_frames.rs) warms a 1,200 by
800 headless viewport and measures one scroll and draw. The [allocator
test](../../bench/tests/allocator.rs) requires equal allocation counts at
100k and 10M rows. It caps each draw at 15,000 allocations and 7 MiB of
requests. The [mounted interaction test](tests/interaction.rs) requires
equal source reads, row renders, constructed cells and pool size.

The Callgrind captures below used the same fixture and cached dependencies
with optimized component and harness builds, from `7859897` to the flattened
cell layout. The warmed captures revisit the same two ranges in both datasets.

| DataGrid source size | Instructions before → after | Allocations before → after | Requested bytes before → after |
|---|---:|---:|---:|
| 100k rows | 46,863,204 → 42,034,465 | 15,441 → 14,181 | 7,752,622 → 6,621,646 |
| 10M rows | 46,833,998 → 42,048,645 | 15,441 → 14,181 | 7,752,622 → 6,621,646 |

Removing the extra text wrapper reduced layout work while the two
[visual checks](tests/visual.rs) passed with the existing goldens.
