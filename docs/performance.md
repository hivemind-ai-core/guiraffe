# Performance

## Budgets

Release build, both backends:

- mount to first useful frame in at most 200 ms;
- p99 of 8 ms or less for scroll, pan, zoom, hover and selection, and no main-thread task over 50 ms;
- peak memory over an empty window of at most 1.25 × the raw data plus 64 MiB;
- at most 2 % memory growth over a 30-minute soak;
- per-frame work of O(viewport + log n), and no allocations when repainting an unchanged view.

0.2 does not meet all of these. The misses are listed under [what misses](#what-misses).

## At scale

Measured 2026-11-22 on a Ryzen 7 8745HS under Linux, headless, over two 10-second runs each.
The machine was not idle (load 1.1 to 1.9), so treat these as indicative.

| Scenario | Data | Mount ms (upstream / fast) | Worst p99 ms (upstream / fast) |
|---|---|---:|---:|
| DataGrid | 10M rows × 50 columns | 4.6 / 4.8 | 1.53 / 1.84 |
| Sheet | 100k formulas | 27.4 / 28.4 | 2.74 / 1.72 |
| CodeViewer | 10M lines | 162.7 / 163.7 | 0.62 / 0.42 |
| Flow | 50k nodes, 100k edges | 68.0 / 67.7 | 4.61 / 4.09 |
| Dope sheet | 100k keys | 12.2 / 13.2 | 6.45 / 6.20 |
| Gantt | 100k tasks | 28.6 / 26.9 | 2.15 / 2.23 |
| Tracks | 100k clips | 8.5 / 8.4 | 3.03 / 2.54 |
| Line chart | 10M points | 6.0 / 6.4 | 2.15 / 1.90 |
| Scatter chart | 1M points | 12.6 / 5.4 | 1.50 / 1.47 |
| Flame graph | 10M frames | 3.4 / 3.1 | 1.12 / 0.88 |
| Tree | 1M nodes | 69.3 / 69.9 | 1.23 / 1.27 |
| Property grid | 1M properties | 70.1 / 69.9 | 1.69 / 1.48 |
| Markdown | 50k lines | 26.6 / 27.1 | 2.55 / 4.88 |
| Mermaid | 500 nodes | 62.5 / 65.3 | 0.55 / 0.47 |
| Chat | 100k messages | 43.7 / 45.4 | 2.83 / 3.73 |

## What misses

- **Middle sizes are slower than the largest**, because level of detail only helps at the top:
  flame graphs reach 96 ms p99 at 262k frames upstream, and Tracks 53 ms at 12.8k clips.
- **Memory:** Sheet with 100k formulas uses about 120 MiB, and CodeViewer peaks at 379 MiB while
  loading 10M lines.
- **Soak:** process memory grows about 6 % over 30 minutes. The leak check finds no leak.
- **Allocation:** zero on an unchanged repaint is gated for some components only. DataUi, `ui!`,
  the command menu and devtools still allocate (42 to 735 per repaint).
- **Wide live trees:** editing one label among 3,000 non-virtualized rows takes 19 to 46 ms,
  because work follows the size of the tree. Fixing this is the center of [0.3](../ROADMAP.md).

Details and workarounds are in [known issues](../KNOWN_ISSUES.md).

## Against Electron

[bench/electron](../bench/electron/README.md) holds the same app written with GUIraffe and with
Electron + React. These figures are **preliminary**: two runs of each app in one session on
2026-11-22, on a machine that was not idle.

| | GUIraffe | Electron + React |
|---|---:|---:|
| Warm start to first frame | 135 ms | 349 ms |
| Idle memory (PSS, all processes) | 93 MiB | 305 MiB |
| Peak memory under load (PSS) | 147 MiB | 999 MiB |
| Installed size | 53 MiB | 276 MiB |
| Frame interval p99, across four scenarios | 17.1–17.7 ms | 17.8–20.2 ms |

## Reproduce it

```sh
cargo build --release -p guiraffe-bench --example soak --example limits
target/release/examples/soak flamegraph --short    # one scenario at full size
SOAK_ITEMS=262144 target/release/examples/soak flamegraph --short
target/release/examples/soak mixed                 # 30-minute mixed soak
target/release/examples/limits                     # largest size that stays within budget
bench/electron/run.sh                              # needs an idle machine with a display
```

Add `--features gpui-kit/gpui-fast` to the build for the other backend.
See [bench](../bench/README.md) for suites, baselines and regression comparison.

### Sheet and CodeViewer load memory

On Linux, the Sheet and CodeViewer RSS tests run the full-size mounted fixture and an empty
window in separate processes. They compare the median of five peak RSS differences with
the raw-data budget. Sheet waits for the last cell of its 100,000-formula chain to publish,
changes the first input, and verifies the new last value. CodeViewer streams 199,999,999
UTF-8 bytes into its shared source and builds the 10-million-line index.

An optimized, renderer-free probe of the same CodeDocument source measured 392,988 KiB
peak process RSS when converting the generated String into an Arc, and 197,744 KiB when
reading into the final allocation. Both runs checked the line count. Another process kept
the 100,000-formula SheetModel and the first published result live during a second full
recalculation. Peak RSS fell from 105,784 KiB at 34c59679 to 59,172 KiB. The model stores
single occupancy intervals inline. The calculation worker stores reverse edges in one
ordered set, compiles cell/constant expressions in one allocation, and publishes values
from its working snapshot. These process peaks include model construction and exclude
the mounted window.

### Mixed-soak RSS attribution

On 2026-11-25, two concurrent upstream headless mixed soaks at 7a38af63 ran for
30 minutes in the Cargo development profile on a busy Linux host. Both ran the normal
full-size fixture. One used glibc defaults and the other used `MALLOC_ARENA_MAX=1`
and `MALLOC_TRIM_THRESHOLD_=0`. At each ten-second sample an external process summed
RSS by mapping from `/proc/PID/smaps`. An in-process sampler thread returned glibc
`mallinfo2()` to the observer. The first sample was the warmed baseline.

| Change at peak process RSS, KiB | Default malloc | Single trimmed arena |
|---|---:|---:|
| Process RSS | +4,604 | +3,084 |
| Resident pages in the process heap mapping | +4,600 | +2,932 |
| Arena size | +4,532 | +2,956 |
| Free bytes within arenas | +4,777 | +1,856 |
| In-use bytes within arenas | −245 | +1,100 |
| Separately mmap-allocated bytes | 0 | +584 |

The default allocator retained additional space after objects were freed. In the trimmed
run, free arena space accounted for 63% of the additional arena size at peak. The
remaining arena growth held in-use allocations. The fixture had accepted 31 additional
chat messages and 60,240 additional bytes of chat stream payload by that sample.
Allocator byte counts describe allocation state, whereas the mapping counts measure
resident pages. These samples cannot assign every in-use allocation to a widget.

At the trimmed run's final sample the arena still held 2,381 KiB more free space and
511 KiB more in-use space than at baseline. Process RSS had fallen by 11,332 KiB as
13,864 KiB of executable pages left the resident set. Both diagnostic runs crossed
the warmed RSS limit and returned failure. The development-profile runs quantify the
source of growth on this host. The earlier release measurements for both backends
remain in the 0.2 table.
