//! Immutable render geometry with independent node and edge BVHs.
//! Inspired by graph-core's immutable RenderData hierarchy; edge bounds include
//! every cached sample, including backtracking curves and self loops.
use crate::{Edge, EdgeId, EdgeType, FlowError, Graph, NodeId, Point, Rect};
use std::collections::BTreeMap;

use std::sync::Arc;

#[derive(Clone, Debug)]
struct Branch {
    bounds: Option<Rect>,
    entries: Vec<(u64, Rect)>,
    children: Option<(Arc<Branch>, Arc<Branch>)>,
}
/// Spatially bulk-built BVH with persistent branches. Updates refit only affected paths.
#[derive(Clone, Debug, Default)]
pub struct SpatialIndex {
    root: Option<Arc<Branch>>,
    locations: im::OrdMap<u64, Vec<bool>>,
}
impl SpatialIndex {
    pub fn new(entries: impl IntoIterator<Item = (u64, Rect)>) -> Self {
        let mut entries: Vec<_> = entries.into_iter().filter(|(_, r)| r.valid()).collect();
        let mut index = Self::default();
        if !entries.is_empty() {
            index.root = Some(Self::build(
                &mut entries,
                &mut Vec::new(),
                &mut index.locations,
            ));
        }
        index
    }
    fn build(
        entries: &mut [(u64, Rect)],
        path: &mut Vec<bool>,
        locations: &mut im::OrdMap<u64, Vec<bool>>,
    ) -> Arc<Branch> {
        let bounds = entries.iter().map(|(_, r)| *r).reduce(Rect::union);
        if entries.len() <= 8 {
            for (id, _) in entries.iter() {
                locations.insert(*id, path.clone());
            }
            return Arc::new(Branch {
                bounds,
                entries: entries.to_vec(),
                children: None,
            });
        }
        let x = bounds.is_none_or(|b| b.size.x >= b.size.y);
        let mid = entries.len() / 2;
        entries.select_nth_unstable_by(mid, |a, b| {
            let coord = |r: Rect| if x { r.center().x } else { r.center().y };
            coord(a.1).total_cmp(&coord(b.1)).then(a.0.cmp(&b.0))
        });
        let (left, right) = entries.split_at_mut(mid);
        path.push(false);
        let left = Self::build(left, path, locations);
        path.pop();
        path.push(true);
        let right = Self::build(right, path, locations);
        path.pop();
        Arc::new(Branch {
            bounds,
            entries: Vec::new(),
            children: Some((left, right)),
        })
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.root.as_ref().and_then(|r| r.bounds)
    }
    /// Insert, remove or refit entries by copying only affected branches.
    pub fn update(&mut self, changes: &BTreeMap<u64, Option<Rect>>) {
        for (id, rect) in changes {
            if let Some(path) = self.locations.get(id).cloned() {
                if let Some(root) = &mut self.root {
                    Self::refit(root, &path, *id, *rect);
                }
                if rect.is_none() {
                    self.locations.remove(id);
                }
            } else if let Some(rect) = rect.filter(|r| r.valid()) {
                if let Some(root) = &mut self.root {
                    Self::insert(root, (*id, rect), &mut Vec::new(), &mut self.locations);
                } else {
                    *self = Self::new([(*id, rect)]);
                }
            }
        }
    }
    fn insert(
        root: &mut Arc<Branch>,
        entry: (u64, Rect),
        path: &mut Vec<bool>,
        locations: &mut im::OrdMap<u64, Vec<bool>>,
    ) {
        let branch = Arc::make_mut(root);
        if let Some((left, right)) = &mut branch.children {
            let cost = |b: &Branch| {
                b.bounds.map_or(0., |r| {
                    let union = r.union(entry.1);
                    f64::from(union.size.x) * f64::from(union.size.y)
                        - f64::from(r.size.x) * f64::from(r.size.y)
                })
            };
            let to_right = cost(right) < cost(left);
            path.push(to_right);
            Self::insert(if to_right { right } else { left }, entry, path, locations);
            path.pop();
            branch.bounds = match (left.bounds, right.bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            };
        } else {
            branch.entries.push(entry);
            if branch.entries.len() > 8 {
                *root = Self::build(&mut branch.entries, path, locations);
            } else {
                locations.insert(entry.0, path.clone());
                branch.bounds = branch.entries.iter().map(|(_, r)| *r).reduce(Rect::union);
            }
        }
    }
    fn refit(root: &mut Arc<Branch>, path: &[bool], id: u64, rect: Option<Rect>) {
        let branch = Arc::make_mut(root);
        if let Some((l, r)) = &mut branch.children {
            if let Some((right, rest)) = path.split_first() {
                Self::refit(if *right { r } else { l }, rest, id, rect);
            }
            branch.bounds = match (l.bounds, r.bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            };
        } else {
            branch.entries.retain(|(key, _)| *key != id);
            if let Some(rect) = rect {
                branch.entries.push((id, rect));
            }
            branch.bounds = branch.entries.iter().map(|(_, r)| *r).reduce(Rect::union);
        }
    }
    pub fn query(&self, area: Rect, output: &mut Vec<u64>) -> usize {
        output.clear();
        if !area.valid() {
            return 0;
        }
        let visited = self
            .root
            .as_ref()
            .map_or(0, |r| Self::visit(r, area, output));
        output.sort_unstable();
        visited
    }
    fn visit(branch: &Branch, area: Rect, output: &mut Vec<u64>) -> usize {
        if branch.bounds.is_none_or(|r| !r.intersects(area)) {
            return 1;
        }
        if let Some((l, r)) = &branch.children {
            1 + Self::visit(l, area, output) + Self::visit(r, area, output)
        } else {
            output.extend(
                branch
                    .entries
                    .iter()
                    .filter(|(_, r)| r.intersects(area))
                    .map(|(id, _)| *id),
            );
            1
        }
    }
}

/// Flattened world-space route: tessellation input stays cached across camera moves.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePath {
    pub points: Vec<Point>,
    pub bounds: Rect,
    pub label: Point,
}
impl EdgePath {
    pub fn new(
        graph: &Graph,
        edge: &Edge,
        source_override: Option<Point>,
        target_override: Option<Point>,
    ) -> Result<Self, FlowError> {
        let (source, sh) = graph.handle(&edge.source)?;
        let (target, th) = graph.handle(&edge.target)?;
        let from = sh.position(Rect {
            origin: source_override.unwrap_or(source.pos),
            size: source.size,
        });
        let to = th.position(Rect {
            origin: target_override.unwrap_or(target.pos),
            size: target.size,
        });
        let distance = from.distance(to).mul_add(0.4, 24.).min(300.);
        let c1 = from + sh.side.direction() * distance;
        let c2 = to + th.side.direction() * distance;
        let points = match edge.edge_type {
            EdgeType::Straight => vec![from, to],
            EdgeType::Bezier => (0..=32)
                .map(|i| {
                    let t = i as f32 / 32.;
                    let u = 1. - t;
                    from * (u * u * u)
                        + c1 * (3. * u * u * t)
                        + c2 * (3. * u * t * t)
                        + to * (t * t * t)
                })
                .collect(),
            EdgeType::Step | EdgeType::SmoothStep => {
                let horizontal = sh.side.direction().x != 0.;
                let (a, b) = if horizontal {
                    let x = (c1.x + c2.x) * 0.5;
                    (Point::new(x, from.y), Point::new(x, to.y))
                } else {
                    let y = (c1.y + c2.y) * 0.5;
                    (Point::new(from.x, y), Point::new(to.x, y))
                };
                let corners = [from, c1, a, b, c2, to];
                if edge.edge_type == EdgeType::Step {
                    corners.to_vec()
                } else {
                    rounded(&corners, 12.)
                }
            }
        };
        if !points.iter().all(|point| point.finite()) {
            return Err(FlowError::InvalidGeometry);
        }
        let bounds = points
            .iter()
            .map(|p| Rect {
                origin: *p,
                size: Point::default(),
            })
            .reduce(Rect::union)
            .unwrap_or_default()
            .expand(2.);
        if !bounds.valid() {
            return Err(FlowError::InvalidGeometry);
        }
        let label = midpoint(&points);
        if !label.finite() {
            return Err(FlowError::InvalidGeometry);
        }
        Ok(Self {
            points,
            bounds,
            label,
        })
    }
    pub fn distance(&self, point: Point) -> f32 {
        self.points
            .windows(2)
            .map(|s| segment_distance(point, s[0], s[1]))
            .fold(f32::INFINITY, f32::min)
    }
}
fn rounded(corners: &[Point], radius: f32) -> Vec<Point> {
    let mut points = vec![corners[0]];
    for i in 1..corners.len() - 1 {
        let center = corners[i];
        let before = corners[i - 1] - center;
        let after = corners[i + 1] - center;
        let da = before.distance(Point::default());
        let db = after.distance(Point::default());
        if da < 0.001 || db < 0.001 {
            continue;
        }
        let r = radius.min(da * 0.5).min(db * 0.5);
        let a = center + before * (r / da);
        let b = center + after * (r / db);
        points.push(a);
        for step in 1..=6 {
            let t = step as f32 / 6.;
            let u = 1. - t;
            points.push(a * (u * u) + center * (2. * u * t) + b * (t * t));
        }
    }
    if let Some(last) = corners.last() {
        points.push(*last);
    }
    points
}
fn midpoint(points: &[Point]) -> Point {
    let length: f32 = points.windows(2).map(|p| p[0].distance(p[1])).sum();
    let mut remaining = length * 0.5;
    for p in points.windows(2) {
        let d = p[0].distance(p[1]);
        if d > 0. && remaining <= d {
            return p[0] + (p[1] - p[0]) * (remaining / d);
        }
        remaining -= d;
    }
    points.first().copied().unwrap_or_default()
}
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let v = b - a;
    let d = v.x * v.x + v.y * v.y;
    let t = if d > 0. {
        (((p.x - a.x) * v.x + (p.y - a.y) * v.y) / d).clamp(0., 1.)
    } else {
        0.
    };
    p.distance(a + v * t)
}
#[derive(Clone, Debug, Default)]
pub struct Visible {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub visited: usize,
}
#[derive(Clone, Debug)]
pub struct Scene {
    graph: Graph,
    paths: im::OrdMap<EdgeId, Arc<EdgePath>>,
    nodes: SpatialIndex,
    edges: SpatialIndex,
    bounds: Option<Rect>,
    incident: im::OrdMap<NodeId, Vec<EdgeId>>,
    depths: im::OrdMap<NodeId, usize>,
    revisions: im::OrdMap<NodeId, u64>,
    revision: u64,
    children: im::OrdMap<NodeId, Vec<NodeId>>,
}
impl Scene {
    pub fn new(graph: Graph) -> Result<Self, FlowError> {
        graph.validate()?;
        let paths: im::OrdMap<EdgeId, Arc<EdgePath>> = graph
            .edges
            .values()
            .map(|e| EdgePath::new(&graph, e, None, None).map(|p| (e.id, Arc::new(p))))
            .collect::<Result<_, _>>()?;
        let nodes = SpatialIndex::new(graph.nodes.values().map(|n| (n.id, n.rect())));
        let edges = SpatialIndex::new(paths.iter().map(|(id, path)| (*id, path.bounds)));
        let bounds = graph.bounds();
        let mut incident: im::OrdMap<NodeId, Vec<EdgeId>> = im::OrdMap::new();
        for edge in graph.edges.values() {
            incident.entry(edge.source.node).or_default().push(edge.id);
            if edge.source.node != edge.target.node {
                incident.entry(edge.target.node).or_default().push(edge.id);
            }
        }
        let mut children: im::OrdMap<NodeId, Vec<NodeId>> = im::OrdMap::new();
        for n in graph.nodes.values() {
            if let Some(parent) = n.parent {
                children.entry(parent).or_default().push(n.id);
            }
        }
        let depths = Self::depths(&graph);
        let revisions = graph.nodes.keys().map(|id| (*id, 0u64)).collect();
        Ok(Self {
            depths,
            revisions,
            revision: 0,
            children,
            graph,
            paths,
            nodes,
            edges,
            bounds,
            incident,
        })
    }
    pub fn paths(&self) -> &im::OrdMap<EdgeId, Arc<EdgePath>> {
        &self.paths
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }
    pub fn incident_edges(&self, id: NodeId) -> &[EdgeId] {
        self.incident.get(&id).map_or(&[], Vec::as_slice)
    }
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
    fn depths(graph: &Graph) -> im::OrdMap<NodeId, usize> {
        let mut result = im::OrdMap::new();
        for id in graph.nodes.keys() {
            let mut chain = Vec::new();
            let mut next = Some(*id);
            while let Some(id) = next {
                if result.contains_key(&id) {
                    break;
                }
                chain.push(id);
                next = graph.nodes[&id].parent;
            }
            let depth = next
                .and_then(|id| result.get(&id).copied())
                .map_or(0, |d| d + 1);
            for (offset, id) in chain.into_iter().rev().enumerate() {
                result.insert(id, depth + offset);
            }
        }
        result
    }
    pub fn node_revision(&self, id: NodeId) -> u64 {
        self.revisions.get(&id).copied().unwrap_or(0)
    }
    pub fn z_key(&self, id: NodeId) -> (usize, NodeId) {
        (self.depths.get(&id).copied().unwrap_or(0), id)
    }
    /// Share untouched routes, graph pages and BVH branches with the previous scene.
    pub(crate) fn updated(
        &self,
        graph: Graph,
        changed_nodes: &std::collections::BTreeSet<NodeId>,
        changed_edges: &std::collections::BTreeSet<EdgeId>,
    ) -> Result<Self, FlowError> {
        let mut next = self.clone();
        next.revision = self.revision.wrapping_add(1);
        let node_changes: BTreeMap<_, _> = changed_nodes
            .iter()
            .map(|id| (*id, graph.nodes.get(id).map(|n| n.rect())))
            .collect();
        next.nodes.update(&node_changes);
        next.bounds = next.nodes.bounds();
        if next.bounds.is_some_and(|b| !b.valid()) {
            return Err(FlowError::InvalidGeometry);
        }
        let mut edges = changed_edges.clone();
        for id in changed_nodes {
            edges.extend(self.incident_edges(*id));
            if graph.nodes.contains_key(id) {
                next.revisions.insert(*id, next.revision);
            } else {
                next.revisions.remove(id);
            }
        }
        let mut edge_changes = BTreeMap::new();
        for id in edges {
            if let Some(edge) = graph.edges.get(&id) {
                let path = Arc::new(EdgePath::new(&graph, edge, None, None)?);
                edge_changes.insert(id, Some(path.bounds));
                next.paths.insert(id, path);
            } else {
                next.paths.remove(&id);
                edge_changes.insert(id, None);
            }
        }
        next.edges.update(&edge_changes);
        for id in changed_edges {
            if let Some(old) = self.graph.edges.get(id) {
                for node in [old.source.node, old.target.node] {
                    if let Some(ids) = next.incident.get_mut(&node) {
                        ids.retain(|i| i != id);
                    }
                }
            }
            if let Some(edge) = graph.edges.get(id) {
                next.incident.entry(edge.source.node).or_default().push(*id);
                if edge.source.node != edge.target.node {
                    next.incident.entry(edge.target.node).or_default().push(*id);
                }
            }
        }
        if changed_nodes.iter().any(|id| {
            self.graph.nodes.get(id).map(|n| n.parent) != graph.nodes.get(id).map(|n| n.parent)
        }) {
            next.depths = Self::depths(&graph);
        }
        for id in changed_nodes {
            if let Some(parent) = self.graph.nodes.get(id).and_then(|n| n.parent)
                && let Some(ids) = next.children.get_mut(&parent)
            {
                ids.retain(|i| i != id);
            }
            if let Some(parent) = graph.nodes.get(id).and_then(|n| n.parent) {
                next.children.entry(parent).or_default().push(*id);
            }
        }
        next.graph = graph;
        Ok(next)
    }
    pub fn visible(&self, area: Rect, output: &mut Visible) {
        output.visited =
            self.nodes.query(area, &mut output.nodes) + self.edges.query(area, &mut output.edges);
    }
    pub fn query_nodes(&self, area: Rect, output: &mut Vec<NodeId>) {
        self.nodes.query(area, output);
    }
    pub fn hit_node(&self, at: Point, scratch: &mut Vec<NodeId>) -> Option<NodeId> {
        self.nodes.query(
            Rect {
                origin: at,
                size: Point::default(),
            },
            scratch,
        );
        scratch.iter().copied().max_by_key(|id| self.z_key(*id))
    }
    pub fn hit_edge(&self, at: Point, tolerance: f32, scratch: &mut Vec<EdgeId>) -> Option<EdgeId> {
        if !tolerance.is_finite() || tolerance < 0. {
            return None;
        }
        self.edges.query(
            Rect {
                origin: at,
                size: Point::default(),
            }
            .expand(tolerance),
            scratch,
        );
        scratch.iter().rev().copied().find(|id| {
            self.paths
                .get(id)
                .is_some_and(|p| p.distance(at) <= tolerance)
        })
    }
}
