//! Atomic edits and bounded, structurally shared document history.
use crate::{
    AxisLock, ConnectionPolicy, ConnectionValidator, Edge, EdgeId, Endpoint, FlowError, Graph,
    Node, NodeId, Point, Rect, Scene,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

/// How much undo history to keep: a step count (0 disables history) and an optional weight
/// (approximate bytes).
///
/// Mirrors `guiraffe_state::HistoryLimits` (same names) for this GPUI-free crate; every
/// GUIraffe undo stack is configured the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HistoryLimits {
    /// Most undo steps kept.
    pub max_steps: usize,
    /// Most total step weight kept, or no weight limit.
    pub max_weight: Option<u64>,
}

impl HistoryLimits {
    /// History disabled.
    pub const DISABLED: Self = Self::steps(0);

    /// Keep up to `max_steps` steps, with no weight limit.
    pub const fn steps(max_steps: usize) -> Self {
        Self {
            max_steps,
            max_weight: None,
        }
    }

    /// Also cap the total weight of kept steps.
    pub const fn with_max_weight(self, max_weight: u64) -> Self {
        Self {
            max_weight: Some(max_weight),
            ..self
        }
    }
}

impl From<usize> for HistoryLimits {
    fn from(max_steps: usize) -> Self {
        Self::steps(max_steps)
    }
}

/// Weighs a history snapshot once, when it is recorded.
pub type GraphWeigher = fn(&Graph) -> u64;

/// The default weight: one node and edge record per element. Snapshots share structure, so
/// this over-counts unchanged parts; it is a cheap, monotonic proxy for document size.
pub fn estimate_graph_weight(graph: &Graph) -> u64 {
    (graph.nodes.len() * size_of::<Node>() + graph.edges.len() * size_of::<Edge>()) as u64
}

/// Weighted snapshots, newest at the back; trimming drops from the front (the oldest undo
/// step, or the furthest redo step).
#[derive(Clone, Default)]
struct Snapshots {
    entries: VecDeque<(Graph, u64)>,
    weight: u64,
}

impl Snapshots {
    fn push(&mut self, graph: Graph, weight: u64) {
        self.entries.push_back((graph, weight));
        self.weight = self.weight.saturating_add(weight);
    }
    fn pop(&mut self) -> Option<(Graph, u64)> {
        let (g, w) = self.entries.pop_back()?;
        self.weight -= w;
        Some((g, w))
    }
    fn newest(&self) -> Option<&Graph> {
        self.entries.back().map(|(g, _)| g)
    }
    fn clear(&mut self) {
        self.entries.clear();
        self.weight = 0;
    }
    /// Drop the oldest entries over `limits`, keeping the newest one regardless of weight.
    fn trim(&mut self, limits: HistoryLimits) {
        if limits.max_steps == 0 {
            return self.clear();
        }
        while self.entries.len() > 1
            && (self.entries.len() > limits.max_steps
                || limits.max_weight.is_some_and(|m| self.weight > m))
        {
            if let Some((_, w)) = self.entries.pop_front() {
                self.weight -= w;
            }
        }
    }
}

#[derive(Clone)]
pub struct Editor {
    scene: Arc<Scene>,
    undo: Snapshots,
    redo: Snapshots,
    limits: HistoryLimits,
    weigh: GraphWeigher,
    selected: Arc<BTreeSet<NodeId>>,
    selected_edges: Arc<BTreeSet<EdgeId>>,
    elevated: Arc<BTreeSet<NodeId>>,
    pub policy: ConnectionPolicy,
}
pub struct PreparedLayout {
    base: Arc<Scene>,
    scene: Arc<Scene>,
}

impl PreparedLayout {
    pub fn scene(&self) -> Arc<Scene> {
        self.scene.clone()
    }
}

fn changed_keys<V: Clone + PartialEq>(
    before: &imbl::OrdMap<u64, V>,
    after: &imbl::OrdMap<u64, V>,
) -> BTreeSet<u64> {
    before
        .diff(after)
        .map(|d| match d {
            imbl::ordmap::DiffItem::Add(k, _) | imbl::ordmap::DiffItem::Remove(k, _) => *k,
            imbl::ordmap::DiffItem::Update { new: (k, _), .. } => *k,
        })
        .collect()
}
impl Editor {
    pub fn new(graph: Graph) -> Result<Self, FlowError> {
        Self::with_scene(graph, Scene::new)
    }

    /// Build a document that can render local edges before preparing its full edge index.
    pub fn new_deferred(graph: Graph) -> Result<Self, FlowError> {
        Self::with_scene(graph, Scene::new_deferred)
    }

    fn with_scene(
        mut graph: Graph,
        scene: impl FnOnce(Graph) -> Result<Scene, FlowError>,
    ) -> Result<Self, FlowError> {
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
            scene: Arc::new(scene(graph)?),
            undo: Snapshots::default(),
            redo: Snapshots::default(),
            limits: HistoryLimits::steps(100),
            weigh: estimate_graph_weight,
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
    /// Bound undo (and redo) history by step count and, optionally, approximate bytes; the
    /// oldest steps go first, the newest is kept even when heavier than the weight cap.
    /// The default keeps 100 steps; 0 steps disables history.
    pub fn set_undo_limits(&mut self, limits: impl Into<HistoryLimits>) {
        self.limits = limits.into();
        self.undo.trim(self.limits);
        self.redo.trim(self.limits);
    }
    pub fn undo_limits(&self) -> HistoryLimits {
        self.limits
    }
    /// Weigh snapshots with `weigh` instead of [`estimate_graph_weight`].
    pub fn set_undo_weigher(&mut self, weigh: GraphWeigher) {
        self.weigh = weigh;
    }
    /// Undo steps kept, and their total weight.
    pub fn undo_depth(&self) -> (usize, u64) {
        (self.undo.entries.len(), self.undo.weight)
    }
    fn record_undo(&mut self) {
        if self.limits.max_steps > 0 {
            let graph = self.graph().clone();
            let weight = (self.weigh)(&graph);
            self.undo.push(graph, weight);
            self.undo.trim(self.limits);
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.entries.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.entries.is_empty()
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
        self.install_scene(Arc::new(scene));
        Ok(true)
    }
    /// Install a layout prepared on a clone of this editor, retaining current selection.
    pub fn install_prepared_layout(&mut self, prepared: PreparedLayout) -> bool {
        if !Arc::ptr_eq(&self.scene, &prepared.base) || Arc::ptr_eq(&self.scene, &prepared.scene) {
            return false;
        }
        self.install_scene(prepared.scene);
        true
    }
    pub fn prepare_layout(
        &self,
        layout: &crate::LayoutResult,
    ) -> Result<PreparedLayout, FlowError> {
        let mut prepared = self.clone();
        prepared.apply_layout(layout)?;
        Ok(PreparedLayout {
            base: self.scene.clone(),
            scene: prepared.scene,
        })
    }
    fn install_scene(&mut self, scene: Arc<Scene>) {
        self.record_undo();
        self.scene = scene;
        self.redo.clear();
        self.prune_selection();
        self.refresh_order();
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
        let Some(graph) = self.undo.newest() else {
            return Ok(false);
        };
        let Some(scene) = self.next_scene(graph.clone(), false)? else {
            return Ok(false);
        };
        self.undo.pop();
        let current = self.graph().clone();
        let weight = (self.weigh)(&current);
        self.redo.push(current, weight);
        self.scene = Arc::new(scene);
        self.prune_selection();
        self.refresh_order();
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, FlowError> {
        let Some(graph) = self.redo.newest() else {
            return Ok(false);
        };
        let Some(scene) = self.next_scene(graph.clone(), false)? else {
            return Ok(false);
        };
        self.redo.pop();
        self.record_undo();
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
                .filter(|id| self.graph().nodes.contains_key(*id))
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
    /// Sets the selection to exactly these nodes and edges, ignoring ids not in the graph.
    /// Returns whether it changed; the current selection again is a no-op. Selection is not
    /// part of the graph, so it never enters undo history.
    pub fn set_selection(
        &mut self,
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> bool {
        let graph = self.graph();
        let nodes: BTreeSet<NodeId> = nodes
            .into_iter()
            .filter(|id| graph.nodes.contains_key(id))
            .collect();
        let edges: BTreeSet<EdgeId> = edges
            .into_iter()
            .filter(|id| graph.edges.contains_key(id))
            .collect();
        let mut changed = false;
        if *self.selected != nodes {
            self.selected = Arc::new(nodes);
            self.refresh_order();
            changed = true;
        }
        if *self.selected_edges != edges {
            self.selected_edges = Arc::new(edges);
            changed = true;
        }
        changed
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
            // The node's axis lock holds for every caller (pointer, bus, host); the grid snaps
            // only the axes it may move along.
            let locked = lock_point(self.graph().axis_lock_of(*id), n.pos, *pos);
            let destination = grid.map_or(locked, |g| {
                let snapped = locked.snap(g);
                Point::new(
                    if locked.x == n.pos.x {
                        n.pos.x
                    } else {
                        snapped.x
                    },
                    if locked.y == n.pos.y {
                        n.pos.y
                    } else {
                        snapped.y
                    },
                )
            });
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

fn geo(p: Point) -> guiraffe_geometry::Point {
    guiraffe_geometry::Point::new(p.x, p.y)
}

/// [`guiraffe_geometry::apply`] on this crate's points.
pub fn lock_point(lock: AxisLock, from: Point, to: Point) -> Point {
    let p = guiraffe_geometry::apply(lock, geo(from), geo(to));
    Point::new(p.x, p.y)
}

/// The lock for a drag of `delta`: `lock`, or the dominant-axis lock when unlocked and the
/// modifier is held.
pub fn drag_lock(lock: AxisLock, modifier: bool, delta: Point) -> AxisLock {
    lock.with_modifier(modifier, geo(Point::default()), geo(delta))
}
