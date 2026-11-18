# guiraffe-flow-core

GPUI-free graph documents, atomic editing, bounded undo/redo, spatial queries and
cached edge geometry. Uses serde/serde_json and structurally shared `im` maps.

`Graph::from_parts` validates IDs, finite positive node rectangles, named handles,
handle directions and acyclic parent chains. Deserialization is deliberately
separate from validation: pass a decoded `Graph` to `Editor::new` or `Scene::new`
before using it. Nodes carry arbitrary JSON data and an application-defined
`node_type`; default labels use string data or the node type.

`Editor::edit` validates and builds the replacement scene before publishing it.
An error changes neither document nor history; no-op edits preserve redo.
Selection does not enter history. A drag should preview positions and call
`move_nodes` once at release. Deleting a selected parent deletes descendants and
incident edges. History defaults to 100 structurally shared document snapshots and can be disabled.
Nodes marked `draggable = false` stay put during direct dragging. Moving an ancestor
translates its entire subtree, including pinned descendants; layout places every node.

Parents express structural ownership. All positions are **absolute world
coordinates**, including child positions (unlike React Flow parent-relative positions).
Moving a parent translates all descendants by the same delta in preview and commit.
Only the moved root snaps; descendant offsets remain exact. Selecting both a parent
and child does not apply the movement twice. There are no implicit group frames.
Layered layout treats each root subtree as one bounding box and preserves offsets.

Connection validation checks source/target handle kinds, optional self/parallel
and cycle policies, then the application's `ConnectionValidator` hook.
`LayeredLayout` is a deterministic topological layout with size-aware layer
spacing; cycles go in a final fallback layer. Implement `Layout` for another
layout engine. This is not a dagre crossing-minimization implementation.

`Scene` caches flattened Bezier, rounded smooth-step, step and straight paths,
arc-length midpoint labels, incident edge lists and independent node/edge BVHs.
Edge indexing includes the complete cached stroke, so a crossing edge remains
visible when both endpoints are outside the viewport. `SpatialIndex::query` and
`Scene::visible` reuse caller-owned vectors and return deterministic ID order.
`Scene::hit_node` orders parents before descendants, then by ID. Use
`Editor::hit_node` and `Editor::z_key` for selection-aware picking and paint order.
Selection is owned by `Editor::selected_nodes/selected_edges`, outside the document
and its history. `Node::selected` is an import hint consumed by `Editor::new`; node
renderers receive the live flag in their view copy. Undo does not resurrect deleted
selection. New connections use the serialized `Graph::edge_id_high_water`, which
is never rewound by undo or deletion.

Graph maps are persistent ordered maps with the same JSON map representation.
History snapshots share unchanged pages. Moves/deletions update affected routes
and refit BVH branches; arbitrary `edit` closures still run structural validation.
New index entries split only their destination leaf; large movements can degrade
spatial grouping over time, so a host may rebuild a scene for rebalancing.

Run `cargo run --release -p guiraffe-flow-core --example stress` for the
criterion-free 5,000-node/10,000-edge geometry benchmark. The companion UI crate
has a separate end-to-end headless CPU benchmark; neither measures GPU FPS.

The immutable BVH/query design and pointer-anchored camera were informed by
`gui/workbench/crates/graph-core` and `graph-ui`. This implementation does not
import their asynchronous pipeline, domain records or dependencies. The source
repository's KNOWN_ISSUES.md was reviewed, including crossing-edge culling,
selection/keyboard and group behavior limits.

Review #2: `LayeredLayout::positions` validates its public input first and returns
an empty map for invalid documents (including missing parents and parent cycles).
`Scene::hit_node` is structural picking; it cannot see selection. Use
`Editor::hit_node` for the live selected-subtree z-order used by the canvas.
Depth-cache updates preserve unrelated entries on child deletion; reparenting
updates affected descendants. BVH refits track a normalized perimeter cost and
rebalance when it exceeds 1.8 times the build quality (plus the insertion-depth
safeguard). Run `cargo run --release -p guiraffe-flow-core --example delete_child_stress`
for child deletion/undo timings on 5,000 nodes and 10,000 edges.

### Auto-layout

Use `LayoutEngine::layout` with an `AutoLayout` engine and `LayoutOptions`.
Layered layout supports four directions, Tree centers parents over a spanning
forest, Radial places ranks on concentric rings, Grid packs rows, and Force
uses seeded initial positions and local repulsion. The built-in engines
return world positions and orthogonal paths anchored to the node handles.

Pinned nodes freeze their entire root group. Group members retain their
relative offsets. Relative constraints refer to the named nodes' top-left
coordinates. The solver rejects contradictory or unplaceable requests with
`LayoutError::Infeasible`. Collision repair is heuristic, so a rejected
request may still have a solution. Routes use port sides and endpoint bounds
and can cross unrelated nodes.

For incremental layout, set `affected` to the node IDs in the edited region.
The solver holds the remaining groups fixed. Cancel a running invocation with
a clone of its `LayoutCancellation`. `Editor::apply_layout` applies positions
and route styles together so undo restores both.
