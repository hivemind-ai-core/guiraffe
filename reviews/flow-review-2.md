# Flow review #2 (verification): MERGED with follow-ups

13 of 16 fixed, #14 documented, #12 and #16 partial. Follow-ups, assigned with the FlowState/FlowCanvas split:
1. Notify/event noise (§6.2): mouse-down always notifies and emits SelectionChanged, even when it starts a pan (lib.rs ~459). Mouse-up always emits Selection+Viewport. Move notifies below the 3 px threshold. changed() and Ctrl+A notify without a change.
2. §6.1: the viewport and gestures live in the single Flow view, so a pan re-runs the visible-node loop. Split FlowState (model), viewport and gesture leaf entities, and FlowCanvas.
3. Per-frame work: theme clone and per-node compare, gesture map clone during drags (paint.rs ~151), z-sort with a parent-chain walk every frame, per-edge mesh transform allocation.
4. Panicking indexing in library code (editor.rs 317/332, scene.rs 463, layout.rs 22..114, interaction.rs 701, lib.rs 376/380, paint.rs 190). The public Layout::positions panics on a missing parent and LOOPS FOREVER on a parent cycle. Validate first.
5. Deleting a child node rebuilds all depths (O(n)). Add an edit benchmark for it.
6. The BVH degrades under repeated moves (refit without rebalance). Add a rebuild trigger and a degradation test.
7. Scene::hit_node vs Editor::hit_node disagree. Make it private or document it.
8. A selection-only edit creates an empty undo step.
9. Selection isn't saved (documented). 10. edit() does full validation, O(n) (documented). 11. Box select is always additive (support replace and add with modifier).
Weak tests to strengthen: arrange overlap, a real second-frame mesh-cache test, toolbar test via real events.
