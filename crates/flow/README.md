# guiraffe-flow

A GPUI flow editor on `gpui-kit =0.7.0`, with a GPUI-independent document and
editing model in `guiraffe-flow-core`. Initialize `guiraffe_core::init(cx)` before
opening a window so Controls and the canvas share GUIraffe theme tokens.

```rust,ignore
let graph = Graph::from_parts(nodes, edges)?;
let flow = cx.new(|cx| Flow::new(graph, cx)
    .expect("validated graph")
    .options(FlowOptions::default())
    .node_renderer(MyRenderer));
```

See `examples/editor.rs` for a complete runnable application:
`cargo run -p guiraffe-flow --example editor`.

The canvas mounts only visible nodes and edge labels. Node entities persist while visible; GPUI paint reuse applies only when absolute
bounds, text style and state match (a pan still changes their bounds). World-space routes and
bounded tessellated meshes are cached separately from viewport translation.
Dragged nodes and incident edges stay mounted during previews; only release
creates a document history entry. `NodeRenderer` receives the node and zoom;
custom content should scale its internal typography and spacing to match the
scaled node frame. Theme changes are reflected on the next render.

Pointer gestures:

- Drag empty background to pan; wheel pans; Control/platform + wheel and pinch
  zoom about the pointer.
- Click a node to select; Shift/Control/platform + click toggles selection.
  Drag a selected node to move all selected draggable nodes.
- Shift + drag on empty canvas adds the touched nodes to the selection.
- Drag a source handle onto a target handle to connect. Structural and optional
  host semantic validators run before the edge is accepted.
- Click the minimap to recenter the camera. Controls offer zoom, fit, undo and
  redo; the selected node toolbar deletes the selection.

Keyboard (with the canvas focused): arrows pan, `+`/`-` zoom at center, `f` fits,
Escape cancels a preview, Delete/Backspace deletes selected nodes and descendants,
Control/platform + A selects all, Z undoes, Shift+Z/Y redoes.

Use `Flow::edit` for document changes so history and caches remain coherent.
Subscribe to `FlowEvent` for accepted edits, selection/camera changes and rejected
connections; `last_error` also exposes the most recent rejected interaction.
`FlowOptions` controls snapping, background (dots/lines/cross/none), minimap,
controls and the selected-node toolbar. Bezier, smooth-step, step and straight
edges support labels, start/end arrow markers and optional animated beads.

Validation uses real GPUI TestAppContext pointer/key/wheel dispatch, plus pure
interaction tests and core tests. Headless tests verify state/layout, not pixels,
OS trackpad delivery or GPU rendering. A desktop visual/refresh-rate run remains
necessary before claiming 60fps. No published 60fps claim is made here.

Run `cargo run -p guiraffe-flow --example stress` for a headless GPUI CPU benchmark
with 5,000 nodes and 10,000 edges, including culling, node rendering, edge painting
and cached grid/minimap work. It reports 200 measured pan frames after 20 warm-up frames.
The workspace dev profile optimizes dependencies at level 2 and this crate at
level 0; record the profile with any result. For an optimized local-crate comparison
without rebuilding dependency optimization settings, run
`CARGO_PROFILE_DEV_OPT_LEVEL=2 cargo run -p guiraffe-flow --example stress`.
Use `-- --no-minimap` for an explicit diagnostic comparison. `flow-core` offers an independent
release geometry benchmark.

Current limits: document edits rebuild spatial indexes and route geometry;
undo stores bounded full-document snapshots. The UI creates GPUI elements and
copies visible triangles each frame, so it does not promise zero allocations.
Every visible node gets full content; fitting a graph so all 5,000 nodes are
visible needs separate performance evaluation. Parent ownership has no implicit
group frames or relative child transforms, parallel edges share their paths,
and keyboard focus does not yet navigate individual handles/edge labels.
