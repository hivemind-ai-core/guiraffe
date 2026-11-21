# Mermaid production checks

The scale fixture is a 500-node flowchart with tree edges and periodic cross
links. The real-window mount probe is `cargo run --release -p
guiraffe-mermaid --example first_frame`. Add `--features gpui-kit/gpui-fast` before any example arguments to select
gpui-fast. The workspace soak probe measures panning, zooming, hovering and
idle frames.

| Criterion | Check |
| --- | --- |
| First useful frame | The native mount probe times the first render with a published preview. Large flowcharts do placement once. Flowcharts with styles, labels, subgraphs and decorated edges use a bounded GPUI overview. The exact SVG renderer refines the same placement. The worker waits for preview paint, with a 100 ms fallback for hidden views. |
| Exact refinement | `vendor/mermaid-rs-renderer/parity/run.sh --soak` compares output to renderer 0.3.1. The staged fixture also checks node dimensions and relative placement through refinement. |
| Static repaint | The allocator regression in `bench/tests/allocator.rs` compares eight warmed draws of empty, 50-node and 500-node Mermaid views. On the release libraries all three measured 8 allocations and 448 bytes upstream, and zero on gpui-fast. A diagram added zero warm allocations on either backend. |
| Memory | The native-window probe measures peak RSS in independent processes. Pass `-- --window-only` for the empty-window control. See the paired measurements below. |

## Native 500-node measurements

The release harness runs a 1280 × 800 native Wayland window. Timings are
indicative under shared-machine load and start at source submission, after
native window creation. Empty and diagram RSS use the peak from each child
process through exit. The diagram run includes exact refinement.

| Backend | Empty peak RSS | Mermaid peak RSS | Peak difference | Mount |
| --- | ---: | ---: | ---: | ---: |
| Upstream GPUI | 103,888 KiB | 120,848 KiB | 16,960 KiB | 137 ms |
| gpui-fast | 103,172 KiB | 121,728 KiB | 18,556 KiB | 138 ms |

At `737b7eb`, upstream took 299 ms unprofiled and peaked at 120,348 KiB
with the diagram. Its 104,612 KiB window control still mounted the widget
without a source. The current control uses an empty root.

The empty window already exceeds the whole-process 64 MiB allowance for this
fixture. The peak difference describes the two runs and is an estimate of the
incremental cost. A quiet-window mount check, interaction p99, main-thread
long-task tracking and the 30-minute RSS drift gate remain open.

Samply on upstream GPUI observed 259 ms at `737b7eb` and 113 ms with the
changes. The table counts inclusive samples from the worker stacks passing
through `draw_layout` before exact routing. Rows can overlap.

| Worker phase | Before (81 samples) | After (36 samples) |
| --- | ---: | ---: |
| Mermaid parse | 8.6% | 36.1% |
| Manual placement | 39.5% | 52.8% |
| Rank computation | 16.0% | 0 sampled |
| Text measurement | 1.2% | 0 sampled |
| Preview routing | 1.2% | 0 sampled |
| SVG parse | 9.9% | 0 sampled |
| Rasterization | 29.6% | 0 sampled |

The earlier headless harness drained deterministic background execution in
the frame loop and inspected the diagram after refinement. Its first frame
was usually the refined raster. The production-headless Mermaid path in
`guiraffe-bench --example soak` uses the normal platform executor with a
hidden window. It observes the first useful draw and reports exact
refinement separately as `refined_ready_us`. The two 10-second samples drive
wheel zoom, drag, and hover through the window and include scene construction.
The refined phase starts at 1× and alternates reciprocal wheel deltas. The
short soak verifies that it observes at least one change in refined raster size.
The headless platform discards the scene before GPU submission.

## Headless 500-node follow-up

Release measurements are indicative under shared load. The baseline uses
release/0.2 at `6ef0ad6`. Its quiet-window values came from the foreman.

| Backend and run | First useful frame | Refined ready | p99 intervals | Max frame | Peak RSS | Raster size changes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Upstream baseline, quiet | raster, 2.81 s | included in mount | 15.6 ms | — | — | — |
| gpui-fast baseline, quiet | raster, 2.79 s | included in mount | 15.9 ms | — | — | — |
| Upstream baseline, local | raster, 4.09 s | included in mount | 19.90 / 0.71 ms | 28.93 ms | 34,852 KiB | — |
| Upstream, threaded probe | preview, 182 ms | 6.47 s | 0.92 / 0.54 ms | 5.30 ms | 48,460 KiB | — |
| Upstream, sustained zoom | preview, 120–204 ms | 6.11–6.68 s | ≤1.77 ms | ≤7.25 ms | ≤54,592 KiB | 49–50 |
| gpui-fast, threaded probe | preview, 122 ms | 4.56 s | 0.92 / 0.43 ms | 2.78 ms | 61,632 KiB | — |
| gpui-fast, sustained zoom | preview, 130–195 ms | 6.19–6.54 s | ≤1.04 ms | ≤6.07 ms | 56,080 KiB | 49–50 |

The sustained-zoom runs overlapped the workspace build. RSS rose between
their first two windows while zoom rasters were still warming.

The upstream profile (96 scripted frames on the initial threaded probe) moved exact routing from the test
thread to a worker. Worker and baseline shares use their respective thread
sample totals. The routed-edge percentage is inclusive.

| Profile and instruction metric | Release baseline | Follow-up |
| --- | ---: | ---: |
| Exact routed-edge construction | 93.54% of test thread | 95.21% of busiest worker |
| Edge crossings, self | 18.40% | 16.72% of worker |
| Spatial index query, self | 13.13% | 12.83% of worker |
| Segment/rectangle intersection, self | 11.88% | 11.66% of worker |
| Main-thread retired user instructions | 51.01 billion | 1.47 billion |
| Process retired user instructions | 51.01 billion | 51.73 billion |
| Upstream warm allocations over eight draws | 8 allocations, 448 bytes | 8 allocations, 448 bytes |
| gpui-fast warm allocations over eight draws | 0 allocations, 0 bytes | 0 allocations, 0 bytes |

Hardware instruction counts include startup, refinement, and 96 scripted
interactions. The threaded app can draw the preview while refinement runs,
so draw counts and scheduler overhead differ. The gpui-fast follow-up ran
0.276 billion instructions on the main thread and 50.53 billion process-wide. Process-wide work still
includes the unchanged exact routing. The earlier isolated rank test retired
2.758 billion → 0.222 billion user instructions over 25 runs after bucketing
edges by component.

The overview uses style-colored nodes and group boxes, batched edge strokes,
and first-line labels when readable. The exact renderer supplies full shape,
edge-decoration, and label fidelity. The styled/subgraph stage test asserts
one native publication, one rasterization for the final image, and
byte-identical staged and unstaged SVG. The zoom interaction test checks that
the old raster stays paintable through the settle delay and the refined SVG
is reused. A single background job owns the renderer and zoom changes
coalesce while it is busy. Replaced images are evicted from the sprite atlas.

Independent foreground-task instrumentation, native GPU interaction timing,
and 30-minute drift remain open. The release upstream platform still allocates
56 bytes per frame in the empty-view control. The 10K and
10M item sizes exceed Mermaid source admission. The 50-to-500-node warm
repaint test covers that widget at supported sizes.

## Cache bounds

| Owner | Bound |
| --- | --- |
| Renderer per viewer | 16 entries and 64 MiB of combined source, SVG and raster bytes. The parsed SVG tree is bounded by the 16-entry limit and is additional to the counted bytes. Each entry keeps one raster size, at most 4096 px per edge and 4 million pixels. |
| GPUI viewer | One cached, viewport-sized view per mounted Mermaid widget. Its refined scene paints one image plus the viewport and status. |
| Published viewer image | One current image, shared with the cache when it contains that raster. The sprite atlas retains the published image until replacement or release. A transient SVG overview with subpixel labels is at most 1024 px per edge and stays outside the cache. |
| Native preview | One transient scene per viewer, capped at 4,096 nodes, subgraphs and edge points combined. At most three labels per edge and one per node or subgraph. Released on refinement or supersession. |
| Rank component edges | One stage-local edge reference per internal edge and one bucket per component. Released after ranking. |
| Routing collision grid | Stage-local and proportional to the route or obstacle count. An object is referenced in up to 128 cells or stored once in the fallback list. |
| Rank transposition | One sorted list of neighbor positions per node in the current rank, plus one tie-break key per node. Released after each rank pass. |

Source admission limits each diagram to 64 KiB and 2,000 lines.
