# Foreman review #1 of flow-core + flow — must-fix before merge to release/0.1

Fix ALL of 1–12 and 15, add the tests in 16. For 14, switch child positions to parent-relative (react-flow semantics) with helpers for absolute positions, OR document clearly — prefer relative.

1. PANIC: toolbar "Delete selected" (flow/src/lib.rs ~479) doesn't cancel the active gesture; next render indexes graph().nodes[id] for deleted drag nodes (~279-294). Cancel interaction; also make render tolerate gestures referencing missing nodes.
2. Dragging a parent leaves children behind (interaction.rs ~627, editor.rs move_nodes ~504). Move descendants too (preview + commit). LayeredLayout also ignores parent/child.
3. hit_node returns highest id; paint order is id order → child under parent. Introduce z-order (parents before children, selected on top) shared by hit-test & paint.
4. Selection lives in Graph → undo/redo restores old selection, snapshots carry it. Move selection out of history (Editor/Interaction-owned set).
5. Drag snap preview vs commit consistency is fragile; add test for off-grid origin.
6. arrange() skips non-draggable nodes → overlaps. Layout should move all or treat pinned nodes as fixed obstacles.
7. Interaction.viewport is pub with no validation; zoom 0/NaN bricks camera. Make private + validating setter; clamp.
8. Edges can't be selected/deleted from UI (hit_edge unused). Add edge selection + delete.
9. Each selection click Arc::make_mut-clones whole Scene (paint frame + minimap hold Arcs). Fix via #4; skip clearing empty selection.
10. Edge mesh cache FIFO cap 2048 thrashes at >2048 visible edges (0% hit). Real LRU / larger cap / cache with scene; test: second frame of >2048 visible edges has 0 misses.
11. Every edit clones/validates/rebuilds everything + full-graph undo snapshots ×100. Incremental rebuild of incident edges & index entries; diff-based or structurally shared history.
12. Per-frame O(n): toolbar selected scan (lib.rs ~471), full Node compare/clone per visible node (~304-313), visible edge list clone (paint.rs ~961). Cache/version counters/reuse buffers.
13. Minimap rebuild on every selection change (goes away with #9).
15. Edge ids reused (max+1). Use monotonic counter (persisted in serde).
16. Tests for: delete during drag; parent drag; child/parent hit when child id lower; undo doesn't restore selection; off-grid drag; arrange with pinned & parent nodes; mesh cache >2048; self-loop via UI with allow_self; delete node with edges → undo → scene equals Scene::new (paths + index); delete_selected with nothing selected creates no history; pinch delta <= -1 and wheel zoom clamping; minimap click on empty graph.

Re-run the stress benchmarks after and update PERFORMANCE.md with before/after.
