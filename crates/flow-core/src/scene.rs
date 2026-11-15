//! Immutable render geometry with independent node and edge BVHs.
//! Inspired by graph-core's immutable RenderData hierarchy; edge bounds include
//! every cached sample, including backtracking curves and self loops.
use crate::{Edge, EdgeId, EdgeType, FlowError, Graph, NodeId, Point, Rect};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Branch {
    bounds: Rect,
    range: std::ops::Range<usize>,
    children: Option<(usize, usize)>,
}
/// Bulk-built bounding volume hierarchy. Queries reuse caller-owned output buffers.
#[derive(Clone, Debug, Default)]
pub struct SpatialIndex {
    entries: Vec<(u64, Rect)>,
    branches: Vec<Branch>,
}
impl SpatialIndex {
    pub fn new(entries: impl IntoIterator<Item = (u64, Rect)>) -> Self {
        let mut index = Self {
            entries: entries.into_iter().collect(),
            branches: Vec::new(),
        };
        index.entries.retain(|(_, r)| r.valid());
        if !index.entries.is_empty() {
            index.build(0, index.entries.len());
        }
        index
    }
    fn build(&mut self, start: usize, end: usize) -> usize {
        let bounds = self.entries[start..end]
            .iter()
            .map(|(_, r)| *r)
            .reduce(Rect::union)
            .unwrap_or_default();
        let branch = self.branches.len();
        self.branches.push(Branch {
            bounds,
            range: start..end,
            children: None,
        });
        if end - start > 8 {
            let mid = (end - start) / 2;
            let x = bounds.size.x >= bounds.size.y;
            self.entries[start..end].select_nth_unstable_by(mid, |a, b| {
                let coord = |r: Rect| if x { r.center().x } else { r.center().y };
                coord(a.1).total_cmp(&coord(b.1)).then(a.0.cmp(&b.0))
            });
            let left = self.build(start, start + mid);
            let right = self.build(start + mid, end);
            self.branches[branch].children = Some((left, right));
        }
        branch
    }
    /// Returns the number of branches visited; output order is deterministic by ID.
    pub fn query(&self, area: Rect, output: &mut Vec<u64>) -> usize {
        output.clear();
        if self.branches.is_empty() || !area.valid() {
            return 0;
        }
        let visited = self.visit(0, area, output);
        output.sort_unstable();
        visited
    }
    fn visit(&self, index: usize, area: Rect, output: &mut Vec<u64>) -> usize {
        let branch = &self.branches[index];
        if !branch.bounds.intersects(area) {
            return 1;
        }
        if let Some((left, right)) = branch.children {
            1 + self.visit(left, area, output) + self.visit(right, area, output)
        } else {
            output.extend(
                self.entries[branch.range.clone()]
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
        let bounds = points
            .iter()
            .map(|p| Rect {
                origin: *p,
                size: Point::default(),
            })
            .reduce(Rect::union)
            .unwrap_or_default()
            .expand(2.);
        let label = midpoint(&points);
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
    pub paths: BTreeMap<EdgeId, EdgePath>,
    nodes: SpatialIndex,
    edges: SpatialIndex,
    pub bounds: Option<Rect>,
    incident: BTreeMap<NodeId, Vec<EdgeId>>,
}
impl Scene {
    pub fn new(graph: Graph) -> Result<Self, FlowError> {
        graph.validate()?;
        let paths: BTreeMap<_, _> = graph
            .edges
            .values()
            .map(|e| EdgePath::new(&graph, e, None, None).map(|p| (e.id, p)))
            .collect::<Result<_, _>>()?;
        let nodes = SpatialIndex::new(graph.nodes.values().map(|n| (n.id, n.rect())));
        let edges = SpatialIndex::new(paths.iter().map(|(id, path)| (*id, path.bounds)));
        let bounds = graph.bounds();
        let mut incident: BTreeMap<NodeId, Vec<EdgeId>> = BTreeMap::new();
        for edge in graph.edges.values() {
            incident.entry(edge.source.node).or_default().push(edge.id);
            if edge.source.node != edge.target.node {
                incident.entry(edge.target.node).or_default().push(edge.id);
            }
        }
        Ok(Self {
            graph,
            paths,
            nodes,
            edges,
            bounds,
            incident,
        })
    }
    pub fn incident_edges(&self, id: NodeId) -> &[EdgeId] {
        self.incident.get(&id).map_or(&[], Vec::as_slice)
    }
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
    pub(crate) fn graph_mut(&mut self) -> &mut Graph {
        &mut self.graph
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
        scratch.last().copied()
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
