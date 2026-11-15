//! Atomic edits and bounded document history. Camera changes do not enter history.
use crate::{
    ConnectionPolicy, ConnectionValidator, Edge, Endpoint, FlowError, Graph, Node, NodeId, Point,
    Rect, Scene,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

pub struct Editor {
    scene: Arc<Scene>,
    undo: VecDeque<Graph>,
    redo: Vec<Graph>,
    limit: usize,
    pub policy: ConnectionPolicy,
}
impl Editor {
    pub fn new(graph: Graph) -> Result<Self, FlowError> {
        Ok(Self {
            scene: Arc::new(Scene::new(graph)?),
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: 100,
            policy: ConnectionPolicy::default(),
        })
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
    pub fn history_limit(&mut self, limit: usize) {
        self.limit = limit;
        while self.undo.len() > limit {
            self.undo.pop_front();
        }
        if limit == 0 {
            self.redo.clear();
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// A rejected transaction leaves document, caches and both history stacks intact.
    pub fn edit(
        &mut self,
        operation: impl FnOnce(&mut Graph) -> Result<(), FlowError>,
    ) -> Result<bool, FlowError> {
        let mut next = self.graph().clone();
        operation(&mut next)?;
        if &next == self.graph() {
            return Ok(false);
        }
        let scene = Scene::new(next)?;
        if self.limit > 0 {
            self.undo.push_back(self.graph().clone());
            if self.undo.len() > self.limit {
                self.undo.pop_front();
            }
        }
        self.scene = Arc::new(scene);
        self.redo.clear();
        Ok(true)
    }
    pub fn undo(&mut self) -> Result<bool, FlowError> {
        let Some(previous) = self.undo.back() else {
            return Ok(false);
        };
        let scene = Scene::new(previous.clone())?;
        self.undo.pop_back();
        self.redo.push(self.graph().clone());
        self.scene = Arc::new(scene);
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, FlowError> {
        let Some(next) = self.redo.last() else {
            return Ok(false);
        };
        let scene = Scene::new(next.clone())?;
        self.redo.pop();
        if self.limit > 0 {
            self.undo.push_back(self.graph().clone());
            if self.undo.len() > self.limit {
                self.undo.pop_front();
            }
        }
        self.scene = Arc::new(scene);
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
            .edges
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| FlowError::Rejected("edge ID exhausted".into()))?;
        let mut edge = Edge::new(id, source.node, target.node);
        edge.source = source;
        edge.target = target;
        self.connect(edge, validator)
    }
    /// Selection is view state: never allocate a history snapshot or invalidate route geometry.
    pub fn select(&mut self, ids: &BTreeSet<NodeId>, additive: bool) {
        for node in Arc::make_mut(&mut self.scene)
            .graph_mut()
            .nodes
            .values_mut()
        {
            node.selected = ids.contains(&node.id) || (additive && node.selected);
        }
    }
    pub fn toggle(&mut self, id: NodeId) {
        if let Some(node) = Arc::make_mut(&mut self.scene)
            .graph_mut()
            .nodes
            .get_mut(&id)
        {
            node.selected = !node.selected;
        }
    }
    pub fn select_box(&mut self, area: Rect, additive: bool) {
        let mut ids = Vec::new();
        self.scene.query_nodes(area, &mut ids);
        self.select(&ids.into_iter().collect(), additive);
    }
    pub fn move_nodes(
        &mut self,
        positions: &BTreeMap<NodeId, Point>,
        grid: Option<f32>,
    ) -> Result<bool, FlowError> {
        self.edit(|g| {
            for (id, pos) in positions {
                let node = g.nodes.get_mut(id).ok_or(FlowError::MissingNode(*id))?;
                if node.draggable {
                    node.pos = grid.map_or(*pos, |s| pos.snap(s));
                }
            }
            Ok(())
        })
    }
    /// Removing parents also removes descendants and all incident edges atomically.
    pub fn delete_selected(&mut self) -> Result<bool, FlowError> {
        self.edit(|g| {
            let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
            for n in g.nodes.values() {
                if let Some(parent) = n.parent {
                    children.entry(parent).or_default().push(n.id);
                }
            }
            let mut stack: Vec<_> = g
                .nodes
                .values()
                .filter(|n| n.selected)
                .map(|n| n.id)
                .collect();
            let mut remove = BTreeSet::new();
            while let Some(id) = stack.pop() {
                if remove.insert(id)
                    && let Some(next) = children.get(&id)
                {
                    stack.extend(next);
                }
            }
            g.nodes.retain(|id, _| !remove.contains(id));
            g.edges.retain(|_, e| {
                !remove.contains(&e.source.node) && !remove.contains(&e.target.node)
            });
            Ok(())
        })
    }
}
