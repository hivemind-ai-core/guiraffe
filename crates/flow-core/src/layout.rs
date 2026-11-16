//! Deterministic layered layout hook. Cycles are placed in a final fallback layer.
use crate::{Editor, FlowError, Graph, NodeId, Point};
use std::collections::{BTreeMap, BTreeSet};
pub trait Layout {
    fn positions(&self, graph: &Graph) -> BTreeMap<NodeId, Point>;
}
#[derive(Clone, Copy, Debug)]
pub struct LayeredLayout {
    pub gap: Point,
}
impl Default for LayeredLayout {
    fn default() -> Self {
        Self {
            gap: Point::new(80., 40.),
        }
    }
}
impl Layout for LayeredLayout {
    /// Invalid documents, including malformed parent chains, produce no positions.
    fn positions(&self, graph: &Graph) -> BTreeMap<NodeId, Point> {
        if graph.validate().is_err() {
            return BTreeMap::new();
        }
        if graph.nodes.values().any(|n| n.parent.is_some()) {
            let root = |mut id: NodeId| {
                while let Some(parent) = graph.nodes.get(&id).and_then(|n| n.parent) {
                    id = parent;
                }
                id
            };
            let mut groups = BTreeMap::new();
            for n in graph.nodes.values() {
                groups
                    .entry(root(n.id))
                    .and_modify(|r: &mut crate::Rect| *r = r.union(n.rect()))
                    .or_insert(n.rect());
            }
            let mut flat = Graph::default();
            for (id, bounds) in &groups {
                let mut n = crate::Node::new(*id, "", bounds.origin);
                n.pos = bounds.origin;
                n.size = bounds.size;
                flat.nodes.insert(*id, n);
            }
            for e in graph.edges.values() {
                let (source, target) = (root(e.source.node), root(e.target.node));
                if source != target {
                    flat.edges
                        .insert(e.id, crate::Edge::new(e.id, source, target));
                }
            }
            let positions = self.positions(&flat);
            return graph
                .nodes
                .values()
                .filter_map(|n| {
                    let r = root(n.id);
                    Some((n.id, n.pos + *positions.get(&r)? - groups.get(&r)?.origin))
                })
                .collect();
        }
        let mut degree: BTreeMap<_, usize> = graph.nodes.keys().map(|id| (*id, 0)).collect();
        let mut next: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        for edge in graph.edges.values() {
            if let Some(d) = degree.get_mut(&edge.target.node) {
                *d += 1;
            }
            next.entry(edge.source.node)
                .or_default()
                .push(edge.target.node);
        }
        let mut ready: BTreeSet<_> = degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut ranks = BTreeMap::new();
        let mut visited = BTreeSet::new();
        while let Some(id) = ready.pop_first() {
            visited.insert(id);
            let rank = *ranks.entry(id).or_insert(0);
            if let Some(targets) = next.get(&id) {
                for target in targets {
                    let r = ranks.entry(*target).or_insert(0);
                    *r = (*r).max(rank + 1);
                    if let Some(d) = degree.get_mut(target) {
                        *d = d.saturating_sub(1);
                        if *d == 0 {
                            ready.insert(*target);
                        }
                    }
                }
            }
        }
        let fallback = ranks.values().copied().max().unwrap_or(0) + 1;
        let mut layers: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
        for id in graph.nodes.keys() {
            layers
                .entry(if visited.contains(id) {
                    ranks.get(id).copied().unwrap_or(0)
                } else {
                    fallback
                })
                .or_default()
                .push(*id);
        }
        let gap = if self.gap.finite() {
            Point::new(self.gap.x.max(0.), self.gap.y.max(0.))
        } else {
            Point::new(80., 40.)
        };
        let mut positions = BTreeMap::new();
        let mut x = 0.;
        for ids in layers.values() {
            let mut y = 0.;
            let mut width: f32 = 0.;
            for id in ids {
                let Some(n) = graph.nodes.get(id) else {
                    continue;
                };
                positions.insert(*id, Point::new(x, y));
                y += n.size.y + gap.y;
                width = width.max(n.size.x);
            }
            x += width + gap.x;
        }
        positions
    }
}
impl Editor {
    pub fn arrange(&mut self, layout: &dyn Layout) -> Result<bool, FlowError> {
        let positions = layout.positions(self.graph());
        self.set_positions(&positions)
    }
}
