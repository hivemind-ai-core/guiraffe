# guiraffe-flow

A GPUI flow editor on `gpui-kit =0.9.1`, with a GPUI-independent document and
editing model in `guiraffe-flow-core`. Initialize `guiraffe_core::init(cx)` before
opening a window so Controls and the canvas share GUIraffe theme tokens.

```rust,ignore
let graph = Graph::from_parts(nodes, edges)?;
let state = cx.new(|cx| FlowState::new(graph, cx)
    .expect("validated graph")
    .options(FlowOptions::default())
    .node_renderer(MyRenderer));
// In the host view:
let canvas = FlowCanvas::new(&state);
```

`Flow` remains an alias for `FlowState`, including its lightweight Render host,
so existing `.child(flow_entity)` compositions continue to work. `FlowCanvas`
provides the state/element control shape. Camera and pointer feedback live in
`FlowViewport` and `FlowGesture` leaf entities; document/history/selection remain
in the model. Hosts that read the document do not depend on pan or gesture state.

The graph fits once after its first nonempty, measured layout by default. Set
`FlowOptions::fit_on_init = false` or explicitly set a camera before layout to
opt out; later graph edits and resizes preserve the user camera.

See `examples/editor.rs` for a complete runnable application:
`cargo run -p guiraffe-flow --example editor`.

The canvas mounts only visible nodes and edge labels. Node entities persist while visible; GPUI paint reuse applies only when absolute
bounds, text style and state match (a pan still changes their bounds). World-space routes and
scene-scoped tessellated meshes are cached separately from viewport translation.
Dragged nodes and incident edges stay mounted during previews; only release
creates a document history entry. `NodeRenderer` receives the node and zoom;
custom content should scale its internal typography and spacing to match the
scaled node frame. Theme changes are reflected on the next render.

Pointer gestures:

- Drag empty background to pan; wheel pans; Control/platform + wheel and pinch
  zoom about the pointer.
- Click a node to select; Shift/Control/platform + click toggles selection.
  Drag a selected node to move selected draggable roots and their descendants.
- Shift + drag on empty canvas replaces the selection; Control/platform + Shift
  + drag adds the touched nodes.
- Drag a source handle onto a target handle to connect. Structural and optional
  host semantic validators run before the edge is accepted.
- Click the minimap to recenter the camera. Controls offer zoom, fit, undo and
  redo; the selected node toolbar deletes the selection.

Keyboard (with the canvas focused): arrows pan, `+`/`-` zoom at center, `f` fits,
Click an edge to select it (modifier-click toggles additive selection).
Escape cancels a preview, Delete/Backspace deletes selected edges, nodes and descendants,
Control/platform + A selects all, Z undoes, Shift+Z/Y redoes.

Use `Flow::edit` for document changes so history and caches remain coherent.
Subscribe to `FlowEvent` on the state for accepted edits, selection changes and
rejected connections. Camera events are emitted by `state.viewport_entity()`;
subscribe there for `ViewportChanged` so panning never writes the document entity.
No-op input and document operations emit no redundant events or notifications.
`last_error` exposes the most recent rejected interaction.
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

History snapshots share unchanged document pages. Moves/deletes refit BVHs and
update affected routes; quality degradation triggers a balanced rebuild. Deleting
a child removes its depth entry without rebuilding unrelated depths. Arbitrary
`edit` closures still perform full structural validation (O(n)), and hierarchy
reparenting may update the affected subtree. Selection remains view state and is
not serialized. Base and selected minimap geometry are cached independently.

The UI creates GPUI elements and submits owned visible triangles each frame, so
it does not promise zero allocations. Edge transformations are batched by colour,
with an owned vertex buffer per batch instead of one allocation per edge.
Every visible node gets full content; fitting a graph so all 5,000 nodes are
visible needs separate performance evaluation. Parent ownership has no implicit
group frames or relative child transforms, parallel edges share their paths,
and keyboard focus does not yet navigate individual handles/edge labels.

`Interaction::viewport()` is read-only; use `set_viewport` to reject invalid cameras
and clamp finite positive zooms. Selection stays outside document/history and does
not invalidate minimap geometry. Parent and child coordinates are absolute world
coordinates; parent moves translate descendants and preserve their offsets.

Run both backends with `cargo test -p guiraffe-flow-core -p guiraffe-flow` and
`cargo test -p guiraffe-flow-core -p guiraffe-flow --features gpui-kit/gpui-fast`.
The local `gpui-fast` feature also forwards the backend feature. Render-isolation
tests use `gpui_kit::GPUI_FAST` to distinguish the selected backend; upstream GPUI
can redraw ancestors for a child change, whereas gpui-fast isolates these leaves.

## Migration note (review 2)
- `Flow` is now an alias of `FlowState` (the model). Camera changes live on a separate leaf entity, so
  **`FlowEvent::ViewportChanged` is emitted on `flow.viewport_entity()`, not on `Flow`**. Subscribe there
  for pan/zoom. This split is what keeps panning from re-rendering the host on gpui-fast.
- `Changed` / `SelectionChanged` are currently emitted on both the model and the viewport entity:
  subscribe to one of them (follow-up: emit on the model only).

### Paint styles

Colours are theme palette names (`"teal"`, `"orange"`: `guiraffe_core::ColorName`), shaded per
scheme to keep 3:1 against the canvas, so typed edges and ports stay distinct in both schemes.

- `Edge::color` colours an idle edge (the default is the dimmed edge colour); a selected edge is
  drawn in the accent whatever its colour. It combines with `Edge::stroke` (solid, dashed,
  dotted). Edges batch by colour: one vertex buffer per colour in use, none per edge.
- `Handle::color`, `Handle::ring` (`PortRing::Dot`, or `Ring`: hollow until connected or
  hovered, then filled) and `Handle::tooltip`. A ring keeps the dot's hit target and anchor.
  Connection state is shown for a node's first 64 ports.
- `Node::frame`: `NodeFrame::Rounded` (default), `Pill`, or `None`, for a renderer that draws the
  whole node; with `None` the renderer shows selection itself (`Node::selected`).
- `Graph::hide_markers` draws no edge markers, for graphs whose direction is plain from their
  ports.
- `FlowOptions::grid`: `GridStyle { color, step, dot }`; the step follows the snap grid by
  default.
- Cables: `Cable::color` (see `cables`).

### Selection by id

`FlowState::set_selection(nodes, edges, source, cx)` sets the selection of a mounted flow to
exactly these ids (unknown ids are ignored), for a host that keeps another view, such as a rack
or an inspector, in step. The current selection again is a no-op and emits nothing. Positions,
the viewport, keyboard focus and undo history are untouched. `FlowEvent::SelectionChanged`
carries the `Source`: `Pointer` or `Keyboard` for canvas input, the caller's for
`set_selection`, the dispatch's for the bus `select` action (`{nodes: [...], edges: [...]}`,
`FlowAction::Select`).

### Auto-layout

Call `FlowState::arrange(engine, options, duration, cx)` to compute geometry on
the background executor. Repeated requests supersede earlier ones. Use
`FlowState::cancel_layout(cx)` to cancel the active request explicitly.

For small graphs the canvas shows interpolated positions before committing
the final arrangement as one undoable edit. Pass a zero duration for an
immediate transition. The canvas also skips preview frames under reduced motion
or when the graph exceeds 150 nodes or 300 edges.
