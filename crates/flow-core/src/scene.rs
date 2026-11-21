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
    cost: f64,
}
/// Spatially bulk-built BVH with persistent branches. Updates refit only affected paths.
#[derive(Clone, Debug, Default)]
pub struct SpatialIndex {
    root: Option<Arc<Branch>>,
    locations: im::OrdMap<u64, u64>,
    max_depth: u32,
    baseline_quality: f64,
}
impl SpatialIndex {
    pub fn new(entries: impl IntoIterator<Item = (u64, Rect)>) -> Self {
        let mut entries: Vec<_> = entries.into_iter().filter(|(_, r)| r.valid()).collect();
        let mut index = Self::default();
        let mut locations = BTreeMap::new();
        if !entries.is_empty() {
            index.root = Some(Self::build(&mut entries, 1, &mut locations));
        }
        index.max_depth = locations
            .values()
            .map(|p| 63 - p.leading_zeros())
            .max()
            .unwrap_or(0);
        index.locations = locations.into_iter().collect();
        index.baseline_quality = index.quality();
        index
    }
    fn build(
        entries: &mut [(u64, Rect)],
        path: u64,
        locations: &mut BTreeMap<u64, u64>,
    ) -> Arc<Branch> {
        let bounds = entries.iter().map(|(_, r)| *r).reduce(Rect::union);
        if entries.len() <= 8 {
            for (id, _) in entries.iter() {
                locations.insert(*id, path);
            }
            return Arc::new(Branch {
                bounds,
                cost: Self::measure(bounds),
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
        let left = Self::build(left, path << 1, locations);
        let right = Self::build(right, (path << 1) | 1, locations);
        Arc::new(Branch {
            bounds,
            cost: Self::measure(bounds) + left.cost + right.cost,
            entries: Vec::new(),
            children: Some((left, right)),
        })
    }
    fn measure(bounds: Option<Rect>) -> f64 {
        bounds.map_or(0., |b| f64::from(b.size.x) + f64::from(b.size.y))
    }
    fn quality(&self) -> f64 {
        self.root
            .as_ref()
            .map_or(0., |b| b.cost / Self::measure(b.bounds).max(1.))
    }
    fn rebalance(&mut self) {
        let mut entries = BTreeMap::new();
        if let Some(root) = &self.root {
            Self::collect(root, &mut entries);
        }
        *self = Self::new(entries);
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.root.as_ref().and_then(|r| r.bounds)
    }
    /// Insert, remove or refit entries by copying only affected branches.
    pub fn update(&mut self, changes: &BTreeMap<u64, Option<Rect>>) {
        for (id, rect) in changes {
            if let Some(path) = self.locations.get(id).copied() {
                if let Some(root) = &mut self.root {
                    Self::refit(root, path, 63 - path.leading_zeros(), *id, *rect);
                }
                if rect.is_none() {
                    self.locations.remove(id);
                }
            } else if let Some(rect) = rect.filter(|r| r.valid()) {
                // Bound insertion chains before the packed path can overflow.
                if self.max_depth >= 48 {
                    let mut entries = BTreeMap::new();
                    if let Some(root) = &self.root {
                        Self::collect(root, &mut entries);
                    }
                    entries.insert(*id, rect);
                    *self = Self::new(entries);
                    continue;
                }
                if let Some(root) = &mut self.root {
                    Self::insert(
                        root,
                        (*id, rect),
                        1,
                        &mut self.locations,
                        &mut self.max_depth,
                    );
                } else {
                    *self = Self::new([(*id, rect)]);
                }
            }
        }
        if self.locations.len() > 16 && self.quality() > self.baseline_quality.max(1.) * 1.8 {
            self.rebalance();
        }
    }
    fn collect(branch: &Branch, entries: &mut BTreeMap<u64, Rect>) {
        entries.extend(branch.entries.iter().copied());
        if let Some((left, right)) = &branch.children {
            Self::collect(left, entries);
            Self::collect(right, entries);
        }
    }
    fn insert(
        root: &mut Arc<Branch>,
        entry: (u64, Rect),
        path: u64,
        locations: &mut im::OrdMap<u64, u64>,
        max_depth: &mut u32,
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
            Self::insert(
                if to_right { right } else { left },
                entry,
                (path << 1) | u64::from(to_right),
                locations,
                max_depth,
            );
            branch.bounds = match (left.bounds, right.bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            };
        } else {
            branch.entries.push(entry);
            if branch.entries.len() > 8 {
                let mut split_locations = BTreeMap::new();
                *root = Self::build(&mut branch.entries, path, &mut split_locations);
                for (id, path) in split_locations {
                    *max_depth = (*max_depth).max(63 - path.leading_zeros());
                    locations.insert(id, path);
                }
            } else {
                *max_depth = (*max_depth).max(63 - path.leading_zeros());
                locations.insert(entry.0, path);
                branch.bounds = branch.entries.iter().map(|(_, r)| *r).reduce(Rect::union);
            }
        }
        let branch = Arc::make_mut(root);
        branch.cost = Self::measure(branch.bounds)
            + branch
                .children
                .as_ref()
                .map_or(0., |(l, r)| l.cost + r.cost);
    }
    fn refit(root: &mut Arc<Branch>, path: u64, depth: u32, id: u64, rect: Option<Rect>) {
        let branch = Arc::make_mut(root);
        if let Some((l, r)) = &mut branch.children {
            if depth > 0 {
                Self::refit(
                    if path & (1 << (depth - 1)) != 0 { r } else { l },
                    path,
                    depth - 1,
                    id,
                    rect,
                );
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
        branch.cost = Self::measure(branch.bounds)
            + branch
                .children
                .as_ref()
                .map_or(0., |(l, r)| l.cost + r.cost);
    }
    fn count_exceeds(&self, area: Rect, mut limit: usize) -> bool {
        fn visit(branch: &Branch, area: Rect, remaining: &mut usize) -> bool {
            if branch.bounds.is_none_or(|bounds| !bounds.intersects(area)) {
                return false;
            }
            if let Some((left, right)) = &branch.children {
                return visit(left, area, remaining) || visit(right, area, remaining);
            }
            for (_, bounds) in &branch.entries {
                if bounds.intersects(area) {
                    if *remaining == 0 {
                        return true;
                    }
                    *remaining -= 1;
                }
            }
            false
        }

        self.root
            .as_ref()
            .is_some_and(|root| visit(root, area, &mut limit))
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
        let source = graph
            .nodes
            .get(&edge.source.node)
            .ok_or(FlowError::MissingNode(edge.source.node))?;
        let target = graph
            .nodes
            .get(&edge.target.node)
            .ok_or(FlowError::MissingNode(edge.target.node))?;
        Self::from_nodes(edge, source, target, source_override, target_override)
    }
    fn from_nodes(
        edge: &Edge,
        source: &crate::Node,
        target: &crate::Node,
        source_override: Option<Point>,
        target_override: Option<Point>,
    ) -> Result<Self, FlowError> {
        let sh = Graph::node_handle(source, &edge.source)?;
        let th = Graph::node_handle(target, &edge.target)?;
        let source = Rect {
            origin: source_override.unwrap_or(source.pos),
            size: source.size,
        };
        let target = Rect {
            origin: target_override.unwrap_or(target.pos),
            size: target.size,
        };
        let from = sh.position(source);
        let to = th.position(target);
        let points = match edge.edge_type {
            EdgeType::OrthogonalHorizontal | EdgeType::OrthogonalVertical => {
                crate::auto_layout::orthogonal(
                    (source, sh),
                    (target, th),
                    edge.edge_type == EdgeType::OrthogonalHorizontal,
                )
            }
            EdgeType::Straight => vec![from, to],
            EdgeType::Bezier => bezier(from, sh.side.direction(), to, th.side.direction()),
            EdgeType::Step | EdgeType::SmoothStep => {
                let distance = from.distance(to).mul_add(0.4, 24.).min(300.);
                let c1 = from + sh.side.direction() * distance;
                let c2 = to + th.side.direction() * distance;
                let horizontal = sh.side.direction().x != 0.;
                let (a, b) = if horizontal {
                    let x = (c1.x + c2.x) * 0.5;
                    (Point::new(x, from.y), Point::new(x, to.y))
                } else {
                    let y = (c1.y + c2.y) * 0.5;
                    (Point::new(from.x, y), Point::new(to.x, y))
                };
                // Facing handles with room between them need only the two bends.
                // The Bezier control distance can exceed half the gap: inserting
                // those controls into a step route creates visible reverse stubs.
                let direction = sh.side.direction();
                let facing = direction + th.side.direction() == Point::default();
                let gap = to - from;
                let forward = gap.x * direction.x + gap.y * direction.y >= 0.;
                let mut corners = if facing && forward {
                    vec![from, a, b, to]
                } else {
                    vec![from, c1, a, b, c2, to]
                };
                corners.dedup();
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
            .iter()
            .zip(self.points.iter().skip(1))
            .map(|(a, b)| segment_distance(point, *a, *b))
            .fold(f32::INFINITY, f32::min)
    }
}
fn rounded(corners: &[Point], radius: f32) -> Vec<Point> {
    let mut points = corners.first().copied().into_iter().collect::<Vec<_>>();
    for (left, (center, right)) in corners
        .iter()
        .zip(corners.iter().skip(1).zip(corners.iter().skip(2)))
    {
        let center = *center;
        let before = *left - center;
        let after = *right - center;
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
pub(crate) fn bezier(from: Point, outward: Point, to: Point, inward: Point) -> Vec<Point> {
    let distance = from.distance(to).mul_add(0.4, 24.).min(300.);
    let c1 = from + outward * distance;
    let c2 = to + inward * distance;
    (0..=32)
        .map(|i| {
            let t = i as f32 / 32.;
            let u = 1. - t;
            from * (u * u * u) + c1 * (3. * u * u * t) + c2 * (3. * u * t * t) + to * (t * t * t)
        })
        .collect()
}

fn midpoint(points: &[Point]) -> Point {
    let length: f32 = points
        .iter()
        .zip(points.iter().skip(1))
        .map(|(a, b)| a.distance(*b))
        .sum();
    let mut remaining = length * 0.5;
    for (a, b) in points.iter().zip(points.iter().skip(1)) {
        let d = a.distance(*b);
        if d > 0. && remaining <= d {
            return *a + (*b - *a) * (remaining / d);
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
        let node_refs = graph.validated_node_index()?;
        let paths: im::OrdMap<EdgeId, Arc<EdgePath>> = graph
            .edges
            .values()
            .map(|e| {
                EdgePath::from_nodes(
                    e,
                    node_refs
                        .get(&e.source.node)
                        .ok_or(FlowError::MissingNode(e.source.node))?,
                    node_refs
                        .get(&e.target.node)
                        .ok_or(FlowError::MissingNode(e.target.node))?,
                    None,
                    None,
                )
                .map(|p| (e.id, Arc::new(p)))
            })
            .collect::<Result<_, _>>()?;
        drop(node_refs);
        let nodes = SpatialIndex::new(graph.nodes.values().map(|n| (n.id, n.rect())));
        let edges = SpatialIndex::new(paths.iter().map(|(id, path)| (*id, path.bounds)));
        let bounds = nodes.bounds();
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
        let depths = if children.is_empty() {
            im::OrdMap::new()
        } else {
            Self::depths(&graph)
        };
        Ok(Self {
            depths,
            revisions: im::OrdMap::new(),
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
                next = graph.nodes.get(&id).and_then(|n| n.parent);
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
            .filter(|id| {
                self.graph.nodes.get(id).map(|n| n.rect()) != graph.nodes.get(id).map(|n| n.rect())
            })
            .map(|id| (*id, graph.nodes.get(id).map(|n| n.rect())))
            .collect();
        next.nodes.update(&node_changes);
        next.bounds = next.nodes.bounds();
        if next.bounds.is_some_and(|b| !b.valid()) {
            return Err(FlowError::InvalidGeometry);
        }
        let mut edges = changed_edges.clone();
        for id in changed_nodes {
            let old = self.graph.nodes.get(id);
            let new = graph.nodes.get(id);
            if old.map(|n| (n.pos, n.size, &n.handles)) != new.map(|n| (n.pos, n.size, &n.handles))
            {
                edges.extend(self.incident_edges(*id));
            }
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
        for id in changed_nodes {
            if self.graph.nodes.get(id).and_then(|n| n.parent)
                == graph.nodes.get(id).and_then(|n| n.parent)
            {
                continue;
            }
            if let Some(parent) = self.graph.nodes.get(id).and_then(|n| n.parent)
                && let Some(ids) = next.children.get_mut(&parent)
            {
                ids.retain(|i| i != id);
            }
            if let Some(parent) = graph.nodes.get(id).and_then(|n| n.parent) {
                next.children.entry(parent).or_default().push(*id);
            }
        }
        let mut affected = std::collections::BTreeSet::new();
        for id in changed_nodes {
            if !graph.nodes.contains_key(id) {
                next.depths.remove(id);
                continue;
            }
            if self.graph.nodes.get(id).map(|n| n.parent) == graph.nodes.get(id).map(|n| n.parent) {
                continue;
            }
            let mut stack = vec![*id];
            while let Some(id) = stack.pop() {
                if affected.insert(id) {
                    stack.extend(next.children(id));
                }
            }
        }
        for id in affected {
            let mut depth = 0;
            let mut parent = graph.nodes.get(&id).and_then(|n| n.parent);
            while let Some(id) = parent {
                depth += 1;
                parent = graph.nodes.get(&id).and_then(|n| n.parent);
            }
            next.depths.insert(id, depth);
        }
        next.graph = graph;
        Ok(next)
    }
    pub fn visible(&self, area: Rect, output: &mut Visible) {
        output.visited =
            self.nodes.query(area, &mut output.nodes) + self.edges.query(area, &mut output.edges);
    }
    /// Visit a coarse coverage map for zoomed-out drawing. Small BVH branches
    /// contribute one region and leaf entries retain their individual bounds.
    pub fn visit_node_regions(&self, area: Rect, detail: f32, mut emit: impl FnMut(Rect)) {
        fn visit(branch: &Branch, area: Rect, detail: f32, emit: &mut impl FnMut(Rect)) {
            let Some(bounds) = branch.bounds.filter(|bounds| bounds.intersects(area)) else {
                return;
            };
            if bounds.size.x.max(bounds.size.y) <= detail {
                emit(bounds);
            } else if let Some((left, right)) = &branch.children {
                visit(left, area, detail, emit);
                visit(right, area, detail, emit);
            } else {
                for (_, rect) in &branch.entries {
                    if rect.intersects(area) {
                        emit(*rect);
                    }
                }
            }
        }
        if let Some(root) = self.nodes.root.as_ref().filter(|_| area.valid()) {
            visit(root, area, detail, &mut emit);
        }
    }
    /// Check viewport density without collecting IDs or visiting nodes beyond the limit.
    pub fn node_count_exceeds(&self, area: Rect, limit: usize) -> bool {
        self.nodes.count_exceeds(area, limit)
    }

    pub fn query_nodes(&self, area: Rect, output: &mut Vec<NodeId>) {
        self.nodes.query(area, output);
    }
    /// Structural picking only: parents before descendants, ID ties. This immutable
    /// scene has no selection; interactive clients must use `Editor::hit_node`.
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

#[cfg(test)]
mod review_2_tests {
    use super::*;
    #[test]
    fn review2_5_delete_child_keeps_unrelated_depth_storage() {
        let mut graph = Graph::from_parts(
            (0..256).map(|id| crate::Node::new(id, "", Point::new(id as f32 * 200., 0.))),
            [],
        )
        .expect("graph");
        graph.nodes.get_mut(&1).expect("child").parent = Some(0);
        let mut editor = crate::Editor::new(graph).expect("editor");
        let before = editor.scene_shared();
        editor.select(&std::collections::BTreeSet::from([1]), false);
        editor.delete_selected().expect("delete");
        assert_eq!(
            before.depths.get(&200).map(|d| d as *const usize),
            editor.scene().depths.get(&200).map(|d| d as *const usize)
        );
    }
}

#[cfg(test)]
mod feedback_tests {
    use super::*;
    #[test]
    fn forward_step_edges_do_not_backtrack_at_corners() {
        for kind in [EdgeType::Step, EdgeType::SmoothStep] {
            let edge = Edge {
                edge_type: kind,
                ..Edge::new(1, 1, 2)
            };
            let graph = Graph::from_parts(
                [
                    crate::Node::new(1, "", Point::new(240., 240.)),
                    crate::Node::new(2, "", Point::new(500., 300.)),
                ],
                [edge.clone()],
            )
            .expect("graph");
            let path = EdgePath::new(&graph, &edge, None, None).expect("path");
            for pair in path.points.windows(2) {
                assert!(
                    pair[1].x >= pair[0].x,
                    "forward route backtracks: {:?}",
                    path.points
                );
                assert!(
                    pair[1].y >= pair[0].y,
                    "forward route doubles back vertically"
                );
            }
        }
    }
}
