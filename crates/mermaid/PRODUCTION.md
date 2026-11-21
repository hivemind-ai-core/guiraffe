# Mermaid production checks

The scale fixture is a 500-node flowchart with tree edges and periodic cross
links. The real-window mount probe is `cargo run --release -p
guiraffe-mermaid --example first_frame`. The workspace soak probe measures
panning, zooming, hovering and idle frames.

| Criterion | Check |
| --- | --- |
| First useful frame | The mount probe times a published preview. Large flowcharts do placement once, publish a temporary overview and run cancellable exact routing in the background. |
| Exact refinement | `vendor/mermaid-rs-renderer/parity/run.sh --soak` compares output to renderer 0.3.1. The staged fixture also checks node dimensions and relative placement through refinement. |
| Static repaint | The viewer paints one cached image regardless of node count. Frame allocation and scaling acceptance are pending measurement. |
| Memory | The native-window mount probe measures process peak RSS. Pass `-- --window-only` to measure the window and backend overhead. The Linux native window alone reached about 102 MiB and the full 500-node probe reached about 118 MiB. The 64 MiB process budget remains open in this environment. |

## Cache bounds

| Owner | Bound |
| --- | --- |
| Renderer per viewer | 16 entries and 64 MiB of combined source, SVG and raster bytes. The parsed SVG tree is bounded by the 16-entry limit and is additional to the counted bytes. Each entry keeps one raster size, at most 4096 px per edge and 4 million pixels. |
| Published viewer image | One current image, shared with the cache when it contains that raster. A transient overview with subpixel labels is at most 1024 px per edge and does not enter the cache. |
| Routing collision grid | Stage-local and proportional to the route or obstacle count. An object is referenced in up to 128 cells or stored once in the fallback list. |
| Rank transposition | One sorted list of neighbor positions per node in the current rank, plus one tie-break key per node. Released after each rank pass. |

Source admission limits each diagram to 64 KiB and 2,000 lines.
