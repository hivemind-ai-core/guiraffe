//! Deterministic, cancellable graph layout in world coordinates.
mod layered;
mod placement;

use crate::{EdgeId, EdgeType, Editor, FlowError, Graph, NodeId, Point, Rect};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Default, Debug)]
pub struct LayoutCancellation(Arc<AtomicBool>);

impl LayoutCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn check(&self) -> Result<(), LayoutError> {
        if self.0.load(Ordering::Relaxed) {
            return Err(LayoutError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    Cancelled,
    Document(FlowError),
    InvalidOptions,
    Infeasible,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LayoutError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    #[default]
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl LayoutDirection {
    fn horizontal(self) -> bool {
        matches!(self, Self::LeftToRight | Self::RightToLeft)
    }

    fn reversed(self) -> bool {
        matches!(self, Self::RightToLeft | Self::BottomToTop)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutConstraint {
    LeftOf(NodeId, NodeId),
    Above(NodeId, NodeId),
    AlignX(NodeId, NodeId),
    AlignY(NodeId, NodeId),
    SameRank(NodeId, NodeId),
}

#[derive(Clone, Debug)]
pub struct LayoutOptions {
    pub direction: LayoutDirection,
    pub spacing: Point,
    pub pinned: BTreeSet<NodeId>,
    pub constraints: Vec<LayoutConstraint>,
    /// Lower values sort first within a layer or packing sequence.
    pub order: BTreeMap<NodeId, i32>,
    /// Controls the number of crossing-reduction sweeps. Zero preserves initial order.
    pub crossing_weight: f32,
    /// When provided, groups outside this set retain their current positions.
    pub affected: Option<BTreeSet<NodeId>>,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::default(),
            spacing: Point::new(80., 40.),
            pinned: BTreeSet::new(),
            constraints: Vec::new(),
            order: BTreeMap::new(),
            crossing_weight: 1.,
            affected: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutResult {
    pub positions: BTreeMap<NodeId, Point>,
    pub routes: BTreeMap<EdgeId, Vec<Point>>,
    pub direction: LayoutDirection,
}

pub trait LayoutEngine: Send + Sync {
    fn layout(
        &self,
        graph: &Graph,
        options: &LayoutOptions,
        cancel: &LayoutCancellation,
    ) -> Result<LayoutResult, LayoutError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub enum AutoLayout {
    #[default]
    Layered,
    Tree,
    Radial,
    Grid {
        columns: usize,
    },
    Force {
        seed: u64,
        iterations: usize,
    },
}

struct Unit {
    bounds: Rect,
    fixed: bool,
    order: i32,
}

struct Link {
    source: usize,
    target: usize,
    source_port: f32,
    target_port: f32,
}

impl LayoutEngine for AutoLayout {
    fn layout(
        &self,
        graph: &Graph,
        options: &LayoutOptions,
        cancel: &LayoutCancellation,
    ) -> Result<LayoutResult, LayoutError> {
        cancel.check()?;
        graph.validate().map_err(LayoutError::Document)?;
        if !options.spacing.finite()
            || options.spacing.x < 0.
            || options.spacing.y < 0.
            || !options.crossing_weight.is_finite()
            || options.crossing_weight < 0.
        {
            return Err(LayoutError::InvalidOptions);
        }

        let mut roots = BTreeMap::new();
        for id in graph.nodes.keys().copied() {
            cancel.check()?;
            let mut root = id;
            let mut chain = Vec::new();
            while let Some(parent) = graph.nodes[&root].parent {
                if let Some(cached) = roots.get(&root) {
                    root = *cached;
                    break;
                }
                chain.push(root);
                root = parent;
            }
            roots.insert(id, root);
            for member in chain {
                roots.insert(member, root);
            }
        }

        let mut units = Vec::new();
        let mut indices = BTreeMap::new();
        for node in graph.nodes.values() {
            let root = roots[&node.id];
            let index = *indices.entry(root).or_insert_with(|| {
                let index = units.len();
                units.push(Unit {
                    bounds: node.rect(),
                    fixed: true,
                    order: options.order.get(&root).copied().unwrap_or_default(),
                });
                index
            });
            units[index].bounds = units[index].bounds.union(node.rect());
            // A group moves as soon as any member belongs to the affected set.
            units[index].fixed &= options
                .affected
                .as_ref()
                .is_some_and(|affected| !affected.contains(&node.id));
        }
        for id in &options.pinned {
            let root = roots
                .get(id)
                .ok_or(LayoutError::Document(FlowError::MissingNode(*id)))?;
            units[indices[root]].fixed = true;
        }

        let mut links = Vec::new();
        for edge in graph.edges.values() {
            cancel.check()?;
            let source = indices[&roots[&edge.source.node]];
            let target = indices[&roots[&edge.target.node]];
            if source == target {
                continue;
            }
            let port = |endpoint| {
                let (node, handle) = graph.handle(endpoint).map_err(LayoutError::Document)?;
                Ok(handle.position(node.rect()))
            };
            let cross = |p: Point| {
                if options.direction.horizontal() {
                    p.y
                } else {
                    p.x
                }
            };
            links.push(Link {
                source,
                target,
                source_port: cross(port(&edge.source)? - units[source].bounds.origin),
                target_port: cross(port(&edge.target)? - units[target].bounds.origin),
            });
        }

        let mut positions = match *self {
            Self::Layered | Self::Tree | Self::Radial => {
                layered::positions(&units, &links, options, *self, cancel)?
            }
            Self::Grid { columns } => placement::grid(&units, columns, options.spacing),
            Self::Force { seed, iterations } => {
                placement::force(&units, &links, options, seed, iterations, cancel)?
            }
        };
        let index = |id| {
            roots
                .get(&id)
                .map(|root| indices[root])
                .ok_or(LayoutError::Document(FlowError::MissingNode(id)))
        };
        let mut constraints = Vec::new();
        for &constraint in &options.constraints {
            use LayoutConstraint::*;
            let (a, b, axis, aligned) = match constraint {
                LeftOf(a, b) => (a, b, 0, false),
                Above(a, b) => (a, b, 1, false),
                AlignX(a, b) => (a, b, 0, true),
                AlignY(a, b) => (a, b, 1, true),
                SameRank(a, b) => (a, b, usize::from(!options.direction.horizontal()), true),
            };
            constraints.push((
                index(a)?,
                index(b)?,
                axis,
                aligned,
                graph.nodes[&a].pos - units[index(a)?].bounds.origin,
                graph.nodes[&b].pos - units[index(b)?].bounds.origin,
                graph.nodes[&a].size,
            ));
        }
        placement::resolve(
            &units,
            &mut positions,
            &constraints,
            options.spacing,
            cancel,
        )?;

        let mut result = LayoutResult {
            direction: options.direction,
            ..LayoutResult::default()
        };
        for node in graph.nodes.values() {
            cancel.check()?;
            let unit = indices[&roots[&node.id]];
            let pos = node.pos + (positions[unit] - units[unit].bounds.origin);
            if !(Rect {
                origin: pos,
                size: node.size,
            })
            .valid()
            {
                return Err(LayoutError::Infeasible);
            }
            result.positions.insert(node.id, pos);
        }
        for edge in graph.edges.values() {
            cancel.check()?;
            let endpoint = |end| {
                let (node, handle) = graph.handle(end).map_err(LayoutError::Document)?;
                let point = handle.position(Rect {
                    origin: result.positions[&node.id],
                    size: node.size,
                });
                Ok((point, handle.side.direction()))
            };
            let (start, outward) = endpoint(&edge.source)?;
            let (end, inward) = endpoint(&edge.target)?;
            let route = orthogonal(start, outward, end, inward, options.direction.horizontal());
            if route.iter().any(|p| !p.finite()) {
                return Err(LayoutError::Infeasible);
            }
            result.routes.insert(edge.id, route);
        }
        Ok(result)
    }
}

impl Editor {
    pub fn apply_layout(&mut self, result: &LayoutResult) -> Result<bool, FlowError> {
        self.edit(|graph| {
            for (&id, &position) in &result.positions {
                graph
                    .nodes
                    .get_mut(&id)
                    .ok_or(FlowError::MissingNode(id))?
                    .pos = position;
            }
            let kind = if result.direction.horizontal() {
                EdgeType::OrthogonalHorizontal
            } else {
                EdgeType::OrthogonalVertical
            };
            for id in result.routes.keys() {
                if let Some(edge) = graph.edges.get_mut(id) {
                    edge.edge_type = kind;
                }
            }
            Ok(())
        })
    }
}

pub(crate) fn orthogonal(
    start: Point,
    outward: Point,
    end: Point,
    inward: Point,
    horizontal: bool,
) -> Vec<Point> {
    let stub = (start.distance(end) * 0.125).min(8.);
    let a = start + outward * stub;
    let b = end + inward * stub;
    let middle = if horizontal {
        let x = a.x * 0.5 + b.x * 0.5;
        [Point::new(x, a.y), Point::new(x, b.y)]
    } else {
        let y = a.y * 0.5 + b.y * 0.5;
        [Point::new(a.x, y), Point::new(b.x, y)]
    };
    let mut points = vec![start, a, middle[0], middle[1], b, end];
    points.dedup();
    points
}
