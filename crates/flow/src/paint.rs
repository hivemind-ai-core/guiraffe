//! Cached world-space tessellation with bounded storage and indexed culling.
use crate::{Background, Gesture, Interaction};
use gpui_kit::{Bounds, Path, PathBuilder, Pixels, Window, fill, point, px};
use guiraffe_core::{GuiraffeTheme, Rgb, Surfaces};
use guiraffe_flow_core::{
    EdgeId, EdgePath, Marker, Node, Point as WorldPoint, Rect, Scene, Viewport, Visible,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const EDGE_STROKE: f32 = 1.5;

#[derive(Default)]
pub(crate) struct MeshCache {
    entries: BTreeMap<EdgeId, (Arc<EdgePath>, Path<Pixels>)>,
    pub edge_buffer: Vec<EdgeId>,
    vertices: usize,
    pub misses: usize,
    pub timings: (u128, u128, u128),
    batch_capacity: (usize, usize),
    #[cfg(test)]
    pub batches: usize,
    minimap: Option<MinimapMeshes>,
    grid: Option<GridMesh>,
}
impl MeshCache {
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.entries.clear();

        self.vertices = 0;
        self.minimap = None;
    }
    pub fn sync(&mut self, scene: &Scene) {
        self.entries.retain(|id, (route, _)| {
            scene
                .paths()
                .get(id)
                .is_some_and(|next| Arc::ptr_eq(route, next))
        });
        self.vertices = self.entries.values().map(|(_, p)| p.vertices.len()).sum();
    }
    // Scene-scoped: every graph edge can stay resident, with no FIFO working-set cliff.
    fn get(&mut self, id: EdgeId, route: &Arc<EdgePath>) -> Option<&Path<Pixels>> {
        if self
            .entries
            .get(&id)
            .is_none_or(|(old, _)| !Arc::ptr_eq(old, route))
        {
            let path = mesh(route)?;
            if let Some((_, old)) = self.entries.remove(&id) {
                self.vertices -= old.vertices.len();
            }
            self.vertices += path.vertices.len();
            self.misses += 1;
            self.entries.insert(id, (route.clone(), path));
        }
        self.entries.get(&id).map(|(_, p)| p)
    }
}
struct GridMesh {
    size: WorldPoint,
    step: f32,
    background: Background,
    path: Path<Pixels>,
}
fn rectangle(path: &mut Path<Pixels>, rect: Rect) {
    let a = pixel(rect.origin);
    let b = pixel(WorldPoint::new(rect.max().x, rect.origin.y));
    let c = pixel(rect.max());
    let d = pixel(WorldPoint::new(rect.origin.x, rect.max().y));
    let uv = (point(0., 1.), point(0., 1.), point(0., 1.));
    path.push_triangle((a, b, c), uv);
    path.push_triangle((a, c, d), uv);
}
struct MinimapMeshes {
    scene: Arc<Scene>,
    size: WorldPoint,
    normal: Path<Pixels>,
    selection: Option<(Arc<BTreeSet<u64>>, Path<Pixels>)>,
}
/// Rasterize dense minimaps in four-pixel cells so the submitted triangle count
/// depends on the minimap dimensions even when all graph nodes are selected.
fn minimap_mesh<'a>(
    map: Rect,
    camera: Viewport,
    count: usize,
    nodes: impl Iterator<Item = &'a Node>,
) -> Path<Pixels> {
    const CELL: f32 = 4.;
    const COLUMNS: usize = 45;
    const ROWS: usize = 30;
    let mut path = Path::new(pixel(map.origin));
    let rects = nodes.map(|node| {
        let size = node.size * camera.zoom;
        Rect {
            origin: camera.screen(node.pos),
            size: WorldPoint::new(size.x.max(1.), size.y.max(1.)),
        }
    });
    if count <= COLUMNS * ROWS {
        for rect in rects {
            rectangle(&mut path, rect);
        }
        return path;
    }

    let mut occupied = [[false; COLUMNS]; ROWS];
    for rect in rects {
        let min = (rect.origin - map.origin) * (1. / CELL);
        let max = (rect.max() - map.origin) * (1. / CELL);
        let left = (min.x as usize).min(COLUMNS);
        let right = (max.x.ceil() as usize).min(COLUMNS);
        let top = (min.y as usize).min(ROWS);
        let bottom = (max.y.ceil() as usize).min(ROWS);
        for row in &mut occupied[top..bottom] {
            row[left..right].fill(true);
        }
    }
    for (y, row) in occupied.iter().enumerate() {
        for (x, &filled) in row.iter().enumerate() {
            if filled {
                rectangle(
                    &mut path,
                    Rect::new(
                        map.origin.x + x as f32 * CELL,
                        map.origin.y + y as f32 * CELL,
                        CELL,
                        CELL,
                    ),
                );
            }
        }
    }
    path
}

fn mesh(route: &EdgePath) -> Option<Path<Pixels>> {
    let mut builder = PathBuilder::stroke(px(EDGE_STROKE));
    for (i, p) in route.points.iter().enumerate() {
        if i == 0 {
            builder.move_to(pixel(*p));
        } else {
            builder.line_to(pixel(*p));
        }
    }
    builder.build().ok()
}
fn pixel(p: WorldPoint) -> gpui_kit::Point<Pixels> {
    point(px(p.x), px(p.y))
}
fn transformed_mesh(
    path: &Path<Pixels>,
    viewport: Viewport,
    origin: gpui_kit::Point<Pixels>,
) -> Path<Pixels> {
    // GPUI owns the submitted triangles. Transform copies, preserving cached world vertices.
    let mut screen = Path::new(origin + pixel(viewport.pan));
    screen.bounds = Bounds::new(
        path.bounds.origin * viewport.zoom + origin + pixel(viewport.pan) - point(px(1.), px(1.)),
        gpui_kit::size(
            path.bounds.size.width * viewport.zoom + px(2.),
            path.bounds.size.height * viewport.zoom + px(2.),
        ),
    );
    screen
        .vertices
        .extend(path.vertices.iter().cloned().map(|mut vertex| {
            vertex.xy_position = vertex.xy_position * viewport.zoom + origin + pixel(viewport.pan);
            vertex
        }));
    screen
}
fn append_transformed(
    output: &mut Path<Pixels>,
    path: &Path<Pixels>,
    viewport: Viewport,
    origin: gpui_kit::Point<Pixels>,
) {
    let bounds = Bounds::new(
        path.bounds.origin * viewport.zoom + origin + pixel(viewport.pan) - point(px(1.), px(1.)),
        gpui_kit::size(
            path.bounds.size.width * viewport.zoom + px(2.),
            path.bounds.size.height * viewport.zoom + px(2.),
        ),
    );
    output.bounds = if output.vertices.is_empty() {
        bounds
    } else {
        output.bounds.union(&bounds)
    };
    output
        .vertices
        .extend(path.vertices.iter().cloned().map(|mut vertex| {
            vertex.xy_position = vertex.xy_position * viewport.zoom + origin + pixel(viewport.pan);
            vertex
        }));
}
fn paint_mesh(
    path: &Path<Pixels>,
    viewport: Viewport,
    origin: gpui_kit::Point<Pixels>,
    window: &mut Window,
    color: Rgb,
) {
    window.paint_path(transformed_mesh(path, viewport, origin), color.to_hsla());
}
pub(crate) struct Frame {
    scene: Arc<Scene>,
    viewport: Viewport,
    edges: Vec<EdgeId>,
    background: Background,
    minimap: bool,
    gesture: Option<Gesture>,
    grid: Option<f32>,
    surfaces: Surfaces,
    accent: Rgb,
    selected: Arc<BTreeSet<u64>>,
    selected_edges: Arc<BTreeSet<u64>>,
    /// Animated-edge phase in [0, 1); `None` disables edge animation (reduced motion).
    phase: Option<f32>,
}
impl Frame {
    pub fn new(
        i: &Interaction,
        visible: &mut Visible,
        background: Background,
        minimap: bool,
        theme: &GuiraffeTheme,
        phase: Option<f32>,
    ) -> Self {
        Self {
            scene: i.editor.scene_shared(),
            viewport: i.viewport(),
            edges: std::mem::take(&mut visible.edges),
            background,
            minimap,
            gesture: i.gesture.clone(),
            grid: i.grid,
            surfaces: theme.surfaces(),
            accent: theme.primary_color(),
            selected: i.editor.selected_nodes_shared(),
            selected_edges: i.editor.selected_edges_shared(),
            phase,
        }
    }
    fn preview(&self, id: u64) -> Option<WorldPoint> {
        self.gesture
            .as_ref()
            .and_then(|g| g.preview(id, self.scene.graph(), self.grid))
    }
    pub fn paint(&mut self, bounds: Bounds<Pixels>, cache: &mut MeshCache, window: &mut Window) {
        cache.misses = 0;
        let started = std::time::Instant::now();
        self.grid(bounds, cache, window);
        let grid_us = started.elapsed().as_micros();
        let edge_started = std::time::Instant::now();
        let graph = self.scene.graph();
        // GPUI takes ownership of submitted vertices. Batch by colour, so its
        // ownership boundary requires one allocation per batch, not per edge.
        let mut normal = Path::new(bounds.origin);
        let mut selected = Path::new(bounds.origin);
        normal.vertices.reserve_exact(cache.batch_capacity.0);
        selected.vertices.reserve_exact(cache.batch_capacity.1);
        for id in &self.edges {
            let Some(edge) = graph.edges.get(id) else {
                continue;
            };
            let output = if self.selected_edges.contains(id) {
                &mut selected
            } else {
                &mut normal
            };
            let from = self.preview(edge.source.node);
            let to = self.preview(edge.target.node);
            if from.is_some() || to.is_some() {
                if let Ok(route) = EdgePath::new(graph, edge, from, to)
                    && let Some(path) = mesh(&route)
                {
                    append_transformed(output, &path, self.viewport, bounds.origin);
                }
            } else if let Some(route) = self.scene.paths().get(id)
                && let Some(path) = cache.get(*id, route)
            {
                append_transformed(output, path, self.viewport, bounds.origin);
            }
        }
        cache.batch_capacity = (normal.vertices.len(), selected.vertices.len());
        #[cfg(test)]
        {
            cache.batches = usize::from(!normal.vertices.is_empty())
                + usize::from(!selected.vertices.is_empty());
        }
        if !normal.vertices.is_empty() {
            window.paint_path(normal, self.surfaces.dimmed.to_hsla());
        }
        if !selected.vertices.is_empty() {
            window.paint_path(selected, self.accent.to_hsla());
        }
        // Markers and animated beads retain their order above the edge strokes.
        for id in &self.edges {
            let Some(edge) = graph.edges.get(id) else {
                continue;
            };
            if edge.marker_start == Marker::None
                && edge.marker_end == Marker::None
                && !(edge.animated && self.phase.is_some())
            {
                continue;
            }
            let from = self.preview(edge.source.node);
            let to = self.preview(edge.target.node);
            let route = if from.is_some() || to.is_some() {
                let Ok(route) = EdgePath::new(graph, edge, from, to) else {
                    continue;
                };
                std::borrow::Cow::Owned(route)
            } else {
                let Some(route) = self.scene.paths().get(id) else {
                    continue;
                };
                std::borrow::Cow::Borrowed(route.as_ref())
            };
            self.markers(&route, edge.marker_start, edge.marker_end, bounds, window);
            if edge.animated
                && let Some(t) = self.phase
            {
                let i = (t * (route.points.len().saturating_sub(1)) as f32) as usize;
                if let Some(p) = route.points.get(i) {
                    let p = pixel(self.viewport.screen(*p)) + bounds.origin;
                    let radius = px(2. * self.viewport.zoom);
                    window.paint_quad(fill(
                        Bounds::new(
                            p - point(radius, radius),
                            gpui_kit::size(radius * 2., radius * 2.),
                        ),
                        self.accent.to_hsla(),
                    ));
                }
                window.request_animation_frame();
            }
        }
        let edge_us = edge_started.elapsed().as_micros();
        cache.timings = (grid_us, edge_us, 0);
        cache.edge_buffer = std::mem::take(&mut self.edges);
    }
    fn grid(&self, b: Bounds<Pixels>, cache: &mut MeshCache, window: &mut Window) {
        if self.background == Background::None {
            return;
        }
        let raw = self.grid.filter(|g| g.is_finite()).unwrap_or(20.);
        let step = (raw.max(8.) * self.viewport.zoom).max(16.);
        let size = WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32());
        if cache
            .grid
            .as_ref()
            .is_none_or(|g| g.size != size || g.step != step || g.background != self.background)
        {
            let mut path = Path::new(pixel(WorldPoint::new(-step, -step)));
            let mut x = -step;
            while x < size.x + step {
                if self.background == Background::Lines {
                    rectangle(&mut path, Rect::new(x, -step, 0.6, size.y + 2. * step));
                } else {
                    let mut y = -step;
                    while y < size.y + step {
                        if self.background == Background::Cross {
                            rectangle(&mut path, Rect::new(x - 3., y - 0.3, 6., 0.6));
                            rectangle(&mut path, Rect::new(x - 0.3, y - 3., 0.6, 6.));
                        } else {
                            rectangle(&mut path, Rect::new(x - 0.6, y - 0.6, 1.2, 1.2));
                        }
                        y += step;
                    }
                }
                x += step;
            }
            if self.background == Background::Lines {
                let mut y = -step;
                while y < size.y + step {
                    rectangle(&mut path, Rect::new(-step, y, size.x + 2. * step, 0.6));
                    y += step;
                }
            }
            cache.grid = Some(GridMesh {
                size,
                step,
                background: self.background,
                path,
            });
        }
        if let Some(grid) = &cache.grid {
            let pan = WorldPoint::new(
                self.viewport.pan.x.rem_euclid(step),
                self.viewport.pan.y.rem_euclid(step),
            );
            paint_mesh(
                &grid.path,
                Viewport { pan, zoom: 1. },
                b.origin,
                window,
                self.surfaces.border,
            );
        }
    }
    fn markers(
        &self,
        route: &EdgePath,
        start: Marker,
        end: Marker,
        b: Bounds<Pixels>,
        window: &mut Window,
    ) {
        if route.points.len() < 2 {
            return;
        }
        let start_points = route
            .points
            .first()
            .zip(route.points.get(1))
            .map(|(tip, neighbor)| (start, *tip, *neighbor));
        let end_points = route
            .points
            .last()
            .zip(route.points.get(route.points.len().saturating_sub(2)))
            .map(|(tip, neighbor)| (end, *tip, *neighbor));
        for (marker, tip, neighbor) in start_points.into_iter().chain(end_points) {
            if marker == Marker::None {
                continue;
            }
            let Some([left, tip, right]) = marker_triangle(tip, neighbor, self.viewport) else {
                continue;
            };
            let mut builder = if marker == Marker::ArrowClosed {
                PathBuilder::fill()
            } else {
                PathBuilder::stroke(px(EDGE_STROKE * self.viewport.zoom))
            };
            builder.move_to(b.origin + pixel(left));
            builder.line_to(b.origin + pixel(tip));
            builder.line_to(b.origin + pixel(right));
            if marker == Marker::ArrowClosed {
                builder.close();
            }
            if let Ok(path) = builder.build() {
                window.paint_path(path, self.accent.to_hsla());
            }
        }
    }
    pub(crate) fn paint_minimap(
        &self,
        b: Bounds<Pixels>,
        cache: &mut MeshCache,
        window: &mut Window,
    ) {
        if !self.minimap {
            return;
        }
        let started = std::time::Instant::now();
        self.minimap(b, cache, window);
        cache.timings.2 = started.elapsed().as_micros();
    }
    fn minimap(&self, b: Bounds<Pixels>, cache: &mut MeshCache, window: &mut Window) {
        let size = WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32());
        let map = minimap_rect(size);
        let Some(bounds) = self.scene.bounds() else {
            return;
        };
        let camera = map_camera(size, bounds);
        if cache
            .minimap
            .as_ref()
            .is_none_or(|m| !Arc::ptr_eq(&m.scene, &self.scene) || m.size != size)
        {
            let nodes = &self.scene.graph().nodes;
            let normal = minimap_mesh(map, camera, nodes.len(), nodes.values());
            cache.minimap = Some(MinimapMeshes {
                scene: self.scene.clone(),
                size,
                normal,
                selection: None,
            });
        }
        if let Some(meshes) = &mut cache.minimap {
            paint_mesh(
                &meshes.normal,
                Viewport::default(),
                b.origin,
                window,
                self.surfaces.dimmed,
            );
            if meshes
                .selection
                .as_ref()
                .is_none_or(|(selected, _)| !Arc::ptr_eq(selected, &self.selected))
            {
                let nodes = &self.scene.graph().nodes;
                let path = minimap_mesh(
                    map,
                    camera,
                    self.selected.len(),
                    self.selected.iter().filter_map(|id| nodes.get(id)),
                );
                meshes.selection = Some((self.selected.clone(), path));
            }
            if let Some((_, path)) = &meshes.selection
                && !path.vertices.is_empty()
            {
                paint_mesh(path, Viewport::default(), b.origin, window, self.accent);
            }
        }
        let visible = self.viewport.visible(size);
        let screen = Rect {
            origin: camera.screen(visible.origin),
            size: visible.size * camera.zoom,
        };
        let min = WorldPoint::new(
            screen.origin.x.max(map.origin.x),
            screen.origin.y.max(map.origin.y),
        );
        let max = WorldPoint::new(
            screen.max().x.min(map.max().x),
            screen.max().y.min(map.max().y),
        );
        if min.x < max.x && min.y < max.y {
            window.paint_quad(fill(
                Bounds::new(
                    b.origin + pixel(min),
                    gpui_kit::size(px(max.x - min.x), px(max.y - min.y)),
                ),
                self.accent.alpha(0.27).to_hsla(),
            ));
            window.paint_quad(gpui_kit::outline(
                Bounds::new(
                    b.origin + pixel(min),
                    gpui_kit::size(px(max.x - min.x), px(max.y - min.y)),
                ),
                self.accent.to_hsla(),
                gpui_kit::BorderStyle::default(),
            ));
        }
    }
}
pub(crate) fn paint_gesture(
    scene: &Scene,
    gesture: Option<&Gesture>,
    viewport: Viewport,
    bounds: Bounds<Pixels>,
    accent: Rgb,
    window: &mut Window,
) {
    if let Some(Gesture::Connect { source, at }) = gesture
        && let Ok((node, handle)) = scene.graph().handle(source)
    {
        let from = handle.position(node.rect());
        let mut builder = PathBuilder::stroke(px(2. * viewport.zoom));
        builder.move_to(bounds.origin + pixel(viewport.screen(from)));
        builder.line_to(bounds.origin + pixel(viewport.screen(*at)));
        if let Ok(path) = builder.build() {
            window.paint_path(path, accent.to_hsla());
        }
    }
    if let Some(Gesture::Box { start, end, .. }) = gesture {
        let r = Rect::between(viewport.screen(*start), viewport.screen(*end));
        window.paint_quad(fill(
            Bounds::new(
                bounds.origin + pixel(r.origin),
                gpui_kit::size(px(r.size.x), px(r.size.y)),
            ),
            accent.alpha(0.2).to_hsla(),
        ));
    }
}
fn marker_triangle(
    tip: WorldPoint,
    neighbor: WorldPoint,
    viewport: Viewport,
) -> Option<[WorldPoint; 3]> {
    let direction = tip - neighbor;
    let distance = direction.distance(WorldPoint::default());
    if distance < 0.001 {
        return None;
    }
    let d = direction * (1. / distance);
    let side = WorldPoint::new(-d.y, d.x);
    // The tip of the arrow is the outer edge of the handle, so the arrow
    // touches the handle instead of pointing into it. The dot is painted
    // above the edge, so the edge itself is seen to end there as well.
    let tip = tip - d * crate::HANDLE_RADIUS;
    Some(
        [tip - d * 10. + side * 4., tip, tip - d * 10. - side * 4.]
            .map(|point| viewport.screen(point)),
    )
}
pub(crate) fn minimap_rect(size: WorldPoint) -> Rect {
    Rect::new((size.x - 196.).max(0.), (size.y - 136.).max(0.), 180., 120.)
}
fn map_camera(size: WorldPoint, bounds: Rect) -> Viewport {
    let map = minimap_rect(size);
    let zoom =
        ((map.size.x - 8.) / bounds.size.x.max(1.)).min((map.size.y - 8.) / bounds.size.y.max(1.));
    Viewport {
        zoom,
        pan: map.center() - bounds.center() * zoom,
    }
}
pub(crate) fn minimap_world(at: WorldPoint, size: WorldPoint, bounds: Rect) -> WorldPoint {
    map_camera(size, bounds).world(at)
}
#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_flow_core::{Edge, Graph};
    #[test]
    fn arrow_tip_touches_the_outer_edge_of_the_handle_at_all_zooms() {
        for zoom in [0.25, 0.5, 1., 2., 4.] {
            let viewport = Viewport {
                zoom,
                pan: WorldPoint::new(15., -20.),
            };
            for direction in [
                WorldPoint::new(1., 0.),
                WorldPoint::new(0., 1.),
                WorldPoint::new(-1., 0.),
            ] {
                let center = WorldPoint::new(200., 300.);
                let triangle =
                    marker_triangle(center, center - direction * 20., viewport).expect("marker");
                let screen = viewport.screen(center);
                let radius = crate::HANDLE_RADIUS * zoom;
                let base = (triangle[0] + triangle[2]) * 0.5;
                assert!((base.distance(triangle[1]) / radius - 2.).abs() < 0.001);
                assert!((triangle[0].distance(triangle[2]) / radius - 1.6).abs() < 0.001);
                assert!(
                    (triangle[1].distance(screen) - radius).abs() < 0.001,
                    "the tip of the arrow is the outer edge of the handle at zoom {zoom}"
                );
                for point in triangle {
                    assert!(
                        (point.x - screen.x) * direction.x + (point.y - screen.y) * direction.y
                            <= -radius + 0.001,
                        "the arrow stays outside the handle at zoom {zoom}"
                    );
                }
            }
        }
    }
    #[test]
    fn the_handle_dot_is_centred_on_the_border_of_the_node() {
        for zoom in [0.25, 0.5, 1., 2., 4.] {
            let node = Node::new(1, "", WorldPoint::new(40., 60.));
            let handle = node.handles.first().expect("handle");
            // The dot is a circle of the radius of a handle, centred on the
            // point where the handle sits: the border of the node.
            let centre = handle.position(node.rect());
            assert_eq!(
                centre.x,
                node.rect().origin.x,
                "the handle is on the border"
            );
            assert!(
                (centre.y - node.rect().origin.y - node.rect().size.y * handle.offset).abs()
                    < 0.001
            );
            let bounds = crate::node::handle_bounds(centre, zoom);
            assert!(bounds.center().distance(centre * zoom) < 0.001);
            assert!((bounds.size.x - 2. * crate::HANDLE_RADIUS * zoom).abs() < 0.001);
        }
    }
    #[test]
    fn cache_reuses_mesh_and_bounds_triangle_storage() {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::default()),
                Node::new(2, "", WorldPoint::new(400., 100.)),
            ],
            [Edge::new(1, 1, 2)],
        )
        .expect("graph");
        let route = Arc::new(EdgePath::new(&graph, &graph.edges[&1], None, None).expect("route"));
        let mut cache = MeshCache::default();
        let vertices = cache.get(1, &route).expect("mesh").vertices.len();
        assert!(vertices > 0);
        assert_eq!(cache.misses, 1);
        cache.get(1, &route).expect("cached");
        assert_eq!(cache.misses, 1);
        for id in 2..2200 {
            cache.get(id, &route).expect("mesh");
            assert_eq!(cache.entries.len(), id as usize);
        }
        cache.clear();
        assert_eq!(cache.vertices, 0);
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn transformed_mesh_metadata_contains_all_transformed_vertices() {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::default()),
                Node::new(2, "", WorldPoint::new(400., 100.)),
            ],
            [Edge::new(1, 1, 2)],
        )
        .expect("graph");
        let route = Arc::new(EdgePath::new(&graph, &graph.edges[&1], None, None).expect("route"));
        let cached = mesh(&route).expect("mesh");
        let viewport = Viewport {
            pan: WorldPoint::new(-150., 200.),
            zoom: 2.,
        };
        let translated = transformed_mesh(&cached, viewport, point(px(40.), px(70.)));
        assert_eq!(translated.vertices.len(), cached.vertices.len());
        for vertex in &translated.vertices {
            assert!(
                translated.bounds.contains(&vertex.xy_position),
                "bounds={:?}, vertex={:?}, source={:?}",
                translated.bounds,
                vertex.xy_position,
                cached.bounds
            );
        }
        assert_ne!(translated.bounds, cached.bounds);
    }
    #[test]
    fn minimap_mapping_centers_on_graph_and_roundtrips() {
        let size = WorldPoint::new(1000., 800.);
        let bounds = Rect::new(-100., -200., 5000., 3000.);
        let map = minimap_rect(size);
        let world = minimap_world(map.center(), size, bounds);
        assert!(world.distance(bounds.center()) < 0.001);
    }
    #[gpui_kit::test]
    fn dense_minimap_bounds_submitted_vertices(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};
        let graph = Graph::from_parts(
            (0..10_000).map(|id| {
                Node::new(
                    id,
                    "",
                    WorldPoint::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
                )
            }),
            [],
        )
        .expect("graph");
        let (window, view) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    crate::FlowViewport::new(graph, cx)
                        .expect("flow")
                        .options(crate::FlowOptions {
                            fit_on_init: false,
                            ..Default::default()
                        })
                })
            })
            .expect("window")
        });
        window
            .update(cx, |_, w, cx| {
                w.render_frame(cx);
                w.render_frame(cx);
            })
            .expect("frames");
        view.read_with(cx, |view, _| {
            let meshes = view.meshes.borrow();
            let vertices = &meshes.minimap.as_ref().expect("minimap").normal.vertices;
            assert!(!vertices.is_empty());
            assert!(vertices.len() <= 45 * 30 * 6, "{} vertices", vertices.len());
        });
    }

    #[test]
    fn review_10_second_frame_over_2048_edges_has_no_misses() {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::default()),
                Node::new(2, "", WorldPoint::new(400., 100.)),
            ],
            [],
        )
        .expect("graph");
        let route =
            Arc::new(EdgePath::new(&graph, &Edge::new(1, 1, 2), None, None).expect("route"));
        let mut cache = MeshCache::default();
        for id in 0..3000 {
            cache.get(id, &route).expect("mesh");
        }
        cache.misses = 0;
        for id in 0..3000 {
            cache.get(id, &route).expect("mesh");
        }
        assert_eq!(cache.misses, 0);
    }
    #[test]
    fn review_12_frame_transfers_visible_edge_buffer() {
        let i = Interaction::new(Graph::default()).expect("interaction");
        let mut visible = Visible {
            edges: vec![1, 2, 3],
            ..Default::default()
        };
        let storage = visible.edges.as_ptr();
        let frame = Frame::new(
            &i,
            &mut visible,
            Background::None,
            false,
            &GuiraffeTheme::default(),
            None,
        );
        assert_eq!(frame.edges.as_ptr(), storage);
    }
    #[gpui_kit::test]
    fn review_13_selection_does_not_rebuild_minimap(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};
        let graph =
            Graph::from_parts([Node::new(1, "", WorldPoint::new(100., 100.))], []).expect("graph");
        let (w, v) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| crate::FlowViewport::new(graph, cx).expect("flow"))
            })
            .expect("window")
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
        })
        .expect("frames");
        let before = v.read_with(cx, |v, _| {
            v.meshes
                .borrow()
                .minimap
                .as_ref()
                .expect("minimap")
                .normal
                .vertices
                .as_ptr()
        });
        v.update(cx, |v, cx| {
            v.interaction
                .editor
                .select(&std::collections::BTreeSet::from([1]), false);
            cx.notify();
        });
        w.update(cx, |_, w, cx| w.render_frame(cx))
            .expect("selection frame");
        let after = v.read_with(cx, |v, _| {
            v.meshes
                .borrow()
                .minimap
                .as_ref()
                .expect("minimap")
                .normal
                .vertices
                .as_ptr()
        });
        assert_eq!(before, after);
        let overlay_before = v.read_with(cx, |v, _| {
            v.meshes
                .borrow()
                .minimap
                .as_ref()
                .expect("minimap")
                .selection
                .as_ref()
                .expect("selection")
                .1
                .vertices
                .as_ptr()
        });
        v.update(cx, |v, cx| {
            v.set_viewport(
                Viewport {
                    pan: WorldPoint::new(20., 10.),
                    zoom: 1.,
                },
                cx,
            )
            .expect("camera");
        });
        w.update(cx, |_, w, cx| w.render_frame(cx))
            .expect("camera frame");
        let overlay_after = v.read_with(cx, |v, _| {
            v.meshes
                .borrow()
                .minimap
                .as_ref()
                .expect("minimap")
                .selection
                .as_ref()
                .expect("selection")
                .1
                .vertices
                .as_ptr()
        });
        assert_eq!(overlay_before, overlay_after);
    }
    #[test]
    fn review_12_view_frames_share_selection_storage() {
        let mut i = Interaction::new(
            Graph::from_parts([Node::new(1, "", WorldPoint::default())], []).expect("graph"),
        )
        .expect("interaction");
        i.editor.select(&BTreeSet::from([1]), false);
        let mut visible = Visible::default();
        let a = Frame::new(
            &i,
            &mut visible,
            Background::None,
            false,
            &GuiraffeTheme::default(),
            None,
        );
        let b = Frame::new(
            &i,
            &mut visible,
            Background::None,
            false,
            &GuiraffeTheme::default(),
            None,
        );
        assert_eq!(
            a.selected.iter().next().map(|id| id as *const u64),
            b.selected.iter().next().map(|id| id as *const u64)
        );
    }
    #[gpui_kit::test]
    fn review2_real_second_frame_over_2048_visible_edges_is_cached(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::new(100., 100.)),
                Node::new(2, "", WorldPoint::new(500., 100.)),
            ],
            (0..3000).map(|id| {
                let mut e = Edge::new(id, 1, 2);
                e.marker_end = Marker::None;
                e
            }),
        )
        .expect("graph");
        let (w, v) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    crate::FlowViewport::new(graph, cx)
                        .expect("flow")
                        .options(crate::FlowOptions {
                            background: Background::None,
                            minimap: false,
                            controls: false,
                            fit_on_init: false,
                            ..Default::default()
                        })
                })
            })
            .expect("window")
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
        })
        .expect("settle");
        v.update(cx, |v, _| v.meshes.borrow_mut().clear());
        w.update(cx, |_, w, cx| w.render_frame(cx))
            .expect("cold frame");
        assert_eq!(v.read_with(cx, |v, _| v.visible_counts().1), 3000);
        assert_eq!(v.read_with(cx, |v, _| v.meshes.borrow().misses), 3000);
        w.update(cx, |_, w, cx| w.render_frame(cx))
            .expect("warm frame");
        assert_eq!(v.read_with(cx, |v, _| v.meshes.borrow().misses), 0);
        assert_eq!(v.read_with(cx, |v, _| v.meshes.borrow().batches), 1);
    }
    #[test]
    fn review2_drag_frames_share_origin_storage() {
        let mut i = Interaction::new(
            Graph::from_parts([Node::new(1, "", WorldPoint::default())], []).expect("graph"),
        )
        .expect("interaction");
        i.press(WorldPoint::new(30., 30.), false, false);
        i.motion(WorldPoint::new(70., 50.));
        let mut visible = Visible::default();
        let a = Frame::new(
            &i,
            &mut visible,
            Background::None,
            false,
            &GuiraffeTheme::default(),
            None,
        );
        let b = Frame::new(
            &i,
            &mut visible,
            Background::None,
            false,
            &GuiraffeTheme::default(),
            None,
        );
        let storage = |f: &Frame| match &f.gesture {
            Some(Gesture::Move { origins, .. }) => origins.get(&1).map(|p| p as *const WorldPoint),
            _ => None,
        };
        assert!(storage(&a).is_some());
        assert_eq!(storage(&a), storage(&b));
    }
    #[test]
    fn review2_batched_transform_preserves_geometry_and_reserved_storage() {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::default()),
                Node::new(2, "", WorldPoint::new(400., 100.)),
            ],
            [],
        )
        .expect("graph");
        let route = EdgePath::new(&graph, &Edge::new(1, 1, 2), None, None).expect("route");
        let source = mesh(&route).expect("mesh");
        let viewport = Viewport {
            pan: WorldPoint::new(-100., 50.),
            zoom: 1.5,
        };
        let origin = point(px(20.), px(30.));
        let expected = transformed_mesh(&source, viewport, origin);
        let mut batch = Path::new(origin);
        batch.vertices.reserve_exact(source.vertices.len() * 20);
        let ptr = batch.vertices.as_ptr();
        for _ in 0..20 {
            append_transformed(&mut batch, &source, viewport, origin);
            assert_eq!(batch.vertices.as_ptr(), ptr);
        }
        assert_eq!(batch.vertices.len(), expected.vertices.len() * 20);
        for (actual, expected) in batch.vertices.iter().zip(&expected.vertices) {
            assert_eq!(actual.xy_position, expected.xy_position);
            assert!(batch.bounds.contains(&actual.xy_position));
        }
    }
}
