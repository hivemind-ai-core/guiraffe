# CodeViewer production readiness

Audit and follow-up on `feat-prod-a` from `62b8de3`, against [spec/23](../../spec/23-production-readiness.md).
⚠️ marks criteria with outstanding checks or implementation.

| Criterion | Status | Evidence and remaining check |
|---|---|---|
| §1 Scale | ✅ | [Mounted tests](tests/interaction.rs) cover 10M lines and a 1 MB multilingual line. [Bench suites](../../bench/src/suites.rs) cover 10M-line model scroll and index construction. |
| §2 Frame and cold mount | ⚠️ | [Paint](src/paint.rs) bounds visible rows and shaped bytes per source line. The [headless soak](../../bench/examples/production_soak.rs) times updates and draws, with night-only p99/max gates. [Mounted complexity checks](tests/interaction.rs) compare work at 10k and 10M lines. Quiet-host run pending for the frame and mount budgets. Desktop presentation latency remains open. |
| §2 Background work | ⚠️ | [Surface](src/surface.rs) dispatches analysis and indexing off-thread and restricts publication to the correct revision. Supersession and drop stop indexing at checkpoints and skip later highlight phases. An in-progress parser/style call still runs to its next phase boundary. The [coalescing test](tests/interaction.rs) verifies the queued and completed states exposed by `CodeHandle::is_analyzing`. |
| §3 Memory and allocation | ⚠️ | [Document](src/document.rs) shares source bytes and retains a line-start index. Each offset takes 4 B for source up to 4 GiB minus one byte and one native word for larger source. This excludes shared source and the fixed minimap summary. Line hashes are computed on demand. [Allocator tests](../../bench/tests/allocator.rs) cover warmed model scrolling and exact index allocation. The [mounted allocation check](../../bench/tests/allocator.rs) gates viewport-scale requests. The zero-allocation GPUI paint target remains open. |
| §4 Model and parser checks | ⚠️ | [Model tests](src/model.rs) and [document tests](src/document.rs) cover row mapping and positions. A split-line reference property now exercises UTF-8 and CRLF offsets. The [line-index fuzz target](../../fuzz/fuzz_targets/code_line_index.rs) compares offsets and UTF-8 columns with a split-line reference in the [fuzz smoke workflow](../../.github/workflows/model-fuzz.yml). Fold update sequences compare visible rows with a naive filter. Bounded malformed syntax analysis remains open. |
| §4 Edges and concurrency | ⚠️ | [Interaction tests](tests/interaction.rs) cover rapid revision publication and viewport interactions. The long-line test includes RTL, CJK and emoji and verifies full copying. The viewport model handles zero and extreme finite dimensions. A mounted scroll burst and theme change preserve selection and keyboard copy. Mounted DPI and scale input bursts remain open. |
| §5 Soak, RSS and streaming | ⚠️ | The [headless soak](../../bench/examples/production_soak.rs) runs a structural smoke in the bench tests. Its nightly profile samples percentiles and RSS every 10 s and gates frame time and RSS growth. Inspect/drive replay of source updates and full 30-minute evidence remain open. |
| §6 Accessibility | ⚠️ | [Interaction tests](tests/interaction.rs) cover keyboard navigation, selection and copy. The 10M-line test drives repeated page movement in both directions. The [virtual-node test](src/surface.rs) queries a bounded window of source lines with absolute positions. Native screen-reader announcements remain open. |
| §6 Visual snapshots | ✅ | [Snapshots](tests/snapshots.rs) cover the top, middle and end of the 10M-line viewer in both themes and both backends. |
| §7 Review and baseline | ⚠️ | Scoped Cargo tests and all-target Clippy passed on the upstream backend. The [interaction checks](tests/interaction.rs) also passed with cached GPUI-fast dependencies. The full release gate and independent review remain open. |

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

## Follow-up profiling

The before source was `7859897`. The after source includes the gutter
buffer reuse in `2856359`. The same headless fixture ran with 60 warmed
samples per build. Both builds used `-C opt-level=3` for the component and
harness, with shared cached dependencies. One-minute host loads ranged from
107 to 113 with concurrent builds.

| CodeViewer metric | Before | After |
|---|---:|---:|
| Headless p50 (ms) | 1.312 | 5.398 |
| Headless p99 and max (ms) | 36.454 | 55.061 |
| Median allocations per frame | 302 | 138 |
| Median requested bytes per frame | 194,468 | 192,500 |

The allocation counts show the effect of reusing the gutter buffer. Timing
varied across repeated runs and exceeded the frame budget in this capture.

## Deterministic mounted frame checks

The [CI instruction check](../../.github/workflows/ci.yml) caps the growth in
instructions from 100k to 10M items at 2%.
The release iai-callgrind gate measured 6,393,770 instructions at 100k
lines and 6,460,295 instructions at 10M lines.

The [instruction suite](../../bench/benches/production_frames.rs) measures a
warmed scroll and draw in a 1,200 by 800 headless viewport. The [allocator
test](../../bench/tests/allocator.rs) requires equal allocation counts at
100k and 10M lines. It caps each draw at 160 allocations and 192 KiB of
requests. The [mounted interaction test](tests/interaction.rs) requires
equal visible row counts, shaped-line counts and shape-cache hits.

The Callgrind captures below used the same fixture and cached dependencies
with optimized component and harness builds, from `7859897` to the gutter
buffer reuse. The warmed captures revisit the same two ranges in both datasets.

| CodeViewer source size | Instructions before → after | Allocations before → after | Requested bytes before → after |
|---|---:|---:|---:|
| 100k lines | 8,270,193 → 8,247,657 | 219 → 137 | 175,132 → 174,476 |
| 10M lines | 8,350,548 → 8,316,344 | 301 → 137 | 176,444 → 174,476 |

The same fixture linked against cached GPUI-fast dependencies requested
154 allocations and 21,088 bytes at each source size.


## Compact document index

| 10M lines in a 20 MB source | Before | After |
|---|---:|---:|
| Index allocation requests | 2 | 1 |
| Index bytes requested | 160,000,000 | 40,000,000 |
| Isolated peak RSS (KiB) | 178,204 | 60,808 |
| Isolated index construction (ms, one loaded-host sample) | 166.326 | 77.696 |
| Isolated instructions, including identical source generation | 1,669,895,840 | 1,379,895,455 |
| Mounted renderer-backed RSS (KiB) | 351,796 | 237,816 |

The isolated probe constructs the document from a shared source and excludes
GPUI and the headless renderer. The before mounted Samply capture attributed
29.33% of main-thread samples to document construction inclusive.
Source lines up to 64 bytes can share shapes across unstyled rows. Longer or
styled lines key by source row and avoid hashing source text during paint.
Each viewer's shape cache retains at most 2,048 entries in insertion order,
with at most 256 KiB of UTF-8 input across all entries and 16 KiB per entry.
GPUI owns its own glyph and layout caches.


The same 10M-line mounted CPU-only fixture observed 87,740 KiB RSS when
GPUI's WGPU renderer callback was disabled. The full mounted probe still
exceeds the process-wide RSS budget. The renderer-backed and CPU-only
runs both use GPUI event handling, layout and scene construction.
