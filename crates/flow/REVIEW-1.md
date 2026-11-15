# Flow review #1 hand-in

Implementation and validation revision: `6860754de5b06b4c72d4255f4dc972bc3f7fa541`.
This branch was rebased onto `release/0.1` at `8d0f992f37a22d271ae7d49f505ee3e3beefe681`
per the foreman's follow-up. The workspace now pins **gpui-kit =0.9.1**.
No merge or push was performed.

## Per-item status

| Review | Status | Implementation and regression evidence |
|---|---|---|
| 1 | Fixed | Toolbar deletion cancels interaction; toolbar pointer-down does not clear canvas selection; rendering filters missing drag nodes. Pure state and GPUI deleted-drag render/toolbar tests. |
| 2 | Fixed | Selected roots move descendants once in preview and commit, including pinned descendants; layout packs complete root subtrees and preserves offsets. Core, pure interaction and real GPUI parent-drag tests. |
| 3 | Fixed | Shared `Editor::z_key` orders selected subtrees above others and parents before descendants, with deterministic ID ties; hit tests and paint use it. Lower-ID child tests in core and GPUI. |
| 4 | Fixed | Editor owns node/edge selection sets outside graph/history. `Node::selected` remains an import hint and a renderer view-copy field. Undo maintains current selection and prunes removed IDs instead of resurrecting selection. |
| 5 | Fixed | Zero-motion previews keep off-grid origins; moved roots snap, descendants preserve offsets; preview and commit use identical arithmetic. Off-grid and no-op click tests. |
| 6 | Fixed | Arrange applies positions to all nodes, including non-draggable roots; grouped subtree bounds prevent root-group overlap. Pinned/parent layout regression. |
| 7 | Fixed | Interaction camera is private; validating setter rejects zero/negative/nonfinite zoom or pan and clamps positive finite zoom. Rejected updates preserve the camera; camera tests cover clamps. |
| 8 | Fixed | Empty node hits fall through to edge-stroke hits; modifier edge clicks toggle additive selection, selected edges are highlighted, Delete removes edges atomically. Pure and real event tests; public Flow selection getters. |
| 9 | Fixed | Selection never mutates or clones Scene; empty selection/deletion is cheap and creates no history. Shared Scene pointer regression includes empty and nonempty selection. |
| 10 | Fixed | Mesh cache is scoped to the document's edges rather than a FIFO/vertex working-set cap; unchanged route Arcs retain their meshes across edits. Two passes over 3,000 meshes produce zero misses on the second pass. |
| 11 | Fixed | Persistent graph maps share unchanged pages with bounded history; persistent BVHs copy/refit affected branches, and only changed/incident routes are regenerated. Bulk construction uses transient std indexes and packed BVH paths. Mixed move/delete/undo/redo tests compare fresh routes and indexes; independent snapshot and 1,500-entry insertion tests cover index mutations/rebalancing. |
| 12 | Fixed | Toolbar reads the selection set, node cards compare revision/selection counters instead of full Node data, and the visible-edge allocation transfers to paint and back through a reused buffer. Storage-pointer and unrelated-node render-count tests. |
| 13 | Fixed | Stable Scene identity preserves base minimap triangles; selection is a small overlay. GPUI test proves the minimap's geometry storage survives selection changes. |
| 14 | Documented option | Child positions remain **absolute world coordinates**, explicitly documented in both READMEs and `Node::rect`; moving a parent translates the whole subtree. This is the review's permitted documented alternative to parent-relative positions. |
| 15 | Fixed | Serialized edge high-water mark advances on successful connections, survives deletion/undo, and initializes from imported IDs. Undo + serde + reconnect regression proves automatic IDs are not reused. |
| 16 | Complete | All requested cases are present: deleted drag, parent drag, lower-ID child hit, selection-independent undo, off-grid drag, pinned/parent layout, >2048 meshes, real self-loop with allow_self, delete/undo fresh-scene equality, empty delete, extreme pinch and wheel clamp, empty minimap. |

## Red/green record

The regression commit `af344c3` precedes the fixes. Against the original implementation:

- The first core review run failed 7 tests: parent drag, child ordering, undo selection,
  pinned/group layout, Scene identity during selection, unchanged-route storage, and
  monotonic connection IDs; the already-correct empty-delete/undo consistency case passed.
- The first flow run failed 6 tests: deleted-drag rendering (panic at node indexing),
  parent preview, zero-motion off-grid preview, invalid camera, edge deletion through
  selection, and 3,000 misses on the second mesh pass.
- The edge-buffer transfer test was also executed against restored original sources
  (`4c9969c`) and failed because Frame cloned the edge allocation. Sources were restored
  immediately afterward. It passes with the implementation.
- Follow-up event tests exposed wheel exponent overflow and toolbar event handling;
  those tests passed after their corrections. Existing passing edge cases were retained.

Final checks all passed:

```sh
cargo test -p guiraffe-flow-core -p guiraffe-flow
cargo clippy -p guiraffe-flow-core -p guiraffe-flow --all-targets -- -D warnings
cargo fmt -p guiraffe-flow-core -p guiraffe-flow -- --check
git diff --check
```

**69 tests:** 35 flow-core integration tests and 34 flow tests, including 19 GPUI
TestAppContext tests. No ignored tests. Production source has no unwrap/expect;
workspace lints deny unsafe code. The render-count test uses normal `Window::draw`,
because gpui-kit's `render_frame` intentionally forces a refresh of every view.

## Validation limits and follow-up ownership

- The foreman explicitly deferred the FlowState/FlowCanvas element split, serializable
  props/IR and key-action registration to a separate dispatch after the new API conventions.
- Arbitrary user-supplied `Editor::edit` closures still require full structural validation.
  They share document pages and update only changed geometry; direct moves/deletes avoid
  full validation. Hierarchy changes rebuild the depth cache. Insertion chains trigger an
  occasional BVH rebalance at depth 48; large moves can degrade spatial grouping until rebuilt.
- Mesh storage now scales with graph routes instead of evicting a visible working set.
  GPUI element construction and submitted mesh transforms still allocate; zero-allocation
  frames are not claimed. Native pixels, real trackpad delivery and GPU presentation remain
  unmeasured. The 8 ms p99 design budget is not established on this shared host.
- See PERFORMANCE.md and review-1-benchmark.json for the alternating benchmark evidence,
  including construction overhead and per-run spread. These limits are not a claim of
  completion of the later API design or GPU/performance acceptance milestones.
