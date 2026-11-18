# Flow, Timeline and Charts production pass

Branch `feat-prod-b`, based on `release/0.2` at `62b8de3`.
The component tables contain each spec/23 criterion, workload evidence,
measured release numbers, and remaining checks.

| Component | Audit and measurements | Changes |
| --- | --- | --- |
| Flow and Flow Core | [Flow production table](../../crates/flow/PRODUCTION.md) | Scale build and pan/hit, reference spatial-index property, allocation and node-payload checks, bounded dense minimap geometry (`7312e37`). |
| Timeline | [Timeline production table](../../crates/timeline/PRODUCTION.md) | Scale box selection, clip navigation, and bar edits, reference model properties, clip-end admission (`ad3d0ad`), stale-row and key-range selection (`65d401a`). |
| Charts | [Charts production table](../../crates/charts/PRODUCTION.md) | Scale line preparation, full and zoomed views, and dense scatter, prepared memory and recompute traffic, stream snapshot property, valid dense bubble representation (`55fec25`). |

The initial audit is `5aba0a6`. Shared workload and soak changes include
`ea8f04d`, `bd02de2` and `5dbc371`. Reference properties and storage
gates are in `4f88662`, `fcc2268` and `3cffcfa`.

The red/green checks reproduced invalid clip insertion, a missing dense
bubble, unbounded minimap vertices, and a stale dope-sheet row panic.
The 100k-bar and warm core Flow and Timeline paths have structural allocator
checks. Live GPUI repaint allocation, app-level cancellation, native replay,
scale accessibility and snapshots, streaming throughput, and the full soak
remain open. The shared headless harness measures script cycles. A cycle can
dispatch multiple events, and the harness excludes GPU presentation.
