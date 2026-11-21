# GPUI rendering storage bounds

| Storage | Bound and lifetime |
| --- | --- |
| Tessellated path vertices | One owned buffer per distinct tessellation or changed clone. Reference counts release buffers when the existing scene, path or layer owners release them. The patch adds no vertex cache. |
| Existing fast tessellation cache | Up to 2,048 entries and 16 MiB of keys and triangle capacity per thread, plus one oversize shape. The Arc adds one header to each independently owned buffer. |
| Recent element IDs in `gpui-pre` | At most 64 paths per window. Each entry retains one observed stack of at most 32 element IDs. An entry is replaced on a collision and all entries drop with the window. |
| Default font features in `gpui-pre` | One empty vector for the process. |
| Dispatch replay stack in `gpui-pre` | 32 entries inline. Deeper nesting can spill for that replay and frees the buffer when replay returns. |
| Arena validity token | One token owned by the arena. Escaped boxes keep their own invalidated token until dropped, as before. |

The `cached_paths` benchmark integration test counts allocations during CPU
draw and arena clear after warming both frames. It verifies that the cached
canvas stays unpainted and all 1,000 paths remain in the rendered scene.
GPU submission, deep dispatch trees, new tessellations and focusable tab-stop
replay are outside this test.

## Warm-frame measurements

| Fixture and backend | Allocations / requested bytes per draw | Callgrind instructions per draw | Largest measured self shares | Peak process RSS | Loaded mean per draw |
| --- | ---: | ---: | --- | ---: | ---: |
| Unpatched cached chart, `gpui-pre 0.3.12` | 263 / 498,444 | 940,842 | Callgrind: bounds tree 40.40%, vertex clone 16.71% | — | — |
| Unpatched cached chart, `gpui-fast 0.1.15` | 373 / 698,712 | 602,904 | Callgrind: vertex clone 39.11%, malloc 10.24% | — | — |
| Patched 1,000-path canvas, `gpui-pre` | 0 / 0 | 4,945,506 | samply: bounds tree 67.91%, scene replay 12.71% | 14,624 KiB | 0.94 ms |
| Patched 1,000-path canvas, `gpui-fast` | 0 / 0 | 504,888 | samply: unnamed symbol 30.34%, primitive insert 25.84%, scene replay 14.83% | 14,180 KiB | 0.27 ms |

The unpatched chart and patched canvas are different fixtures. DHAT stacks
within the chart's CountingAllocator scope identified 124 vertex allocations
per copy site, for a total of 248 on upstream and 372 on fast. The synthetic
canvas sustained 4,000 full draws on each backend with zero counted
allocations. The fast synthetic numbers use a standalone build of the same
fixture against the Cargo-built GPUI artifact. The loaded means subtract a
one-frame process run from the 4,000-frame run. Quiet-window latency
percentiles and interaction-specific budgets require the component soaks.

## Matched component draws

The component sources were pinned at worker merge `b770a06`, which includes
release base `5978237`. The unpatched runs override only the two GPUI Cargo
patch paths with the published crate sources. Each cell shows allocator calls
and requested bytes per warmed draw.

| Component fixture | Upstream before | Upstream after | Fast before | Fast after |
| --- | ---: | ---: | ---: | ---: |
| Charts, layered scatter with 10,000 points and 100 series | 74 / 79,500 B | 6 / 57,680 B | 59 / 11,160 B | 0 / 0 B |
| Flow, 256 nodes and 248 edges, minimap disabled | 44 / 5,403,204 B | 26 / 336,880 B | 7 / 5,062,680 B | 0 / 0 B |
| Gantt, 1,000 tasks in dependency chains | 515 / 454,348 B | 150 / 235,688 B | 569 / 270,984 B | 43 / 528 B |
| Mermaid, same counts at 50 and 500 nodes | 9 / 4,088 B | 1 / 56 B | 1 / 24 B | 0 / 0 B |

Charts checks that its cached ink and scene build counters stay fixed. Flow
and Gantt use a sized cached host whose render count stays fixed. Mermaid
checks that the refined image remains shared and reports the per-draw
average over eight draws. These probes exercise ordinary warm draws.

DHAT attributes the remaining upstream Flow traffic to quad sort scratch
and tab stops. Upstream Gantt also allocates for tab stops, quad sort scratch,
debug bounds and key context. Fast Gantt has 42 debug-bounds allocations
and one key-context clone in its test-support build. The measurements cover
steady cached draws and leave cold layout and interaction performance to
the component soaks.

`scripts/gate.sh` passed at `a5b429a`, with 1,272 workspace tests on each
backend, per-crate doctests, strict workspace Clippy and formatting. At
`b770a06`, both backends passed the two cached-path tests and 25 allocator
tests. The tracked golden images match their release base at both revisions.
The local measurement logs and the Flow and Gantt probe source are in
`target/gpui-profiles/`.
