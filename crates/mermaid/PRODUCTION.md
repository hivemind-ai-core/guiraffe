# Mermaid production checks

The scale fixture is a 500-node flowchart with tree edges and periodic cross
links. The real-window mount probe is `cargo run --release -p
guiraffe-mermaid --example first_frame`. Add `--features gpui-kit/gpui-fast` before any example arguments to select
gpui-fast. The workspace soak probe measures panning, zooming, hovering and
idle frames.

| Criterion | Check |
| --- | --- |
| First useful frame | The native mount probe times the first render with a published preview. Large flowcharts do placement once. Plain subpixel flowcharts use GPUI primitives and keep the full SVG path for exact refinement. The worker waits for preview paint, with a 100 ms fallback for hidden views. |
| Exact refinement | `vendor/mermaid-rs-renderer/parity/run.sh --soak` compares output to renderer 0.3.1. The staged fixture also checks node dimensions and relative placement through refinement. |
| Static repaint | The allocator regression in `bench/tests/allocator.rs` compares eight warmed GPUI draws at 50 and 500 nodes. Upstream GPUI dropped from 63 to 9 allocations per draw at each size. The zero-allocation criterion remains open. |
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

The native preview builds bounded geometry off-thread and paints up to two
edge paths and one quad per visible preview node. Exact routing starts after
the paint acknowledgment or the fallback timeout. The native preview test
checks zero rasterizations for a plain multi-segment overview and one for
a shape that requires the SVG fallback. The isolated 500-node rank probe
took 2.758 billion → 0.222 billion retired user instructions over 25 runs
after bucketing edges by component. The profile attributed one sample
each to text measurement and preview routing before the changes. Reusing
the existing placement across stages avoids a second text measurement pass.

## Cache bounds

| Owner | Bound |
| --- | --- |
| Renderer per viewer | 16 entries and 64 MiB of combined source, SVG and raster bytes. The parsed SVG tree is bounded by the 16-entry limit and is additional to the counted bytes. Each entry keeps one raster size, at most 4096 px per edge and 4 million pixels. |
| GPUI viewer | One cached, viewport-sized view per mounted Mermaid widget. Its refined scene paints one image plus the viewport and status. |
| Published viewer image | One current image, shared with the cache when it contains that raster. A transient SVG overview with subpixel labels is at most 1024 px per edge and stays outside the cache. |
| Native preview | One transient scene per viewer, capped at 2,048 nodes and edge segments combined. Released on refinement or supersession. |
| Rank component edges | One stage-local edge reference per internal edge and one bucket per component. Released after ranking. |
| Routing collision grid | Stage-local and proportional to the route or obstacle count. An object is referenced in up to 128 cells or stored once in the fallback list. |
| Rank transposition | One sorted list of neighbor positions per node in the current rank, plus one tie-break key per node. Released after each rank pass. |

Source admission limits each diagram to 64 KiB and 2,000 lines.
