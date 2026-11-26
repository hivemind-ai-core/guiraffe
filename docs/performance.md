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
  loading 10M lines in 0.2. Both 0.3 mounted regression gates now pass on both backends
  ([measurements](#sheet-and-codeviewer-load-memory)).
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

## 0.3 comparison baseline

The 0.2 measurements preserved at revision `34c5967` include the following
middle-size peaks. Counts for Tracks include all 100 tracks. Times are p99
milliseconds, rounded as in the original results.

| Scenario | Items | Upstream | gpui-fast |
|---|---:|---:|---:|
| Flame graph | 65,536 | 14.6 | unreported |
| Flame graph | 131,072 | 39 | unreported |
| Flame graph | 262,144 | 96 | ~18 |
| Flame graph | 10,000,000 | 1.12 | 0.88 |
| Tracks | 6,400 | 26 | 17 |
| Tracks | 12,800 | 53 | 32 |
| Tracks | 100,000 | 3.03 | 2.54 |

The historical 30-minute mixed soak grew process memory by 5.8% upstream
and 5.7% on gpui-fast. The heap profile showed flat live heap over 9,600
frames. Trimming a single malloc arena halved upstream growth. Less than
2 MiB remained unexplained. Compare future runs on the same allocator
settings and distinguish process growth from live heap.

For a quick Linux replay on a busy host, build the release soak example
and run `python3 bench/check-mounted-baseline.py target/release/examples/soak upstream`.
Pass `fast` when using the gpui-fast build. Each size runs in a fresh
process and reports peak RSS, mount time, p99 and the ratio to the
[recorded values](../bench/baselines/mounted-0.2.csv). The replay uses
the shorter limits probe at every size. The original largest-size
measurements used two 10-second runs, so sampling and host load both
affect these ratios. For timing comparisons on comparable hosts, supply
`--max-ratio FACTOR` to fail above a chosen multiple of the reference.

The default guiraffe-bench composed test gates load-independent work at
1,000, 3,000 and 10,000 mounted rows. At each size an edit rendered one
cached row and historically visited one selector per row. The synchronous
equal field write allocates zero times. As a timing reference, the busy-host
release run at `ab30919` measured cached label-edit p99s of 13.92, 46.37
and 147.41 ms upstream, and 14.22, 50.03 and 158.50 ms on gpui-fast.

### Sheet and CodeViewer load memory

On Linux, the Sheet and CodeViewer RSS tests run the full-size mounted fixture and an empty
window in separate processes. They compare the median of five peak RSS differences with
the raw-data budget. Sheet waits for the last cell of its 100,000-formula chain to publish,
changes the first input, and verifies the new last value. CodeViewer streams 199,999,999
UTF-8 bytes into its shared source and builds the 10-million-line index.

The Cargo test profile measured the following peak differences in KiB above the empty
window. Each value is the median of five fresh loaded/baseline process pairs.

| Fixture | Upstream | gpui-fast | Budget |
|---|---:|---:|---:|
| Sheet, 100,000 formulas, initial and edited calculation | 59,224 | 59,988 | 69,443 |
| CodeViewer, 10 million lines | 197,144 | 197,044 | 309,677 |

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

### Controlled component sweep

The release worktree retains the upstream and fast soak executables in
`target/sweep-bin/`. Once the host is quiet, run both size sweeps from that worktree
with one command.

```sh
python3 scripts/component-sweep.py target/sweep-bin/soak-up target/sweep-bin/soak-fast
```

The sweep checks mount time and interaction p99 for FlameGraph, Tracks, the dope sheet
and Mermaid, followed by the maximum dope-sheet frame time across four mixed replay
cycles. It exits nonzero if either backend misses any budget. For a fresh checkout,
follow the [build instructions](../bench/README.md#mounted-component-size-sweep).

## Allocations of a re-rendered view

One cached view, notified and drawn again 400 times in a page of 40, on gpui-fast in the release
profile. Every re-render goes through the patched frame. The counts are per re-render, from
`bench/tests/rerender_sites.rs`. `scripts/rerender-sites.py` attributes them to call sites under
valgrind's DHAT: each view runs at N and 2N re-renders, and the difference per site, divided by N,
is what one re-render allocates there (mounting and startup cancel out).

| View | release/0.3 (`b0913deb`) | Now |
| --- | ---: | ---: |
| Empty: one sized `div` | 29 | 8 |
| Plain: an id, a click listener and a text label | 41 | 20 |
| Component row: label, `Checkbox`, `Badge`, `Button` | 103 | 76 |
| Plain view, label edited through a keyed Store selection | not measured | 26 |

The 21 to 27 removed per view were framework overhead in gpui-fast's retained records and splice
gaps, the same for every view. For the empty view:

- **Shared empty lists (−13).** Every record began with fresh empty `Rc` slices for its dependency
  lists, hovers and layout keys, plus a placeholder context (`begin_retained`, 11), and its paint
  turned empty hover lists into new slices (`finish_retained_paint`, 2). An empty `Rc<[T]>` is an
  allocation, so these now share per-thread empties.
- **Reused gap stacks (−7).** Entering a splice gap, twice per patched view (prepaint and paint),
  built a new text-style stack, content-mask stack and element-offset stack (`enter_gap`). The
  stacks the last gap left are now reused through `clone_from` and `clear`.
- **Patch bookkeeping (−1, more with text).** The patch copied slices of the scene's slots into new
  `Vec`s and kept its line slots in one. The bounds-tree check for a moved layer collected nearby
  bounds into a fresh `Vec`, which cost a text edit 2 more. All of these now use buffers kept from
  one patch to the next, or inline storage.

What remains, component row (76):

| Category | Count | Call sites |
| --- | ---: | --- |
| Element builders | 25 | `Interactivity::new` (11, one boxed per stateful `div`); `Checkbox`, `Button` and `Badge` renders in gpui-component and gpui-base (13); the view's own `render` (1) |
| Element ids | 20 | `smol_str::Repr::new` (18): `ElementId`s formatted from strings past the inline size, in gpui-base and gpui-component; `motion::spring` transition ids (2) |
| Listener closures | 9 | `Window::on_mouse_event`, one boxed closure per mouse listener, from `Interactivity::paint_mouse_listeners` |
| Retained records | 8 | `finish_retained_prepaint` (4: the record's context, layout keys, rebuild handle and dependency lists), `finish_retained_paint` (3: hover lists and dependencies), `rebuild_here` (1) |
| Accessibility | 7 | gpui-base's `A11yDisabled` wrapper (3), `accesskit::Properties::set` (2), `Div::write_a11y_info` (2), with accessibility off |
| Text | 5 | `SharedString::request_layout` (3, the measured text node), `TextStyleStack::resolve` (2) |
| Patch bookkeeping | 2 | `finish_recording_claimed_keys` (the layout keys the patch claimed), `App::flush_effects` (1) |

The empty view's 8 are the record (`finish_retained_prepaint` 4, `rebuild_here` 1), the root
`div`'s `Interactivity` (1), the claimed layout keys (1) and the effect flush (1). The plain view
adds five listener closures (down, up, move and two click phases), its render's own allocations (3),
a second paint-record list and the text node.

A label edited through a keyed Store selection allocates 6 more than the same view drawn again
unchanged: the Store's queued event and its deferred flush (2), and measuring and laying out the
changed label (shaping, line layout, the measured node; about 4). The other 20 are the unchanged
redraw's. `patched_edit_allocates_an_unchanged_redraw_and_the_edit` gates that relation.

Everything in the table above is per element or per component and needs element-level changes:
an arena for listener closures and boxed `Interactivity`, interned ids, pooled records. That is
0.5 candidate 8 (#4), which these numbers are the starting data for.

### Frame buffers after a large frame

GPUI's per-frame element-state map, element-state access log, tab-stop history and entity access
log were cleared but never shrunk, so they kept the capacity of the largest frame a window drew.
The accepted 0.2 exception put this at 6 to 10 MB after 10,000 live elements and 64 to 99 MiB
after 100,000. gpui-fast now trims each buffer once its use has stayed under a quarter of its
capacity for 8 frames in a row, to twice what it used in those frames (`fast/high_water.rs`).
Buffers under 64 KiB are left alone. Steady frames, and frames that alternate between large and
small, never trim, so unchanged repaints still allocate nothing. After a 10,000-row frame and 12
frames of 100 rows, the element-state map goes from 1,032,192 to 16,128 bytes, the access log from
1,310,720 to 24,000, and the tab history from 524,288 to 6,400
(`frame_storage_shrinks_back_after_10k_rows`).

## Store vs plain label-edit A/B

One unattended command measures what the Store costs on a label edit:

```sh
python3 scripts/label-scaling.py --ab
```

In this workspace, run it through the build-slot wrapper:
`/home/dan/orca/anteater-cargo.sh python3 scripts/label-scaling.py --ab`.

It builds the release `composed` test for upstream and gpui-fast in
`target/label-ab-*`. Each backend runs the label-edit sweep at 1k, 3k and 10k
rows: virtualized with a visible and an offscreen edit, and fully mounted with
a visible edit. Variant A (store) is the keyed Store write. Variant B (plain)
keeps the same rows in a plain entity and notifies only the edited row's view.
The two variants share the same row view and page.

Instructions per edit come from Callgrind, over three measured edits. CPU and
allocations are per-edit medians over 31 native edits. Each line prints both
variants and the store's share, `(store - plain) / store`. The JSON report goes
to `target/label-ab.json`. The run fails when store instructions per edit
exceed plain by more than 2% of plain plus 4,000 instructions
(`LABEL_AB_TOLERANCE`, `LABEL_AB_FIXED_IR`). The keyed write is a fixed cost
of about 3,300 instructions and two allocations. On an offscreen edit plain does
almost nothing, so a ratio alone would fail. Only instructions are gated,
because CPU time depends on host load. Upstream fully mounted at 10k rows costs
about 150G instructions per edit, so that case reports CPU and allocations only
(`LABEL_AB_SKIP_IR`).

Provisional numbers from 2026-11-25 on a busy host, per edit, store vs plain:

| Backend | Layout | Edit | Rows | Store Ir | Plain Ir | Store share of Ir | Allocations |
|---|---|---|---:|---:|---:|---:|---|
| gpui-fast | virtualized | visible | 1k / 3k | 1.565M | 1.550M | 1.0% | 395 vs 393 |
| gpui-fast | virtualized | offscreen | 1k / 3k | 5,371 | 2,087 to 2,100 | 61% | 3 vs 1 |
| gpui-fast | mounted | visible | 1k | 29.47M | 29.12M | 1.2% | 185 vs 183 |
| gpui-fast | mounted | visible | 3k | 89.21M | 88.28M | 1.0% | 191 vs 189 |
| upstream | virtualized | visible | 1k / 3k | 4.001M | 3.983M | 0.5% | 312 vs 310 |
| upstream | virtualized | offscreen | 1k / 3k | 4,879 | 1,926 | 60% | 3 vs 1 |
| upstream | mounted | visible | 1k | 1.578G | 1.577G | 0.0% | 1,300 vs 1,298 |
| upstream | mounted | visible | 3k | 13.475G | 13.474G | 0.0% | 3,300 vs 3,298 |

CPU medians on that host were too noisy to compare. The quiet run replaces this table.
`LABEL_AB_ROWS=1000,3000` shortens the sweep.
