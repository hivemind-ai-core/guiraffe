# guiraffe-flow-core

GPUI-free graph documents, atomic editing, bounded undo/redo, spatial queries and
cached edge geometry. Depends only on serde and serde_json.

`Graph::from_parts` validates IDs, finite positive node rectangles, named handles,
handle directions and acyclic parent chains. Deserialization is deliberately
separate from validation: pass a decoded `Graph` to `Editor::new` or `Scene::new`
before using it. Nodes carry arbitrary JSON data and an application-defined
`node_type`; default labels use string data or the node type.

`Editor::edit` validates and builds the replacement scene before publishing it.
An error changes neither document nor history; no-op edits preserve redo.
Selection does not enter history. A drag should preview positions and call
`move_nodes` once at release. Deleting a selected parent deletes descendants and
incident edges. History defaults to 100 document snapshots and can be disabled.
Nodes marked `draggable = false` retain their positions during moves and layout.

Parents express structural ownership. All positions are **absolute world
coordinates**; moving a parent does not implicitly move children. Hosts can move
a complete subtree in a single transaction. There are no implicit group frames.

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
Use `Scene::hit_node` and `hit_edge` for topmost picking.

Run `cargo run --release -p guiraffe-flow-core --example stress` for the
criterion-free 5,000-node/10,000-edge geometry benchmark. The companion UI crate
has a separate end-to-end headless CPU benchmark; neither measures GPU FPS.

The immutable BVH/query design and pointer-anchored camera were informed by
`gui/workbench/crates/graph-core` and `graph-ui`. This implementation does not
import their asynchronous pipeline, domain records or dependencies. The source
repository's KNOWN_ISSUES.md was reviewed, including crossing-edge culling,
selection/keyboard and group behavior limits.
