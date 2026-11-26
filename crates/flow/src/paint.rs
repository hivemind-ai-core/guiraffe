//! Cached world-space tessellation with bounded storage and indexed culling.
use crate::{Background, Gesture, Interaction};
use gpui_kit::{Bounds, Path, PathBuilder, PathVertex, Pixels, Window, fill, point, px};
use guiraffe_core::{GuiraffeTheme, Rgb, Surfaces};
use guiraffe_flow_core::{
    EdgeId, EdgePath, Marker, Node, Point as WorldPoint, Rect, Scene, StrokeStyle, Viewport,
    Visible,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
};

const EDGE_STROKE: f32 = 1.5;
// Tessellation uses world coordinates, so camera changes share the same mesh.
const MAX_EDGE_MESHES: usize = 8192;
const MAX_EDGE_MESH_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
struct EdgeVertex {
    position: gpui_kit::Point<Pixels>,
    uv: gpui_kit::Point<f32>,
}

#[derive(Clone)]
struct EdgeMesh {
    bounds: Bounds<Pixels>,
    vertices: Box<[EdgeVertex]>,
}
impl EdgeMesh {
    fn new(route: &EdgePath, style: StrokeStyle) -> Option<Self> {
        mesh(route, style, 0.).map(Self::from_path)
    }

    fn from_path(path: Path<Pixels>) -> Self {
        Self {
            bounds: path.bounds,
            // Stroke tessellation has no per-vertex clip. GPUI applies the
            // current clip to the submitted path, so omit it from retained storage.
            vertices: path
                .vertices
                .into_iter()
                .map(|vertex| EdgeVertex {
                    position: vertex.xy_position,
                    uv: vertex.st_position,
                })
                .collect(),
        }
    }

    fn bytes(&self) -> usize {
        std::mem::size_of_val(self.vertices.as_ref())
    }
}

// Deferred scenes retain only tessellation and sampled routes that the viewport used recently.
const DEFERRED_MESH_ENTRIES: usize = 1_024;
const DEFERRED_MESH_BYTES: usize = 1024 * 1024;
type MeshKey = (EdgeId, i8);

struct DeferredMesh {
    route: Arc<EdgePath>,
    style: StrokeStyle,
    mesh: Option<EdgeMesh>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct DeferredMeshCache {
    entries: BTreeMap<MeshKey, DeferredMesh>,
    recency: BTreeSet<(u64, MeshKey)>,
    bytes: usize,
    frame: u64,
}

impl DeferredMeshCache {
    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        if self.frame == 0 {
            *self = Self::default();
        }
    }

    fn get(
        &mut self,
        scene: &Scene,
        id: EdgeId,
        zoom: f32,
        area: Rect,
        style: StrokeStyle,
    ) -> Option<&DeferredMesh> {
        let key = (id, (zoom.log2() * 2.).floor() as i8);
        if self.entries.get(&key).is_some_and(|e| e.style != style)
            && let Some(stale) = self.entries.remove(&key)
        {
            self.recency.remove(&(stale.touched, key));
            self.bytes -= stale.bytes;
        }
        if self.entries.contains_key(&key) {
            let existing = self.entries.get_mut(&key)?;
            if !existing.route.bounds.intersects(area) {
                return None;
            }
            if existing.touched != self.frame {
                self.recency.remove(&(existing.touched, key));
                existing.touched = self.frame;
                self.recency.insert((self.frame, key));
            }
            return Some(existing);
        }

        let route = scene.edge_route(id)?;
        if !route.bounds.intersects(area) {
            return None;
        }
        let mesh = mesh_at_zoom(&route, key.1, style).map(EdgeMesh::from_path);
        let bytes = route.points.capacity() * std::mem::size_of::<WorldPoint>()
            + mesh.as_ref().map_or(0, EdgeMesh::bytes);
        while self.entries.len() >= DEFERRED_MESH_ENTRIES
            || bytes > DEFERRED_MESH_BYTES - self.bytes
        {
            let (_, oldest) = self.recency.pop_first()?;
            let evicted = self.entries.remove(&oldest)?;
            self.bytes -= evicted.bytes;
        }
        self.bytes += bytes;
        self.recency.insert((self.frame, key));
        self.entries.insert(
            key,
            DeferredMesh {
                route,
                style,
                mesh,
                bytes,
                touched: self.frame,
            },
        );
        self.entries.get(&key)
    }
}

#[derive(Default)]
pub(crate) struct MeshCache {
    entries: BTreeMap<EdgeId, (Arc<EdgePath>, StrokeStyle, EdgeMesh)>,
    deferred: DeferredMeshCache,
    deferred_scene: Weak<Scene>,
    pub edge_buffer: Vec<EdgeId>,
    mesh_bytes: usize,
    admission_limited: bool,
    last_mesh_viewport: Option<(Viewport, gpui_kit::Size<Pixels>)>,
    pub misses: usize,
    pub timings: (u128, u128, u128),
    batch_capacity: (usize, usize),
    #[cfg(test)]
    pub batches: usize,
    #[cfg(test)]
    pub marker_batches: usize,
    minimap: Option<MinimapMeshes>,
    grid: Option<GridMesh>,
}
impl MeshCache {
    pub fn refine_edge_candidates(
        &mut self,
        scene: &Arc<Scene>,
        viewport: Viewport,
        area: Rect,
        edges: &mut Vec<EdgeId>,
    ) {
        if scene.has_deferred_edges() {
            let current = Arc::downgrade(scene);
            if !self.deferred_scene.ptr_eq(&current) {
                self.deferred = DeferredMeshCache::default();
                self.deferred_scene = current;
            }
            self.deferred.begin_frame();
            let graph = scene.graph();
            edges.retain(|id| {
                let style = graph
                    .edges
                    .get(id)
                    .map_or(StrokeStyle::Solid, |edge| graph.stroke_of(edge));
                self.deferred
                    .get(scene, *id, viewport.zoom, area, style)
                    .is_some()
            });
        }
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.deferred = DeferredMeshCache::default();

        self.mesh_bytes = 0;
        self.admission_limited = false;
        self.last_mesh_viewport = None;
        self.minimap = None;
    }
    pub fn sync(&mut self, scene: &Scene) {
        if scene.has_deferred_edges() {
            self.deferred = DeferredMeshCache::default();
        }
        self.entries.retain(|id, (route, _, _)| {
            scene
                .paths()
                .get(id)
                .is_some_and(|next| Arc::ptr_eq(route, next))
        });
        self.mesh_bytes = self.entries.values().map(|(_, _, path)| path.bytes()).sum();
    }
    fn begin_edges(&mut self, viewport: (Viewport, gpui_kit::Size<Pixels>), visible: &[EdgeId]) {
        if self.admission_limited && self.last_mesh_viewport != Some(viewport) {
            self.entries.retain(|id, (_, _, path)| {
                if visible.binary_search(id).is_ok() {
                    return true;
                }
                self.mesh_bytes -= path.bytes();
                false
            });
            self.admission_limited = false;
        }
        self.last_mesh_viewport = Some(viewport);
    }
    fn get(
        &mut self,
        id: EdgeId,
        route: &Arc<EdgePath>,
        style: StrokeStyle,
    ) -> Option<Cow<'_, EdgeMesh>> {
        if self
            .entries
            .get(&id)
            .is_none_or(|(old, old_style, _)| !Arc::ptr_eq(old, route) || *old_style != style)
        {
            let path = EdgeMesh::new(route, style)?;
            if let Some((_, _, old)) = self.entries.remove(&id) {
                self.mesh_bytes -= old.bytes();
            }
            self.misses += 1;
            let bytes = path.bytes();
            if self.entries.len() >= MAX_EDGE_MESHES
                || bytes > MAX_EDGE_MESH_BYTES - self.mesh_bytes
            {
                self.admission_limited = true;
                return Some(Cow::Owned(path));
            }
            self.mesh_bytes += bytes;
            self.entries.insert(id, (route.clone(), style, path));
        }
        self.entries
            .get(&id)
            .map(|(_, _, path)| Cow::Borrowed(path))
    }
}
#[derive(Default)]
struct ClosedMarkers {
    path: Option<Path<Pixels>>,
    #[cfg(test)]
    submissions: usize,
}
impl ClosedMarkers {
    fn push(&mut self, vertices: [gpui_kit::Point<Pixels>; 3]) {
        let [left, tip, right] = vertices;
        let uv = point(0., 1.);
        self.path
            .get_or_insert_with(|| Path::new(left))
            .push_triangle((left, tip, right), (uv, uv, uv));
    }

    fn paint(&mut self, window: &mut Window, accent: Rgb) {
        if let Some(path) = self.path.take() {
            window.paint_path(path, accent.to_hsla());
            #[cfg(test)]
            {
                self.submissions += 1;
            }
        }
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

/// The edge's mesh in world space. A dash pattern is laid out in world units, so it scales
/// with the zoom like the line; `offset` moves it along the edge (animation).
fn mesh(route: &EdgePath, style: StrokeStyle, offset: f32) -> Option<Path<Pixels>> {
    stroke(route.points.iter().copied(), style, offset)
}

fn mesh_at_zoom(route: &EdgePath, bucket: i8, style: StrokeStyle) -> Option<Path<Pixels>> {
    // At every scale in this half-octave band the simplified centreline
    // stays within an eighth of a pixel of the exact route. Picking, labels
    // and arrowheads continue to use the exact points.
    const MAX_ROUTE_SAMPLES: usize = 33;
    let points = route.points.as_slice();
    let mut marked = [false; MAX_ROUTE_SAMPLES];
    let Some(keep) = marked.get_mut(..points.len()) else {
        return mesh(route, style, 0.);
    };
    let max_zoom = 2_f32.powf((f32::from(bucket) + 1.) * 0.5);
    mark_stroke_samples(points, keep, (0.125 / max_zoom).powi(2));
    stroke(
        points
            .iter()
            .zip(keep)
            .filter_map(|(point, keep)| (*keep).then_some(*point)),
        style,
        0.,
    )
}

fn mark_stroke_samples(points: &[WorldPoint], marked: &mut [bool], error_squared: f32) {
    if let Some(first) = marked.first_mut() {
        *first = true;
    }
    let [from, interior @ .., to] = points else {
        return;
    };
    if let Some(last) = marked.last_mut() {
        *last = true;
    }
    let direction = *to - *from;
    let length_squared = direction.x * direction.x + direction.y * direction.y;
    if !length_squared.is_finite() {
        marked.fill(true);
        return;
    }
    let mut farthest = error_squared;
    let mut split = None;
    for (offset, &point) in interior.iter().enumerate() {
        let relative = point - *from;
        let projection = if length_squared > 0. {
            ((relative.x * direction.x + relative.y * direction.y) / length_squared).clamp(0., 1.)
        } else {
            0.
        };
        let delta = relative - direction * projection;
        let squared = delta.x * delta.x + delta.y * delta.y;
        if !squared.is_finite() {
            marked.fill(true);
            return;
        }
        if squared > farthest {
            farthest = squared;
            split = Some(offset + 1);
        }
    }
    if let Some(at) = split {
        mark_stroke_samples(&points[..=at], &mut marked[..=at], error_squared);
        mark_stroke_samples(&points[at..], &mut marked[at..], error_squared);
    }
}

fn stroke(
    points: impl IntoIterator<Item = WorldPoint>,
    style: StrokeStyle,
    offset: f32,
) -> Option<Path<Pixels>> {
    let Some(pattern) = style.pattern(EDGE_STROKE, 1.) else {
        return solid_stroke(points);
    };
    let mut builder = PathBuilder::stroke(px(EDGE_STROKE));
    let mut any = false;
    let mut scratch = Vec::new();
    guiraffe_flow_core::dashes(
        points.into_iter().map(|p| (p.x, p.y)),
        pattern,
        offset,
        &mut scratch,
        |dash| {
            let mut dash = dash.iter();
            if let Some(&(x, y)) = dash.next() {
                builder.move_to(point(px(x), px(y)));
                for &(x, y) in dash {
                    builder.line_to(point(px(x), px(y)));
                }
                any = true;
            }
        },
    );
    if !any {
        return None;
    }
    builder.build().ok()
}

fn solid_stroke(points: impl IntoIterator<Item = WorldPoint>) -> Option<Path<Pixels>> {
    let mut points = points.into_iter();
    let mut builder = PathBuilder::stroke(px(EDGE_STROKE));
    builder.move_to(pixel(points.next()?));
    for point in points {
        builder.line_to(pixel(point));
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
    path: &EdgeMesh,
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
    let pan = pixel(viewport.pan);
    output
        .vertices
        .extend(path.vertices.iter().map(|vertex| PathVertex {
            xy_position: vertex.position * viewport.zoom + origin + pan,
            st_position: vertex.uv,
            content_mask: Default::default(),
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
    pub overview: bool,
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
            overview: false,
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
    pub(crate) fn with_viewport(mut self, viewport: Viewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub(crate) fn with_scene(mut self, scene: Arc<Scene>) -> Self {
        self.scene = scene;
        self
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
        if self.overview {
            let size = WorldPoint::new(bounds.size.width.as_f32(), bounds.size.height.as_f32());
            let area = self.viewport.visible(size);
            let mut path = Path::new(pixel(WorldPoint::default()));
            self.scene
                .visit_node_regions(area, 32. / self.viewport.zoom, |region| {
                    rectangle(
                        &mut path,
                        Rect {
                            origin: self.viewport.screen(region.origin),
                            size: region.size * self.viewport.zoom,
                        },
                    );
                });
            if !path.vertices.is_empty() {
                paint_mesh(
                    &path,
                    Viewport::default(),
                    bounds.origin,
                    window,
                    self.surfaces.dimmed,
                );
            }
            cache.timings = (grid_us, edge_started.elapsed().as_micros(), 0);
            cache.edge_buffer = std::mem::take(&mut self.edges);
            return;
        }
        cache.begin_edges((self.viewport, bounds.size), &self.edges);
        let deferred = self.scene.has_deferred_edges();
        let canvas_size = WorldPoint::new(bounds.size.width.as_f32(), bounds.size.height.as_f32());
        let area = self
            .viewport
            .visible(canvas_size)
            .expand(12. / self.viewport.zoom);
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
            let style = graph.stroke_of(edge);
            // A moving pattern is rebuilt each animation frame; reduced motion (no phase)
            // keeps it still and cached like any other edge.
            let moving = match (edge.animated, self.phase, style.pattern(EDGE_STROKE, 1.)) {
                (true, Some(t), Some((on, off))) => Some(-t * (on + off) * 4.),
                _ => None,
            };
            if from.is_some() || to.is_some() || moving.is_some() {
                let route = if from.is_some() || to.is_some() {
                    EdgePath::new(graph, edge, from, to).ok().map(Arc::new)
                } else {
                    self.scene.edge_route(*id)
                };
                if let Some(route) = route
                    && let Some(path) =
                        mesh(&route, style, moving.unwrap_or(0.)).map(EdgeMesh::from_path)
                {
                    append_transformed(output, &path, self.viewport, bounds.origin);
                }
            } else if deferred {
                if let Some(route) =
                    cache
                        .deferred
                        .get(&self.scene, *id, self.viewport.zoom, area, style)
                    && let Some(path) = &route.mesh
                {
                    append_transformed(output, path, self.viewport, bounds.origin);
                }
            } else if let Some(route) = self.scene.paths().get(id)
                && let Some(path) = cache.get(*id, route, style)
            {
                append_transformed(output, &path, self.viewport, bounds.origin);
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
        // Batch adjacent filled markers, flushing at primitives that used to
        // interleave with them so open markers and animated beads keep their z-order.
        let mut closed = ClosedMarkers::default();
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
            } else if deferred {
                let Some(route) = cache.deferred.get(
                    &self.scene,
                    *id,
                    self.viewport.zoom,
                    area,
                    graph.stroke_of(edge),
                ) else {
                    continue;
                };
                std::borrow::Cow::Borrowed(route.route.as_ref())
            } else {
                let Some(route) = self.scene.paths().get(id) else {
                    continue;
                };
                std::borrow::Cow::Borrowed(route.as_ref())
            };
            self.markers(
                &route,
                edge.marker_start,
                edge.marker_end,
                bounds,
                &mut closed,
                window,
            );
            if edge.animated
                && let Some(t) = self.phase
            {
                if graph.stroke_of(edge) != StrokeStyle::Solid {
                    // The pattern itself moves; no bead.
                    window.request_animation_frame();
                    continue;
                }
                let i = (t * (route.points.len().saturating_sub(1)) as f32) as usize;
                if let Some(p) = route.points.get(i) {
                    closed.paint(window, self.accent);
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
        closed.paint(window, self.accent);
        #[cfg(test)]
        {
            cache.marker_batches = closed.submissions;
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
        closed: &mut ClosedMarkers,
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
        let canvas = Rect::new(0., 0., b.size.width.as_f32(), b.size.height.as_f32());
        for (marker, tip, neighbor) in start_points.into_iter().chain(end_points) {
            if marker == Marker::None {
                continue;
            }
            let Some([left, tip, right]) = marker_triangle(tip, neighbor, self.viewport) else {
                continue;
            };
            let marker_bounds = Rect::between(left, tip)
                .union(Rect::between(tip, right))
                .expand(2. + EDGE_STROKE * self.viewport.zoom);
            if !marker_bounds.intersects(canvas) {
                continue;
            }
            let vertices = [left, tip, right].map(|p| b.origin + pixel(p));
            if marker == Marker::ArrowClosed {
                closed.push(vertices);
            } else {
                let mut builder = PathBuilder::stroke(px(EDGE_STROKE * self.viewport.zoom));
                let [left, tip, right] = vertices;
                builder.move_to(left);
                builder.line_to(tip);
                builder.line_to(right);
                if let Ok(path) = builder.build() {
                    closed.paint(window, self.accent);
                    window.paint_path(path, self.accent.to_hsla());
                }
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
        let vertices = cache
            .get(1, &route, StrokeStyle::Solid)
            .expect("mesh")
            .vertices
            .len();
        assert!(vertices > 0);
        assert_eq!(cache.misses, 1);
        cache.get(1, &route, StrokeStyle::Solid).expect("cached");
        assert_eq!(cache.misses, 1);
        for id in 2..2200 {
            cache.get(id, &route, StrokeStyle::Solid).expect("mesh");
            assert_eq!(cache.entries.len(), id as usize);
        }
        for id in 2200..(MAX_EDGE_MESHES as u64 + 100) {
            cache
                .get(id, &route, StrokeStyle::Solid)
                .expect("uncached meshes still paint");
        }
        assert!(cache.admission_limited);
        assert!(cache.entries.len() < MAX_EDGE_MESHES);
        assert!(cache.mesh_bytes <= MAX_EDGE_MESH_BYTES);
        assert!(
            cache.mesh_bytes
                + cache
                    .get(1, &route, StrokeStyle::Solid)
                    .expect("mesh")
                    .bytes()
                > MAX_EDGE_MESH_BYTES
        );
        cache.begin_edges((Viewport::default(), gpui_kit::Size::default()), &[1]);
        assert_eq!(cache.entries.len(), 1);
        let retained = cache
            .get(1, &route, StrokeStyle::Solid)
            .expect("retained")
            .bytes();
        assert_eq!(cache.mesh_bytes, retained);
        cache.clear();
        assert_eq!(cache.mesh_bytes, 0);
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn small_meshes_hit_the_entry_limit_before_the_byte_limit() {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::default()),
                Node::new(2, "", WorldPoint::new(400., 100.)),
            ],
            [],
        )
        .expect("graph");
        let mut edge = Edge::new(1, 1, 2);
        edge.edge_type = guiraffe_flow_core::EdgeType::Straight;
        let route = Arc::new(EdgePath::new(&graph, &edge, None, None).expect("route"));
        let mut cache = MeshCache::default();

        for id in 0..=MAX_EDGE_MESHES as EdgeId {
            cache.get(id, &route, StrokeStyle::Solid).expect("mesh");
        }

        assert_eq!(cache.entries.len(), MAX_EDGE_MESHES);
        assert!(cache.admission_limited);
        assert!(cache.mesh_bytes < MAX_EDGE_MESH_BYTES);
    }

    #[test]
    fn zoom_stroke_simplification_keeps_bends_and_reduces_straight_runs() {
        let points = [
            WorldPoint::new(0., 0.),
            WorldPoint::new(20., 0.),
            WorldPoint::new(40., 0.),
            WorldPoint::new(40., 20.),
            WorldPoint::new(40., 40.),
        ];
        let mut keep = [false; 5];
        mark_stroke_samples(&points, &mut keep, 0.125f32.powi(2));
        assert_eq!(keep, [true, false, true, false, true]);
    }

    #[test]
    fn deferred_mesh_cache_keeps_recent_routes_and_caps_zoom_buckets() {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::default()),
                Node::new(2, "", WorldPoint::new(400., 100.)),
            ],
            (0..3_000).map(|id| Edge::new(id, 1, 2)),
        )
        .expect("graph");
        let scene = Scene::new_deferred(graph).expect("deferred scene");
        let mut cache = DeferredMeshCache::default();
        let area = Rect::new(0., 0., 800., 600.);
        for id in 0..3_000 {
            cache.begin_frame();
            cache
                .get(&scene, 0, 1.25, area, StrokeStyle::Solid)
                .expect("recurring edge");
            cache
                .get(&scene, id, 1., area, StrokeStyle::Solid)
                .expect("new edge");
            assert!(cache.entries.len() <= DEFERRED_MESH_ENTRIES);
            assert!(cache.bytes <= DEFERRED_MESH_BYTES);
        }
        assert!(cache.entries.contains_key(&(0, 0)));
        assert!(!cache.entries.contains_key(&(1, 0)));
        let shared = cache
            .get(&scene, 0, 1., area, StrokeStyle::Solid)
            .expect("base zoom")
            .route
            .clone();
        let bucket = cache
            .get(&scene, 0, 2., area, StrokeStyle::Solid)
            .expect("next bucket")
            .route
            .clone();
        assert!(!Arc::ptr_eq(&shared, &bucket));
        assert_eq!(shared, bucket);
        assert!(scene.paths().is_empty());
    }

    #[test]
    fn deferred_mesh_cache_replaces_routes_when_display_scene_changes() {
        let scene = |target_y| {
            Arc::new(
                Scene::new_deferred(
                    Graph::from_parts(
                        [
                            Node::new(1, "", WorldPoint::default()),
                            Node::new(2, "", WorldPoint::new(400., target_y)),
                        ],
                        [Edge::new(1, 1, 2)],
                    )
                    .expect("graph"),
                )
                .expect("scene"),
            )
        };
        let first = scene(100.);
        let second = scene(300.);
        let mut cache = MeshCache::default();
        let area = Rect::new(0., 0., 800., 600.);
        let mut edges = vec![1];
        cache.refine_edge_candidates(&first, Viewport::default(), area, &mut edges);
        let first_route = cache.deferred.entries[&(1, 0)].route.clone();
        cache.refine_edge_candidates(&second, Viewport::default(), area, &mut edges);
        assert_ne!(&first_route, &cache.deferred.entries[&(1, 0)].route);
        assert_eq!(Arc::strong_count(&first), 1);
    }

    #[test]
    fn mesh_cache_stays_bounded_over_thirty_minutes_of_camera_changes() {
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
        let size = gpui_kit::size(px(800.), px(600.));

        // At 120 Hz this covers 30 minutes. Every fifteenth frame enters a
        // new region, interleaved with zooms and repaints of the same edges.
        for frame in 0..(30 * 60 * 120) {
            let first = frame / 15 * 2;
            let visible = [first, first + 1];
            let viewport = Viewport {
                pan: WorldPoint::new(-(first as f32), 0.),
                zoom: if frame % 2 == 0 { 1. } else { 1.25 },
            };
            cache.begin_edges((viewport, size), &visible);
            let misses_before = cache.misses;
            for id in visible {
                cache.get(id, &route, StrokeStyle::Solid).expect("mesh");
            }
            if frame % 15 >= 2 {
                assert_eq!(cache.misses, misses_before);
            }
            assert!(cache.entries.len() <= MAX_EDGE_MESHES);
            assert!(cache.mesh_bytes <= MAX_EDGE_MESH_BYTES);
        }
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
        let cached = mesh(&route, StrokeStyle::Solid, 0.).expect("mesh");
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
    #[ignore = "materializes and indexes 10 million nodes; run separately from the workspace gate"]
    fn ten_million_nodes_keep_detailed_viewport_work_constant(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};

        let mut results = Vec::new();
        for node_count in [10_000, 10_000_000] {
            let graph = Graph::from_parts(
                (0..node_count).map(|id| {
                    Node::new(
                        id,
                        "",
                        WorldPoint::new((id % 1000) as f32 * 240., (id / 1000) as f32 * 150.),
                    )
                }),
                (0..100).map(|id| Edge::new(id, id, id + 1)),
            )
            .expect("graph");
            let (window, view) = cx.update(|cx| {
                guiraffe_core::init(cx);
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    cx.new(|cx| {
                        crate::FlowViewport::new(graph, cx).expect("flow").options(
                            crate::FlowOptions {
                                fit_on_init: false,
                                minimap: false,
                                ..Default::default()
                            },
                        )
                    })
                })
                .expect("window")
            });
            window
                .update(cx, |_, w, cx| w.render_frame(cx))
                .expect("mount");
            cx.run_until_parked();
            let mut counts = Vec::new();
            for (pan, zoom) in [(0., 1.), (-240., 1.25), (-480., 1.)] {
                view.update(cx, |view, cx| {
                    view.set_viewport(
                        Viewport {
                            pan: WorldPoint::new(pan, 0.),
                            zoom,
                        },
                        cx,
                    )
                    .expect("camera");
                });
                window
                    .update(cx, |_, w, cx| w.render_frame(cx))
                    .expect("frame");
                counts.push(view.read_with(cx, |view, _| {
                    assert!(!view.interaction.editor.scene().index_pending());
                    let meshes = view.meshes.borrow();
                    assert!(meshes.batch_capacity.0 > 0);
                    eprintln!(
                        "{node_count} nodes, pan={pan}, zoom={zoom}: {} BVH visits",
                        view.visible.visited
                    );
                    (
                        view.visible.nodes.clone(),
                        view.visible_counts(),
                        meshes.batch_capacity,
                        meshes.batches,
                        meshes.marker_batches,
                    )
                }));
            }
            results.push(counts);
            window
                .update(cx, |_, window, _| window.remove_window())
                .expect("close");
            cx.run_until_parked();
        }
        assert_eq!(results[0], results[1]);
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

    #[gpui_kit::test]
    fn intermediate_zoom_uses_overview_only_for_dense_viewports(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};
        let graph = Graph::from_parts(
            (0..5_000).map(|id| {
                Node::new(
                    id,
                    "",
                    WorldPoint::new((id % 100) as f32 * 240., (id / 100) as f32 * 150.),
                )
            }),
            (0..5_000).map(|id| Edge::new(id, id, (id + 1) % 5_000)),
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
                            minimap: false,
                            ..Default::default()
                        })
                })
            })
            .expect("window")
        });
        for (pan_x, dense) in [(0., true), (-23_760. * 0.075, false)] {
            view.update(cx, |view, cx| {
                view.set_viewport(
                    Viewport {
                        pan: WorldPoint::new(pan_x, 0.),
                        zoom: 0.075,
                    },
                    cx,
                )
                .expect("camera");
            });
            window
                .update(cx, |_, window, cx| {
                    window.render_frame(cx);
                    window.render_frame(cx);
                })
                .expect("frame");
            let (nodes, edges) = view.read_with(cx, |view, _| view.visible_counts());
            if dense {
                assert_eq!((nodes, edges), (0, 0));
            } else {
                assert!(nodes > 0 && edges > 0);
            }
        }
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
            cache.get(id, &route, StrokeStyle::Solid).expect("mesh");
        }
        cache.misses = 0;
        for id in 0..3000 {
            cache.get(id, &route, StrokeStyle::Solid).expect("mesh");
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
            (0..3000).map(|id| Edge::new(id, 1, 2)),
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
        v.read_with(cx, |v, _| {
            let cache = v.meshes.borrow();
            assert_eq!(cache.batches, 1);
            assert_eq!(cache.marker_batches, 1);
        });
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
        let source = mesh(&route, StrokeStyle::Solid, 0.).expect("mesh");
        let viewport = Viewport {
            pan: WorldPoint::new(-100., 50.),
            zoom: 1.5,
        };
        let origin = point(px(20.), px(30.));
        let expected = transformed_mesh(&source, viewport, origin);
        let source = EdgeMesh::from_path(source);
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
            assert_eq!(actual.st_position, expected.st_position);
            assert_eq!(actual.content_mask.bounds, expected.content_mask.bounds);
            assert!(batch.bounds.contains(&actual.xy_position));
        }
    }
}
