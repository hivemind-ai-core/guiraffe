//! Cached world-space tessellation with bounded storage and indexed culling.
use crate::{Background, Gesture, Interaction};
use gpui_kit::{Bounds, Path, PathBuilder, Pixels, Window, fill, point, px};
use guiraffe_core::{GuiraffeTheme, Rgb, Surfaces};
use guiraffe_flow_core::{
    EdgeId, EdgePath, Marker, Point as WorldPoint, Rect, Scene, Viewport, Visible,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Default)]
pub(crate) struct MeshCache {
    entries: BTreeMap<EdgeId, (Arc<EdgePath>, Path<Pixels>)>,
    pub edge_buffer: Vec<EdgeId>,
    vertices: usize,
    pub misses: usize,
    pub timings: (u128, u128, u128),
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
fn mesh(route: &EdgePath) -> Option<Path<Pixels>> {
    let mut builder = PathBuilder::stroke(px(1.5));
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
}
impl Frame {
    pub fn new(
        i: &Interaction,
        visible: &mut Visible,
        background: Background,
        minimap: bool,
        theme: &GuiraffeTheme,
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
        }
    }
    fn preview(&self, id: u64) -> Option<WorldPoint> {
        self.gesture
            .as_ref()
            .and_then(|g| g.preview(id, self.scene.graph(), self.grid))
    }
    pub fn paint(mut self, bounds: Bounds<Pixels>, cache: &mut MeshCache, window: &mut Window) {
        cache.misses = 0;
        let started = std::time::Instant::now();
        self.grid(bounds, cache, window);
        let grid_us = started.elapsed().as_micros();
        let edge_started = std::time::Instant::now();
        let graph = self.scene.graph();
        for id in &self.edges {
            let Some(edge) = graph.edges.get(id) else {
                continue;
            };
            let color = if self.selected_edges.contains(id) {
                self.accent
            } else {
                self.surfaces.dimmed
            };
            let from = self.preview(edge.source.node);
            let to = self.preview(edge.target.node);
            if from.is_some() || to.is_some() {
                if let Ok(route) = EdgePath::new(graph, edge, from, to) {
                    if let Some(path) = mesh(&route) {
                        paint_mesh(&path, self.viewport, bounds.origin, window, color);
                    }
                    self.markers(&route, edge.marker_start, edge.marker_end, bounds, window);
                }
            } else {
                let route = &self.scene.paths()[id];
                if let Some(path) = cache.get(*id, route) {
                    paint_mesh(path, self.viewport, bounds.origin, window, color);
                }
                self.markers(route, edge.marker_start, edge.marker_end, bounds, window);
                if edge.animated {
                    // A moving bead follows the cached polyline; only visible animated edges request frames.
                    let t = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0., |d| (d.as_millis() % 1000) as f32 / 1000.);
                    let i = (t * (route.points.len().saturating_sub(1)) as f32) as usize;
                    if let Some(p) = route.points.get(i) {
                        let p = pixel(self.viewport.screen(*p)) + bounds.origin;
                        window.paint_quad(fill(
                            Bounds::new(p - point(px(2.), px(2.)), gpui_kit::size(px(4.), px(4.))),
                            self.accent.to_hsla(),
                        ));
                    }
                    window.request_animation_frame();
                }
            }
        }
        if let Some(Gesture::Connect { source, at }) = &self.gesture
            && let Ok((node, handle)) = graph.handle(source)
        {
            let from = handle.position(node.rect());
            let mut builder = PathBuilder::stroke(px(2.));
            builder.move_to(bounds.origin + pixel(self.viewport.screen(from)));
            builder.line_to(bounds.origin + pixel(self.viewport.screen(*at)));
            if let Ok(path) = builder.build() {
                window.paint_path(path, self.accent.to_hsla());
            }
        }
        if let Some(Gesture::Box { start, end }) = &self.gesture {
            let r = Rect::between(self.viewport.screen(*start), self.viewport.screen(*end));
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + pixel(r.origin),
                    gpui_kit::size(px(r.size.x), px(r.size.y)),
                ),
                self.accent.alpha(0.2).to_hsla(),
            ));
        }
        let edge_us = edge_started.elapsed().as_micros();
        let minimap_started = std::time::Instant::now();
        if self.minimap {
            self.minimap(bounds, cache, window);
        }
        cache.timings = (grid_us, edge_us, minimap_started.elapsed().as_micros());
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
        for (marker, tip, neighbor) in [
            (start, route.points[0], route.points[1]),
            (
                end,
                route.points[route.points.len() - 1],
                route.points[route.points.len() - 2],
            ),
        ] {
            if marker == Marker::None {
                continue;
            }
            let direction = tip - neighbor;
            let distance = direction.distance(WorldPoint::default());
            if distance < 0.001 {
                continue;
            }
            let d = direction * (1. / distance);
            let side = WorldPoint::new(-d.y, d.x);
            let tip = self.viewport.screen(tip);
            let left = tip - d * 10. + side * 4.;
            let right = tip - d * 10. - side * 4.;
            let mut builder = if marker == Marker::ArrowClosed {
                PathBuilder::fill()
            } else {
                PathBuilder::stroke(px(1.5))
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
    fn minimap(&self, b: Bounds<Pixels>, cache: &mut MeshCache, window: &mut Window) {
        let size = WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32());
        let map = minimap_rect(size);
        window.paint_quad(fill(
            Bounds::new(
                b.origin + pixel(map.origin),
                gpui_kit::size(px(map.size.x), px(map.size.y)),
            ),
            self.surfaces.surface.to_hsla(),
        ));
        let Some(bounds) = self.scene.bounds() else {
            return;
        };
        let camera = map_camera(size, bounds);
        if cache
            .minimap
            .as_ref()
            .is_none_or(|m| !Arc::ptr_eq(&m.scene, &self.scene) || m.size != size)
        {
            let mut normal = Path::new(pixel(map.origin));

            for node in self.scene.graph().nodes.values() {
                let p = camera.screen(node.pos);
                let s = node.size * camera.zoom;
                let right = p.x + s.x.max(1.);
                let bottom = p.y + s.y.max(1.);
                let a = pixel(p);
                let b = pixel(WorldPoint::new(right, p.y));
                let c = pixel(WorldPoint::new(right, bottom));
                let d = pixel(WorldPoint::new(p.x, bottom));
                let path = &mut normal;
                let uv = (point(0., 1.), point(0., 1.), point(0., 1.));
                path.push_triangle((a, b, c), uv);
                path.push_triangle((a, c, d), uv);
            }
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
                let mut path = Path::new(pixel(map.origin));
                for id in self.selected.iter() {
                    if let Some(n) = self.scene.graph().nodes.get(id) {
                        rectangle(
                            &mut path,
                            Rect {
                                origin: camera.screen(n.pos),
                                size: WorldPoint::new(
                                    (n.size.x * camera.zoom).max(1.),
                                    (n.size.y * camera.zoom).max(1.),
                                ),
                            },
                        );
                    }
                }
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
        }
    }
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
    use guiraffe_flow_core::{Edge, Graph, Node};
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
                cx.new(|cx| crate::Flow::new(graph, cx).expect("flow"))
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
        );
        let b = Frame::new(
            &i,
            &mut visible,
            Background::None,
            false,
            &GuiraffeTheme::default(),
        );
        assert_eq!(
            a.selected.iter().next().map(|id| id as *const u64),
            b.selected.iter().next().map(|id| id as *const u64)
        );
    }
}
