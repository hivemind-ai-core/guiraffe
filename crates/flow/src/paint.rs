//! Cached world-space tessellation with bounded storage and indexed culling.
use crate::{Background, Gesture, Interaction};
use gpui_kit::{Bounds, Path, PathBuilder, Pixels, Window, fill, point, px, rgb};
use guiraffe_flow_core::{
    EdgeId, EdgePath, Marker, Point as WorldPoint, Rect, Scene, Viewport, Visible,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[derive(Default)]
pub(crate) struct MeshCache {
    entries: BTreeMap<EdgeId, Path<Pixels>>,
    order: VecDeque<EdgeId>,
    vertices: usize,
    pub misses: usize,
}
impl MeshCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.vertices = 0;
    }
    fn get(&mut self, id: EdgeId, route: &EdgePath) -> Option<&Path<Pixels>> {
        if !self.entries.contains_key(&id) {
            let path = mesh(route)?;
            if path.vertices.len() > 500_000 {
                return None;
            }
            while self.entries.len() >= 2048 || self.vertices + path.vertices.len() > 500_000 {
                let Some(old) = self.order.pop_front() else {
                    break;
                };
                if let Some(path) = self.entries.remove(&old) {
                    self.vertices -= path.vertices.len();
                }
            }
            self.vertices += path.vertices.len();
            self.misses += 1;
            self.order.push_back(id);
            self.entries.insert(id, path);
        }
        self.entries.get(&id)
    }
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
fn paint_mesh(
    path: &Path<Pixels>,
    viewport: Viewport,
    origin: gpui_kit::Point<Pixels>,
    window: &mut Window,
) {
    // GPUI owns the submitted triangles. Transform copies, preserving cached world vertices.
    let mut screen = Path::new(origin + pixel(viewport.pan));
    screen
        .vertices
        .extend(path.vertices.iter().cloned().map(|mut vertex| {
            vertex.xy_position = vertex.xy_position * viewport.zoom + origin + pixel(viewport.pan);
            vertex
        }));
    window.paint_path(screen, rgb(0x7186b3));
}
pub(crate) struct Frame {
    scene: Arc<Scene>,
    viewport: Viewport,
    edges: Vec<EdgeId>,
    background: Background,
    minimap: bool,
    gesture: Option<Gesture>,
    grid: Option<f32>,
}
impl Frame {
    pub fn new(i: &Interaction, visible: &Visible, background: Background, minimap: bool) -> Self {
        Self {
            scene: i.editor.scene_shared(),
            viewport: i.viewport,
            edges: visible.edges.clone(),
            background,
            minimap,
            gesture: i.gesture.clone(),
            grid: i.grid,
        }
    }
    fn preview(&self, id: u64) -> Option<WorldPoint> {
        if let Some(Gesture::Move { origins, delta, .. }) = &self.gesture {
            origins
                .get(&id)
                .map(|p| self.grid.map_or(*p + *delta, |g| (*p + *delta).snap(g)))
        } else {
            None
        }
    }
    pub fn paint(&self, bounds: Bounds<Pixels>, cache: &mut MeshCache, window: &mut Window) {
        cache.misses = 0;
        self.grid(bounds, window);
        let graph = self.scene.graph();
        for id in &self.edges {
            let edge = &graph.edges[id];
            let from = self.preview(edge.source.node);
            let to = self.preview(edge.target.node);
            if from.is_some() || to.is_some() {
                if let Ok(route) = EdgePath::new(graph, edge, from, to) {
                    if let Some(path) = mesh(&route) {
                        paint_mesh(&path, self.viewport, bounds.origin, window);
                    }
                    self.markers(&route, edge.marker_start, edge.marker_end, bounds, window);
                }
            } else {
                let route = &self.scene.paths[id];
                if let Some(path) = cache.get(*id, route) {
                    paint_mesh(path, self.viewport, bounds.origin, window);
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
                            rgb(0xb6c6ff),
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
                window.paint_path(path, rgb(0xa5b4fc));
            }
        }
        if let Some(Gesture::Box { start, end }) = &self.gesture {
            let r = Rect::between(self.viewport.screen(*start), self.viewport.screen(*end));
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + pixel(r.origin),
                    gpui_kit::size(px(r.size.x), px(r.size.y)),
                ),
                gpui_kit::rgba(0x8b9cff33),
            ));
        }
        if self.minimap {
            self.minimap(bounds, window);
        }
    }
    fn grid(&self, b: Bounds<Pixels>, window: &mut Window) {
        if self.background == Background::None {
            return;
        }
        let step = (self.grid.unwrap_or(20.).max(8.) * self.viewport.zoom).max(16.);
        let width = b.size.width.as_f32();
        let height = b.size.height.as_f32();
        let mut path = PathBuilder::stroke(px(0.6));
        let mut x = self.viewport.pan.x.rem_euclid(step);
        while x < width {
            let mut y = self.viewport.pan.y.rem_euclid(step);
            if self.background == Background::Lines {
                path.move_to(b.origin + pixel(WorldPoint::new(x, 0.)));
                path.line_to(b.origin + pixel(WorldPoint::new(x, height)));
            } else {
                while y < height {
                    let p = b.origin + pixel(WorldPoint::new(x, y));
                    let r = if self.background == Background::Cross {
                        3.
                    } else {
                        0.7
                    };
                    path.move_to(p - point(px(r), px(0.)));
                    path.line_to(p + point(px(r), px(0.)));
                    if self.background == Background::Cross {
                        path.move_to(p - point(px(0.), px(r)));
                        path.line_to(p + point(px(0.), px(r)));
                    }
                    y += step;
                }
            }
            x += step;
        }
        if self.background == Background::Lines {
            let mut y = self.viewport.pan.y.rem_euclid(step);
            while y < height {
                path.move_to(b.origin + pixel(WorldPoint::new(0., y)));
                path.line_to(b.origin + pixel(WorldPoint::new(width, y)));
                y += step;
            }
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(0x334155));
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
                window.paint_path(path, rgb(0x9caee0));
            }
        }
    }
    fn minimap(&self, b: Bounds<Pixels>, window: &mut Window) {
        let size = WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32());
        let map = minimap_rect(size);
        window.paint_quad(fill(
            Bounds::new(
                b.origin + pixel(map.origin),
                gpui_kit::size(px(map.size.x), px(map.size.y)),
            ),
            rgb(0x263249),
        ));
        let Some(bounds) = self.scene.bounds else {
            return;
        };
        let camera = map_camera(size, bounds);
        for node in self.scene.graph().nodes.values() {
            let p = camera.screen(node.pos);
            let s = node.size * camera.zoom;
            window.paint_quad(fill(
                Bounds::new(
                    b.origin + pixel(p),
                    gpui_kit::size(px(s.x.max(1.)), px(s.y.max(1.))),
                ),
                rgb(if node.selected { 0xa5b4fc } else { 0x7588af }),
            ));
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
                gpui_kit::rgba(0xa5b4fc44),
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
        let route = EdgePath::new(&graph, &graph.edges[&1], None, None).expect("route");
        let mut cache = MeshCache::default();
        let vertices = cache.get(1, &route).expect("mesh").vertices.len();
        assert!(vertices > 0);
        assert_eq!(cache.misses, 1);
        cache.get(1, &route).expect("cached");
        assert_eq!(cache.misses, 1);
        for id in 2..2200 {
            cache.get(id, &route).expect("mesh");
            assert!(cache.entries.len() <= 2048);
            assert!(cache.vertices <= 500_000);
        }
        cache.clear();
        assert_eq!(cache.vertices, 0);
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn minimap_mapping_centers_on_graph_and_roundtrips() {
        let size = WorldPoint::new(1000., 800.);
        let bounds = Rect::new(-100., -200., 5000., 3000.);
        let map = minimap_rect(size);
        let world = minimap_world(map.center(), size, bounds);
        assert!(world.distance(bounds.center()) < 0.001);
    }
}
