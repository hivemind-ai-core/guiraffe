# Warm repaint allocation audit

The upstream baseline is `c5d59be`. The probe mounts the soak window and a viewport-sized retained content view, then measures eight explicit CPU draws after warming both frame buffers. The fixtures use 1,000 items, except Mermaid, which uses 200 nodes and waits for the refined image. Renderer presentation and background allocations are outside the counting scope. [Probe instructions](../README.md#warm-repaint-allocation-probe)

| Component | Upstream retained baseline | Tab-stop reuse | Taffy scratch | Stable scene sort | Kit root retention | Explicit refresh baseline → after sort |
|---|---:|---:|---:|---:|---:|---:|
| Markdown | 89 | 80 | 63 | 62 | 4 | 1,455.125 → 1,349.125 |
| DataGrid | 85 | 80 | 63 | 62 | 4 | 1,341 → 1,083 |
| Gantt | 215 | 80 | 63 | 62 | 4 | 3,097 → 2,763 |
| Flow | 105 | 80 | 63 | 62 | 2 | 32,543 → 28,471 |
| Charts | 93 | 88 | 68 | 67 | 2 | 538 → 502 |
| Mermaid | 77 | 77 | 60 | 60 | 2 | 134 → 109 |

Counts are allocation and reallocation requests per draw. The explicit refresh column includes full window invalidation. Commits `aff57b4`, `6ec3928`, `3185793`, and `b21fb2f` apply the measured fixes.

The baseline Gantt trace attributed 135 of 215 requests to tab-stop SumTree construction. The remaining upstream traces include root-shell style and listener construction, plus Taffy child lists and result storage. The Flow stable sort requested 1,125,920 bytes of temporary storage in each measured retained frame before its fix.

## CPU and memory samples

Grid baseline samply self samples were 59.11% in `BoundsTree::insert`, 2.36% in `DispatchTree::push_node`, and 2.06% in `Scene::replay` across 6,616 filtered samples. Flow attributed 75.23% to `BoundsTree::insert` after Taffy reuse and 74.12% after stable scene sorting, from 5,677 and 6,565 filtered samples respectively.

| Workload and change | Callgrind instructions | Peak process RSS |
|---|---:|---:|
| Grid, Taffy scratch | 197,193,238 → 196,536,904 | — |
| Flow, scene sort | 2,298,459,849 → 2,293,276,840 | 119,572 → 117,728 KiB |

Callgrind collects only `warm_repaint::draw`, including its eight warm-up draws. Grid records 108 draws and Flow records 16. RSS includes process start and mounting. These samples were taken on a shared host.

The instrumented 160-frame Flow runs averaged 17.5 and 22.9 ms per draw
before and after scene-sort reuse on the shared profiling host. Dedicated
quiet-window interaction timing remains separate from these samples. The
[production inventory](../../PRODUCTION.md#shared-gpui-repaint-storage)
describes the retained buffer bounds.


## Allocation sites after retaining the roots

The upstream Kit root replay removed shell layout, style refinement and
handler construction. Its remaining requests were 48-byte copies of
`KeyContext` entries in retained dispatch nodes. GDB confirmed the same
owner in the fast backend at `fast::dispatch::copy_nodes`.

Before the final fast-backend changes, Markdown used 91 requests and
410,760 bytes per retained draw. DataGrid and Gantt used two 48-byte
requests each. Flow, Charts and Mermaid used one each. Markdown had an
additional rounded-clip scene compilation on every replay.

| Component | Fast requests before → after | Fast requested bytes before → after |
|---|---:|---:|
| Markdown | 91 → 0 | 410,760 → 0 |
| DataGrid | 2 → 0 | 96 → 0 |
| Gantt | 2 → 0 | 96 → 0 |
| Flow | 1 → 0 | 48 → 0 |
| Charts | 1 → 0 | 48 → 0 |
| Mermaid | 1 → 0 | 48 → 0 |

The fast release result uses the six allocator tests after integrating
`release/0.2` at `8d5ff4c` and applying `6caa08e`, `b3fc5f0`, and
`e47017c`. Each test counted eight retained frames and required zero
requests on each frame. The same run passed the integrated Markdown and
Chat memory and repaint-scaling checks, as well as the cached-path and Kit
root notification/refresh tests.
