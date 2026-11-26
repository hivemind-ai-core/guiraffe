//! Immutable render geometry with independent node and edge BVHs.
//! Inspired by graph-core's immutable RenderData hierarchy; edge bounds include
//! every cached sample, including backtracking curves and self loops.
use crate::{
    Edge, EdgeId, EdgeType, FlowError, Graph, LayoutCancellation, LayoutError, NodeId, Point, Rect,
};
use std::collections::BTreeMap;

use std::sync::{Arc, OnceLock};

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
    locations: imbl::OrdMap<u64, u64>,
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
    fn read_only(
        mut entries: Vec<(u64, Rect)>,
        cancel: &LayoutCancellation,
    ) -> Result<Self, LayoutError> {
        let root = if entries.is_empty() {
            None
        } else {
            Some(Self::build_with(
                &mut entries,
                1,
                64,
                &mut || cancel.check(),
                &mut |_, _| {},
            )?)
        };
        Ok(Self {
            root,
            ..Self::default()
        })
    }
    fn build(
        entries: &mut [(u64, Rect)],
        path: u64,
        locations: &mut BTreeMap<u64, u64>,
    ) -> Arc<Branch> {
        match Self::build_with(
            entries,
            path,
            8,
            &mut || Ok::<_, std::convert::Infallible>(()),
            &mut |id, path| {
                locations.insert(id, path);
            },
        ) {
            Ok(root) => root,
            Err(never) => match never {},
        }
    }
    fn build_with<E>(
        entries: &mut [(u64, Rect)],
        path: u64,
        leaf_size: usize,
        check: &mut impl FnMut() -> Result<(), E>,
        locate: &mut impl FnMut(u64, u64),
    ) -> Result<Arc<Branch>, E> {
        check()?;
        let bounds = entries.iter().map(|(_, r)| *r).reduce(Rect::union);
        if entries.len() <= leaf_size {
            for (id, _) in entries.iter() {
                locate(*id, path);
            }
            return Ok(Arc::new(Branch {
                bounds,
                cost: Self::measure(bounds),
                entries: entries.to_vec(),
                children: None,
            }));
        }
        let x = bounds.is_none_or(|b| b.size.x >= b.size.y);
        let mid = entries.len() / 2;
        entries.select_nth_unstable_by(mid, |a, b| {
            let coord = |r: Rect| if x { r.center().x } else { r.center().y };
            coord(a.1).total_cmp(&coord(b.1)).then(a.0.cmp(&b.0))
        });
        let (left, right) = entries.split_at_mut(mid);
        let left = Self::build_with(left, path << 1, leaf_size, check, locate)?;
        let right = Self::build_with(right, (path << 1) | 1, leaf_size, check, locate)?;
        Ok(Arc::new(Branch {
            bounds,
            cost: Self::measure(bounds) + left.cost + right.cost,
            entries: Vec::new(),
            children: Some((left, right)),
        }))
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
        locations: &mut imbl::OrdMap<u64, u64>,
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
const BEZIER_SEGMENTS: usize = 32;
const BEZIER_WEIGHTS: [[f32; 4]; BEZIER_SEGMENTS + 1] = {
    let mut weights = [[0.; 4]; BEZIER_SEGMENTS + 1];
    let mut index = 0;
    while index <= BEZIER_SEGMENTS {
        let t = index as f32 / BEZIER_SEGMENTS as f32;
        let u = 1. - t;
        weights[index] = [u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t];
        index += 1;
    }
    weights
};

pub(crate) fn bezier(from: Point, outward: Point, to: Point, inward: Point) -> Vec<Point> {
    cable_curve(from, outward, to, inward).collect()
}

/// The points of the curve an edge or a cable takes from `from`, leaving along `outward`,
/// to `to`, arriving against `inward` (unit directions, such as [`Side::direction`](crate::Side::direction)).
/// Edges and cables share it, so both bend alike. Allocates nothing.
pub fn cable_curve(
    from: Point,
    outward: Point,
    to: Point,
    inward: Point,
) -> impl ExactSizeIterator<Item = Point> + Clone {
    let distance = from.distance(to).mul_add(0.4, 24.).min(300.);
    let c1 = from + outward * distance;
    let c2 = to + inward * distance;
    BEZIER_WEIGHTS
        .iter()
        .map(move |&[start_weight, c1_weight, c2_weight, end_weight]| {
            from * start_weight + c1 * c1_weight + c2 * c2_weight + to * end_weight
        })
}

fn midpoint(points: &[Point]) -> Point {
    let segments = points.windows(2);
    let count = segments.len();
    let mut inline = [0.; BEZIER_SEGMENTS];
    let mut overflow;
    let lengths = if let Some(lengths) = inline.get_mut(..count) {
        lengths
    } else {
        overflow = vec![0.; count];
        &mut overflow
    };
    let length: f32 = segments
        .clone()
        .zip(lengths.iter_mut())
        .map(|(pair, length)| {
            *length = pair[0].distance(pair[1]);
            *length
        })
        .sum();
    let mut remaining = length * 0.5;
    for (pair, &distance) in segments.zip(lengths.iter()) {
        if distance > 0. && remaining <= distance {
            return pair[0] + (pair[1] - pair[0]) * (remaining / distance);
        }
        remaining -= distance;
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
// Controls extend by at most 300 world units. The remaining two units
// cover the exact route's bounds padding.
const ROUTE_OVERSCAN: f32 = 302.;

#[derive(Debug, Default)]
struct DeferredIndex {
    spatial: OnceLock<(SpatialIndex, SpatialIndex)>,
    incident: OnceLock<Arc<Incidence>>,
}

#[derive(Debug)]
struct Incidence {
    starts: Vec<(NodeId, usize)>,
    edges: Vec<EdgeId>,
}

impl Incidence {
    fn build(graph: &Graph) -> Self {
        let mut pairs = Vec::with_capacity(graph.edges.len().saturating_mul(2));
        for edge in graph.edges.values() {
            pairs.push((edge.source.node, edge.id));
            if edge.source.node != edge.target.node {
                pairs.push((edge.target.node, edge.id));
            }
        }
        pairs.sort_unstable();
        let mut starts = Vec::with_capacity(graph.nodes.len().min(pairs.len()));
        let mut edges = Vec::with_capacity(pairs.len());
        for (node, edge) in pairs {
            if starts.last().is_none_or(|(previous, _)| *previous != node) {
                starts.push((node, edges.len()));
            }
            edges.push(edge);
        }
        Self { starts, edges }
    }

    fn get(&self, node: NodeId) -> &[EdgeId] {
        let Ok(slot) = self.starts.binary_search_by_key(&node, |(id, _)| *id) else {
            return &[];
        };
        let start = self.starts[slot].1;
        let end = self
            .starts
            .get(slot + 1)
            .map_or(self.edges.len(), |pair| pair.1);
        &self.edges[start..end]
    }
}

fn edge_envelope(
    edge: &Edge,
    source: &crate::Node,
    target: &crate::Node,
) -> Result<Rect, FlowError> {
    let source_rect = source.rect();
    let target_rect = target.rect();
    if matches!(
        edge.edge_type,
        EdgeType::OrthogonalHorizontal | EdgeType::OrthogonalVertical
    ) {
        // Orthogonal side channels extend the node envelope by at most eight units.
        return Ok(source_rect.union(target_rect).expand(10.));
    }
    let source_handle = Graph::node_handle(source, &edge.source)?;
    let target_handle = Graph::node_handle(target, &edge.target)?;
    let from = source_handle.position(source_rect);
    let to = target_handle.position(target_rect);
    let endpoint_bounds = Rect::between(from, to);
    if edge.edge_type == EdgeType::Straight {
        return Ok(endpoint_bounds.expand(2.));
    }
    let distance = from.distance(to).mul_add(0.4, 24.).min(300.);
    let control_from = from + source_handle.side.direction() * distance;
    let control_to = to + target_handle.side.direction() * distance;
    let hull = endpoint_bounds.union(Rect::between(control_from, control_to));
    let max = hull.max();
    let roundoff = [hull.origin.x, hull.origin.y, max.x, max.y]
        .into_iter()
        .map(f32::abs)
        .fold(0., f32::max)
        * (f32::EPSILON * 4.);
    Ok(hull.expand(2. + roundoff))
}

#[derive(Clone, Debug)]
pub struct Scene {
    graph: Graph,
    paths: imbl::OrdMap<EdgeId, Arc<EdgePath>>,
    nodes: SpatialIndex,
    edges: SpatialIndex,
    deferred_edges: Option<Arc<DeferredIndex>>,
    bounds: Option<Rect>,
    incident: imbl::OrdMap<NodeId, Vec<EdgeId>>,
    depths: imbl::OrdMap<NodeId, usize>,
    revisions: imbl::OrdMap<NodeId, u64>,
    revision: u64,
    children: imbl::OrdMap<NodeId, Vec<NodeId>>,
}
impl Scene {
    pub fn new(graph: Graph) -> Result<Self, FlowError> {
        Self::with_routes(graph, false)
    }

    /// Prepare exact routes on demand and fill the spatial indexes in the background.
    /// The first frame uses a node preview and edges incident to nearby nodes.
    pub fn new_deferred(graph: Graph) -> Result<Self, FlowError> {
        Self::with_routes(graph, true)
    }

    fn with_routes(graph: Graph, defer_edges: bool) -> Result<Self, FlowError> {
        let node_refs = graph.validated_node_index()?;
        let preview_bounds = if defer_edges { graph.bounds() } else { None };
        // Fallback to exact eager validation for extreme coordinates, where
        // derived route operations can overflow even for valid endpoint rectangles.
        let safe_coordinate = f32::MAX.sqrt() / 8.;
        let deferred_edges = (defer_edges
            && preview_bounds.is_some_and(|bounds| {
                [
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.max().x,
                    bounds.max().y,
                ]
                .iter()
                .all(|value| value.abs() <= safe_coordinate)
            }))
        .then(Arc::<DeferredIndex>::default);
        let paths: imbl::OrdMap<EdgeId, Arc<EdgePath>> = if deferred_edges.is_some() {
            imbl::OrdMap::new()
        } else {
            graph
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
                .collect::<Result<_, _>>()?
        };
        drop(node_refs);
        let nodes = if deferred_edges.is_some() {
            SpatialIndex::default()
        } else {
            SpatialIndex::new(graph.nodes.values().map(|n| (n.id, n.rect())))
        };
        let bounds = if deferred_edges.is_some() {
            preview_bounds
        } else {
            nodes.bounds()
        };
        let edges = SpatialIndex::new(paths.iter().map(|(id, path)| (*id, path.bounds)));
        let mut incident: imbl::OrdMap<NodeId, Vec<EdgeId>> = imbl::OrdMap::new();
        if deferred_edges.is_none() {
            for edge in graph.edges.values() {
                incident.entry(edge.source.node).or_default().push(edge.id);
                if edge.source.node != edge.target.node {
                    incident.entry(edge.target.node).or_default().push(edge.id);
                }
            }
        }
        let mut children: imbl::OrdMap<NodeId, Vec<NodeId>> = imbl::OrdMap::new();
        for n in graph.nodes.values() {
            if let Some(parent) = n.parent {
                children.entry(parent).or_default().push(n.id);
            }
        }
        let depths = if children.is_empty() {
            imbl::OrdMap::new()
        } else {
            Self::depths(&graph)
        };
        Ok(Self {
            depths,
            revisions: imbl::OrdMap::new(),
            revision: 0,
            children,
            graph,
            paths,
            nodes,
            edges,
            deferred_edges,
            bounds,
            incident,
        })
    }
    /// The prepared route table for eager scenes. Deferred scenes use
    /// [Self::edge_route] to read a route without retaining the whole table.
    pub fn paths(&self) -> &imbl::OrdMap<EdgeId, Arc<EdgePath>> {
        &self.paths
    }

    pub fn has_deferred_edges(&self) -> bool {
        self.deferred_edges.is_some()
    }

    pub fn edge_route(&self, id: EdgeId) -> Option<Arc<EdgePath>> {
        if self.has_deferred_edges() {
            let edge = self.graph.edges.get(&id)?;
            EdgePath::new(&self.graph, edge, None, None)
                .ok()
                .map(Arc::new)
        } else {
            self.paths.get(&id).cloned()
        }
    }

    pub fn index_pending(&self) -> bool {
        self.deferred_edges
            .as_ref()
            .is_some_and(|deferred| deferred.spatial.get().is_none())
    }

    /// Install immutable spatial indexes. A cancelled build leaves the
    /// initial local endpoint preview available.
    pub fn prepare_indexes(&self, cancel: &LayoutCancellation) -> Result<bool, LayoutError> {
        let Some(deferred) = self
            .deferred_edges
            .as_ref()
            .filter(|data| data.spatial.get().is_none())
        else {
            return Ok(false);
        };
        cancel.check()?;
        let node_refs: BTreeMap<_, _> = self
            .graph
            .nodes
            .iter()
            .map(|(id, node)| (*id, node))
            .collect();
        let mut entries = Vec::with_capacity(self.graph.edges.len());
        for (offset, edge) in self.graph.edges.values().enumerate() {
            if offset % 256 == 0 {
                cancel.check()?;
            }
            let source = node_refs
                .get(&edge.source.node)
                .ok_or(FlowError::MissingNode(edge.source.node))
                .map_err(LayoutError::Document)?;
            let target = node_refs
                .get(&edge.target.node)
                .ok_or(FlowError::MissingNode(edge.target.node))
                .map_err(LayoutError::Document)?;
            let bounds = edge_envelope(edge, source, target).map_err(LayoutError::Document)?;
            if bounds.valid() {
                entries.push((edge.id, bounds));
            }
        }
        drop(node_refs);
        let edges = SpatialIndex::read_only(entries, cancel)?;
        let nodes = SpatialIndex::read_only(
            self.graph
                .nodes
                .values()
                .map(|n| (n.id, n.rect()))
                .collect(),
            cancel,
        )?;
        cancel.check()?;
        Ok(deferred.spatial.set((nodes, edges)).is_ok())
    }

    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }
    pub fn incident_edges(&self, id: NodeId) -> &[EdgeId] {
        if let Some(deferred) = &self.deferred_edges {
            return deferred
                .incident
                .get_or_init(|| Arc::new(Incidence::build(&self.graph)))
                .get(id);
        }
        self.incident.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Gather incident edges for the supplied nodes without preparing a cold index.
    pub fn append_incident_edges(
        &self,
        ids: impl IntoIterator<Item = NodeId>,
        output: &mut Vec<EdgeId>,
    ) {
        let Some(deferred) = self
            .deferred_edges
            .as_ref()
            .filter(|data| data.incident.get().is_none())
        else {
            for id in ids {
                output.extend(self.incident_edges(id));
            }
            return;
        };
        if let Some((_, edges)) = deferred.spatial.get() {
            let mut candidates = Vec::new();
            for id in ids {
                if let Some(node) = self.graph.nodes.get(&id) {
                    edges.query(node.rect(), &mut candidates);
                    output.extend(candidates.iter().filter(|edge_id| {
                        self.graph
                            .edges
                            .get(*edge_id)
                            .is_some_and(|edge| edge.source.node == id || edge.target.node == id)
                    }));
                }
            }
        } else {
            let affected: std::collections::BTreeSet<_> = ids.into_iter().collect();
            if affected.is_empty() {
                return;
            }
            output.extend(self.graph.edges.values().filter_map(|edge| {
                (affected.contains(&edge.source.node) || affected.contains(&edge.target.node))
                    .then_some(edge.id)
            }));
        }
    }
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
    fn depths(graph: &Graph) -> imbl::OrdMap<NodeId, usize> {
        let mut result = imbl::OrdMap::new();
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
                self.graph.nodes.get(*id).map(|n| n.rect())
                    != graph.nodes.get(*id).map(|n| n.rect())
            })
            .map(|id| (*id, graph.nodes.get(id).map(|n| n.rect())))
            .collect();
        if self.has_deferred_edges() {
            if !node_changes.is_empty() {
                next.bounds = graph.bounds();
            }
        } else {
            next.nodes.update(&node_changes);
            next.bounds = next.nodes.bounds();
        }
        if next.bounds.is_some_and(|b| !b.valid()) {
            return Err(FlowError::InvalidGeometry);
        }
        let mut edges = changed_edges.clone();
        let mut rerouted_nodes = Vec::new();
        for id in changed_nodes {
            let old = self.graph.nodes.get(id);
            let new = graph.nodes.get(id);
            if old.map(|n| (n.pos, n.size, &n.handles)) != new.map(|n| (n.pos, n.size, &n.handles))
            {
                if self.has_deferred_edges() {
                    rerouted_nodes.push(*id);
                } else {
                    edges.extend(self.incident_edges(*id));
                }
            }
            if graph.nodes.contains_key(id) {
                next.revisions.insert(*id, next.revision);
            } else {
                next.revisions.remove(id);
            }
        }
        let mut incident = Vec::new();
        self.append_incident_edges(rerouted_nodes, &mut incident);
        edges.extend(incident);
        let mut edge_changes = BTreeMap::new();
        if let Some(deferred) = &self.deferred_edges
            && (!edges.is_empty() || !node_changes.is_empty())
        {
            let indexes = DeferredIndex::default();
            if changed_edges.is_empty()
                && let Some(incident) = deferred.incident.get()
            {
                let _ = indexes.incident.set(incident.clone());
            }
            next.deferred_edges = Some(Arc::new(indexes));
        }
        for id in edges {
            if let Some(edge) = graph.edges.get(&id) {
                let path = EdgePath::new(&graph, edge, None, None)?;
                if !self.has_deferred_edges() {
                    edge_changes.insert(id, Some(path.bounds));
                    next.paths.insert(id, Arc::new(path));
                }
            } else if !self.has_deferred_edges() {
                next.paths.remove(&id);
                edge_changes.insert(id, None);
            }
        }
        if !edge_changes.is_empty() {
            next.edges.update(&edge_changes);
        }
        if !self.has_deferred_edges() {
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
        self.visible_candidates(area, output);
        if self.has_deferred_edges() {
            output.edges.retain(|id| {
                self.edge_route(*id)
                    .is_some_and(|route| route.bounds.intersects(area))
            });
        }
    }

    /// Collect node IDs and conservative edge candidates without building routes.
    pub fn visible_candidates(&self, area: Rect, output: &mut Visible) {
        output.visited = self.query_node_candidates(area, &mut output.nodes)
            + self.query_edge_candidates(area, &mut output.edges);
    }

    fn query_edge_candidates(&self, area: Rect, output: &mut Vec<EdgeId>) -> usize {
        let Some(deferred) = &self.deferred_edges else {
            return self.edges.query(area, output);
        };
        if let Some((_, edges)) = deferred.spatial.get() {
            return edges.query(area, output);
        }

        output.clear();
        let mut endpoints = Vec::new();
        let visits = self.query_node_candidates(area.expand(ROUTE_OVERSCAN), &mut endpoints);
        if !endpoints.is_empty() {
            self.append_incident_edges(endpoints, output);
            output.sort_unstable();
            output.dedup();
        }
        visits
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
        if !area.valid() || !detail.is_finite() || detail <= 0. {
            return;
        }
        if let Some(nodes) = self.node_index() {
            if let Some(root) = &nodes.root {
                visit(root, area, detail, &mut emit);
            }
        } else {
            self.visit_preview_regions(area, detail, emit);
        }
    }

    fn visit_preview_regions(&self, area: Rect, detail: f32, mut emit: impl FnMut(Rect)) {
        const CELLS: usize = 64;
        let cell = detail
            .max(area.size.x / CELLS as f32)
            .max(area.size.y / CELLS as f32);
        let mut occupied = [[false; CELLS]; CELLS];
        for node in self.graph.nodes.values() {
            let bounds = node.rect();
            if !bounds.intersects(area) {
                continue;
            }
            let min = (bounds.origin - area.origin) * cell.recip();
            let max = (bounds.max() - area.origin) * cell.recip();
            let left = (min.x as usize).min(CELLS - 1);
            let top = (min.y as usize).min(CELLS - 1);
            let right = (max.x.ceil() as usize).clamp(left + 1, CELLS);
            let bottom = (max.y.ceil() as usize).clamp(top + 1, CELLS);
            for row in &mut occupied[top..bottom] {
                row[left..right].fill(true);
            }
        }
        for (row, spans) in occupied.into_iter().enumerate() {
            for (column, covered) in spans.into_iter().enumerate() {
                if covered {
                    emit(Rect {
                        origin: area.origin + Point::new(column as f32 * cell, row as f32 * cell),
                        size: Point::new(cell, cell),
                    });
                }
            }
        }
    }

    fn node_index(&self) -> Option<&SpatialIndex> {
        match &self.deferred_edges {
            Some(deferred) => deferred.spatial.get().map(|(nodes, _)| nodes),
            None => Some(&self.nodes),
        }
    }

    /// Check viewport density without collecting IDs or visiting nodes beyond the limit.
    pub fn node_count_exceeds(&self, area: Rect, limit: usize) -> bool {
        self.node_index().map_or_else(
            || {
                self.graph
                    .nodes
                    .values()
                    .filter(|n| n.rect().intersects(area))
                    .take(limit.saturating_add(1))
                    .count()
                    > limit
            },
            |nodes| nodes.count_exceeds(area, limit),
        )
    }

    fn query_node_candidates(&self, area: Rect, output: &mut Vec<NodeId>) -> usize {
        if let Some(nodes) = self.node_index() {
            return nodes.query(area, output);
        }
        output.clear();
        if !area.valid() {
            return 0;
        }
        output.extend(
            self.graph
                .nodes
                .values()
                .filter(|n| n.rect().intersects(area))
                .map(|n| n.id),
        );
        self.graph.nodes.len()
    }

    pub fn query_nodes(&self, area: Rect, output: &mut Vec<NodeId>) {
        self.query_node_candidates(area, output);
    }
    /// Structural picking only: parents before descendants, ID ties. This immutable
    /// scene has no selection; interactive clients must use `Editor::hit_node`.
    pub fn hit_node(&self, at: Point, scratch: &mut Vec<NodeId>) -> Option<NodeId> {
        self.query_nodes(
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
        self.query_edge_candidates(
            Rect {
                origin: at,
                size: Point::default(),
            }
            .expand(tolerance),
            scratch,
        );
        scratch.iter().rev().copied().find(|id| {
            self.edge_route(*id)
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
    fn precomputed_bezier_weights_keep_the_sample_positions() {
        let from = Point::new(-128., 240.);
        let to = Point::new(384., 100.);
        let outward = Point::new(1., 0.);
        let inward = Point::new(0., 1.);
        let distance = from.distance(to).mul_add(0.4, 24.).min(300.);
        let c1 = from + outward * distance;
        let c2 = to + inward * distance;
        let expected: Vec<_> = (0..=32)
            .map(|index| {
                let t = index as f32 / 32.;
                let u = 1. - t;
                from * (u * u * u)
                    + c1 * (3. * u * u * t)
                    + c2 * (3. * u * t * t)
                    + to * (t * t * t)
            })
            .collect();
        assert_eq!(bezier(from, outward, to, inward), expected);
    }

    #[test]
    fn route_midpoint_uses_distance_and_preserves_long_polylines() {
        let short = [Point::new(0., 0.), Point::new(3., 0.), Point::new(3., 7.)];
        assert_eq!(midpoint(&short), Point::new(3., 2.));
        let long: Vec<_> = (0..=48).map(|index| Point::new(index as f32, 0.)).collect();
        assert_eq!(midpoint(&long), Point::new(24., 0.));
    }

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
