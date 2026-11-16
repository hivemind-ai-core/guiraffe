//! Atomic edits and bounded, structurally shared document history.
use crate::{
    ConnectionPolicy, ConnectionValidator, Edge, EdgeId, Endpoint, FlowError, Graph, Node, NodeId,
    Point, Rect, Scene,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

#[derive(Clone)]
pub struct Editor {
    scene: Arc<Scene>,
    undo: VecDeque<Graph>,
    redo: Vec<Graph>,
    limit: usize,
    selected: Arc<BTreeSet<NodeId>>,
    selected_edges: Arc<BTreeSet<EdgeId>>,
    elevated: Arc<BTreeSet<NodeId>>,
    pub policy: ConnectionPolicy,
}
fn changed_keys<V: Clone + PartialEq>(
    before: &im::OrdMap<u64, V>,
    after: &im::OrdMap<u64, V>,
) -> BTreeSet<u64> {
    before
        .diff(after)
        .map(|d| match d {
            im::ordmap::DiffItem::Add(k, _) | im::ordmap::DiffItem::Remove(k, _) => *k,
            im::ordmap::DiffItem::Update { new: (k, _), .. } => *k,
        })
        .collect()
}
impl Editor {
    pub fn new(mut graph: Graph) -> Result<Self, FlowError> {
        let selected = graph
            .nodes
            .values()
            .filter(|n| n.selected)
            .map(|n| n.id)
            .collect::<BTreeSet<_>>();
        for id in &selected {
            if let Some(n) = graph.nodes.get_mut(id) {
                n.selected = false;
            }
        }
        graph.edge_id_high_water = graph
            .edge_id_high_water
            .max(graph.edges.keys().next_back().copied().unwrap_or(0));
        let mut editor = Self {
            scene: Arc::new(Scene::new(graph)?),
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: 100,
            selected: Arc::new(selected),
            selected_edges: Arc::new(BTreeSet::new()),
            elevated: Arc::new(BTreeSet::new()),
            policy: ConnectionPolicy::default(),
        };
        editor.refresh_order();
        Ok(editor)
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn scene_shared(&self) -> Arc<Scene> {
        self.scene.clone()
    }
    pub fn graph(&self) -> &Graph {
        self.scene.graph()
    }
    pub fn selected_nodes(&self) -> &BTreeSet<NodeId> {
        &self.selected
    }
    pub fn selected_edges(&self) -> &BTreeSet<EdgeId> {
        &self.selected_edges
    }
    /// Immutable view snapshot; pointer identity changes only with selection content.
    pub fn selected_nodes_shared(&self) -> Arc<BTreeSet<NodeId>> {
        self.selected.clone()
    }
    pub fn selected_edges_shared(&self) -> Arc<BTreeSet<EdgeId>> {
        self.selected_edges.clone()
    }
    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selected.contains(&id)
    }
    pub fn history_limit(&mut self, limit: usize) {
        self.limit = limit;
        while self.undo.len() > limit {
            self.undo.pop_front();
        }
        if self.redo.len() > limit {
            self.redo.drain(..self.redo.len() - limit);
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    fn next_scene(&self, mut graph: Graph, validate: bool) -> Result<Option<Scene>, FlowError> {
        graph.edge_id_high_water = graph
            .edge_id_high_water
            .max(self.graph().edge_id_high_water)
            .max(graph.edges.keys().next_back().copied().unwrap_or(0));
        let mut nodes = changed_keys(&self.graph().nodes, &graph.nodes);
        for id in &nodes {
            if let Some(n) = graph.nodes.get_mut(id) {
                n.selected = false;
            }
        }
        nodes = changed_keys(&self.graph().nodes, &graph.nodes);
        let edges = changed_keys(&self.graph().edges, &graph.edges);
        if nodes.is_empty() && edges.is_empty() {
            return Ok(None);
        }
        // Arbitrary caller closures need full structural validation. Moves already preserve
        // IDs, handles, hierarchy and connectivity and validate changed geometry below.
        if validate {
            graph.validate()?;
        }
        self.scene.updated(graph, &nodes, &edges).map(Some)
    }
    fn commit(&mut self, graph: Graph, validate: bool) -> Result<bool, FlowError> {
        let Some(scene) = self.next_scene(graph, validate)? else {
            return Ok(false);
        };
        if self.limit > 0 {
            self.undo.push_back(self.graph().clone());
            if self.undo.len() > self.limit {
                self.undo.pop_front();
            }
        }
        self.scene = Arc::new(scene);
        self.redo.clear();
        self.prune_selection();
        self.refresh_order();
        Ok(true)
    }
    fn prune_selection(&mut self) {
        let graph = self.scene.graph();
        if self.selected.iter().any(|id| !graph.nodes.contains_key(id)) {
            Arc::make_mut(&mut self.selected).retain(|id| graph.nodes.contains_key(id));
        }
        if self
            .selected_edges
            .iter()
            .any(|id| !graph.edges.contains_key(id))
        {
            Arc::make_mut(&mut self.selected_edges).retain(|id| graph.edges.contains_key(id));
        }
    }
    /// Rejected edits leave document, caches, selection and history intact.
    pub fn edit(
        &mut self,
        operation: impl FnOnce(&mut Graph) -> Result<(), FlowError>,
    ) -> Result<bool, FlowError> {
        let mut next = self.graph().clone();
        operation(&mut next)?;
        self.commit(next, true)
    }
    pub fn undo(&mut self) -> Result<bool, FlowError> {
        let Some(graph) = self.undo.back() else {
            return Ok(false);
        };
        let Some(scene) = self.next_scene(graph.clone(), false)? else {
            return Ok(false);
        };
        self.undo.pop_back();
        self.redo.push(self.graph().clone());
        self.scene = Arc::new(scene);
        self.prune_selection();
        self.refresh_order();
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, FlowError> {
        let Some(graph) = self.redo.last() else {
            return Ok(false);
        };
        let Some(scene) = self.next_scene(graph.clone(), false)? else {
            return Ok(false);
        };
        self.redo.pop();
        if self.limit > 0 {
            self.undo.push_back(self.graph().clone());
            if self.undo.len() > self.limit {
                self.undo.pop_front();
            }
        }
        self.scene = Arc::new(scene);
        self.prune_selection();
        self.refresh_order();
        Ok(true)
    }
    pub fn add_node(&mut self, node: Node) -> Result<bool, FlowError> {
        self.edit(|g| {
            if g.nodes.contains_key(&node.id) {
                return Err(FlowError::DuplicateId);
            }
            g.nodes.insert(node.id, node);
            Ok(())
        })
    }
    pub fn connect(
        &mut self,
        edge: Edge,
        validator: Option<&dyn ConnectionValidator>,
    ) -> Result<bool, FlowError> {
        let policy = self.policy;
        self.edit(|g| {
            if g.edges.contains_key(&edge.id) {
                return Err(FlowError::DuplicateId);
            }
            g.validate_connection(&edge.source, &edge.target, policy)?;
            if let Some(v) = validator {
                v.validate(g, &edge.source, &edge.target)?;
            }
            g.edges.insert(edge.id, edge);
            Ok(())
        })
    }
    pub fn connect_endpoints(
        &mut self,
        source: Endpoint,
        target: Endpoint,
        validator: Option<&dyn ConnectionValidator>,
    ) -> Result<bool, FlowError> {
        let id = self
            .graph()
            .edge_id_high_water
            .checked_add(1)
            .ok_or_else(|| FlowError::Rejected("edge ID exhausted".into()))?;
        let mut edge = Edge::new(id, source.node, target.node);
        edge.source = source;
        edge.target = target;
        self.connect(edge, validator)
    }
    pub fn select(&mut self, ids: &BTreeSet<NodeId>, additive: bool) {
        let mut next = if additive {
            self.selected.as_ref().clone()
        } else {
            BTreeSet::new()
        };
        next.extend(
            ids.iter()
                .filter(|id| self.graph().nodes.contains_key(id))
                .copied(),
        );
        if *self.selected != next {
            self.selected = Arc::new(next);
            self.refresh_order();
        }
        if !additive && !self.selected_edges.is_empty() {
            self.selected_edges = Arc::new(BTreeSet::new());
        }
    }
    pub fn toggle(&mut self, id: NodeId) {
        if self.graph().nodes.contains_key(&id) {
            let selection = Arc::make_mut(&mut self.selected);
            if !selection.remove(&id) {
                selection.insert(id);
            }
            self.refresh_order();
        }
    }
    pub fn select_edge(&mut self, id: EdgeId, additive: bool) {
        let mut next = if additive {
            self.selected_edges.as_ref().clone()
        } else {
            BTreeSet::new()
        };
        if self.graph().edges.contains_key(&id) && !next.remove(&id) {
            next.insert(id);
        }
        if *self.selected_edges != next {
            self.selected_edges = Arc::new(next);
        }
        if !additive && !self.selected.is_empty() {
            self.selected = Arc::new(BTreeSet::new());
            self.refresh_order();
        }
    }
    pub fn select_box(&mut self, area: Rect, additive: bool) {
        let mut ids = Vec::new();
        self.scene.query_nodes(area, &mut ids);
        self.select(&ids.into_iter().collect(), additive);
    }
    fn refresh_order(&mut self) {
        let mut elevated = self.selected.as_ref().clone();
        let mut stack: Vec<_> = elevated.iter().copied().collect();
        while let Some(id) = stack.pop() {
            for child in self.scene.children(id) {
                if elevated.insert(*child) {
                    stack.push(*child);
                }
            }
        }
        if *self.elevated != elevated {
            self.elevated = Arc::new(elevated);
        }
    }
    /// Shared paint/hit-test ordering: selected subtrees above unselected, parents before children.
    pub fn z_key(&self, id: NodeId) -> (bool, usize, NodeId) {
        let selected = self.elevated.contains(&id);
        let (depth, id) = self.scene.z_key(id);
        (selected, depth, id)
    }
    pub fn hit_node(&self, at: Point, scratch: &mut Vec<NodeId>) -> Option<NodeId> {
        self.scene.query_nodes(
            Rect {
                origin: at,
                size: Point::default(),
            },
            scratch,
        );
        scratch.iter().copied().max_by_key(|id| self.z_key(*id))
    }
    /// Absolute world coordinates, including children. Only explicit roots snap; descendants
    /// retain their exact offsets, even if they are pinned or also selected.
    pub fn move_positions(
        &self,
        positions: &BTreeMap<NodeId, Point>,
        grid: Option<f32>,
    ) -> Result<BTreeMap<NodeId, Point>, FlowError> {
        let mut result = BTreeMap::new();
        for (id, pos) in positions {
            let n = self
                .graph()
                .nodes
                .get(id)
                .ok_or(FlowError::MissingNode(*id))?;
            if !n.draggable {
                continue;
            }
            let mut parent = n.parent;
            let mut inherited = false;
            while let Some(id) = parent {
                let n = self
                    .graph()
                    .nodes
                    .get(&id)
                    .ok_or(FlowError::MissingNode(id))?;
                if positions.contains_key(&id) && n.draggable {
                    inherited = true;
                    break;
                }
                parent = n.parent;
            }
            if inherited {
                continue;
            }
            let destination = grid.map_or(*pos, |g| pos.snap(g));
            let delta = destination - n.pos;
            result.insert(*id, destination);
            let mut stack = self.scene.children(*id).to_vec();
            while let Some(child) = stack.pop() {
                let node = self
                    .graph()
                    .nodes
                    .get(&child)
                    .ok_or(FlowError::MissingNode(child))?;
                result.insert(child, node.pos + delta);
                stack.extend(self.scene.children(child));
            }
        }
        Ok(result)
    }
    pub fn move_nodes(
        &mut self,
        positions: &BTreeMap<NodeId, Point>,
        grid: Option<f32>,
    ) -> Result<bool, FlowError> {
        let positions = self.move_positions(positions, grid)?;
        self.set_positions(&positions)
    }
    pub(crate) fn set_positions(
        &mut self,
        positions: &BTreeMap<NodeId, Point>,
    ) -> Result<bool, FlowError> {
        let mut graph = self.graph().clone();
        for (id, pos) in positions {
            let n = graph.nodes.get_mut(id).ok_or(FlowError::MissingNode(*id))?;
            n.pos = *pos;
            if !n.rect().valid() {
                return Err(FlowError::InvalidGeometry);
            }
        }
        self.commit(graph, false)
    }
    /// Removing a selected parent also removes descendants and incident edges atomically.
    pub fn delete_selected(&mut self) -> Result<bool, FlowError> {
        if self.selected.is_empty() && self.selected_edges.is_empty() {
            return Ok(false);
        }
        let mut remove = self.selected.as_ref().clone();
        let mut stack: Vec<_> = remove.iter().copied().collect();
        while let Some(id) = stack.pop() {
            for child in self.scene.children(id) {
                if remove.insert(*child) {
                    stack.push(*child);
                }
            }
        }
        let mut graph = self.graph().clone();
        let mut edges = self.selected_edges.as_ref().clone();
        for id in remove {
            edges.extend(self.scene.incident_edges(id));
            graph.nodes.remove(&id);
        }
        for id in edges {
            graph.edges.remove(&id);
        }
        self.commit(graph, false)
    }
}
