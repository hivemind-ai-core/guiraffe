//! A transient preview over a graph (#172): nodes shifted, edges hidden or added, for showing
//! a reorder or a splice while a drag is in progress. It never edits the document, so it never
//! enters undo history.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Edge, EdgeId, FlowError, Graph, NodeId, Point};

/// What a preview changes, relative to the document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphPatch {
    /// World offsets added to these nodes' positions (only the listed nodes; their children
    /// stay put unless listed too). Ids not in the graph are ignored.
    pub offsets: BTreeMap<NodeId, Point>,
    /// Edges not drawn. Ids not in the graph are ignored.
    pub hidden: BTreeSet<EdgeId>,
    /// Edges drawn in addition. An id must not be one of the graph's edges, unless that edge
    /// is hidden: the added edge then takes its place.
    pub added: Vec<Edge>,
}

impl GraphPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.offsets.values().all(|o| *o == Point::default())
            && self.hidden.is_empty()
            && self.added.is_empty()
    }

    /// Shift `node` by `offset`.
    pub fn offset(mut self, node: NodeId, offset: Point) -> Self {
        self.offsets.insert(node, offset);
        self
    }

    /// Hide `edge`.
    pub fn hide_edge(mut self, edge: EdgeId) -> Self {
        self.hidden.insert(edge);
        self
    }

    /// Draw `edge` as well.
    pub fn add_edge(mut self, edge: Edge) -> Self {
        self.added.push(edge);
        self
    }

    /// The graph as previewed. Fails on a non-finite offset, an added edge whose id is taken
    /// (and not hidden) or repeated, or one whose ends are not ports of the graph's nodes.
    pub fn apply(&self, graph: &Graph) -> Result<Graph, FlowError> {
        let mut preview = graph.clone();
        for (id, offset) in &self.offsets {
            if !offset.finite() {
                return Err(FlowError::InvalidGeometry);
            }
            if *offset == Point::default() {
                continue;
            }
            if let Some(node) = preview.nodes.get_mut(id) {
                node.pos = node.pos + *offset;
            }
        }
        for id in &self.hidden {
            preview.edges.remove(id);
        }
        for edge in &self.added {
            if preview.edges.contains_key(&edge.id) {
                return Err(FlowError::DuplicateId);
            }
            preview.edges.insert(edge.id, edge.clone());
        }
        preview.validate()?;
        Ok(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;

    fn graph() -> Graph {
        Graph::from_parts(
            [
                Node::new(1, "A", Point::new(0., 0.)),
                Node::new(2, "B", Point::new(300., 0.)),
                Node::new(3, "C", Point::new(600., 0.)),
            ],
            [Edge::new(1, 1, 2), Edge::new(2, 2, 3)],
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn offsets_hide_and_add_without_touching_the_document() {
        let graph = graph();
        let before = graph.clone();
        let patch = GraphPatch::default()
            .offset(2, Point::new(0., 120.))
            .offset(99, Point::new(5., 5.))
            .hide_edge(1)
            .hide_edge(77)
            .add_edge(Edge::new(10, 1, 3));
        let preview = patch.apply(&graph).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(preview.nodes[&2].pos, Point::new(300., 120.));
        assert_eq!(preview.nodes[&1].pos, Point::new(0., 0.));
        assert!(!preview.edges.contains_key(&1));
        assert!(preview.edges.contains_key(&2));
        assert!(preview.edges.contains_key(&10));
        assert_eq!(graph, before, "the document is untouched");
    }

    #[test]
    fn an_added_edge_may_replace_a_hidden_one() {
        let mut moved = Edge::new(1, 1, 3);
        moved.label = "spliced".into();
        let preview = GraphPatch::default()
            .hide_edge(1)
            .add_edge(moved.clone())
            .apply(&graph())
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(preview.edges[&1], moved);
    }

    #[test]
    fn bad_patches_are_rejected() {
        let taken = GraphPatch::default().add_edge(Edge::new(2, 1, 3));
        assert_eq!(taken.apply(&graph()), Err(FlowError::DuplicateId));
        let twice = GraphPatch::default()
            .add_edge(Edge::new(10, 1, 3))
            .add_edge(Edge::new(10, 1, 2));
        assert_eq!(twice.apply(&graph()), Err(FlowError::DuplicateId));
        let dangling = GraphPatch::default().add_edge(Edge::new(10, 1, 42));
        assert!(dangling.apply(&graph()).is_err());
        let nan = GraphPatch::default().offset(1, Point::new(f32::NAN, 0.));
        assert_eq!(nan.apply(&graph()), Err(FlowError::InvalidGeometry));
    }

    #[test]
    fn emptiness() {
        assert!(GraphPatch::default().is_empty());
        assert!(GraphPatch::default().offset(1, Point::default()).is_empty());
        assert!(!GraphPatch::default().hide_edge(1).is_empty());
    }
}
