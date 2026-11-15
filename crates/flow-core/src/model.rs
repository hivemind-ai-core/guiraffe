//! Serializable document. Validate untrusted/deserialized documents before use.
use crate::{Point, Rect};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type NodeId = u64;
pub type EdgeId = u64;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    #[default]
    Right,
    Top,
    Bottom,
}
impl Side {
    pub fn direction(self) -> Point {
        match self {
            Self::Left => Point::new(-1., 0.),
            Self::Right => Point::new(1., 0.),
            Self::Top => Point::new(0., -1.),
            Self::Bottom => Point::new(0., 1.),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandleKind {
    Source,
    Target,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handle {
    pub id: String,
    pub kind: HandleKind,
    pub side: Side,
    pub offset: f32,
}
impl Handle {
    pub fn new(id: impl Into<String>, kind: HandleKind, side: Side) -> Self {
        Self {
            id: id.into(),
            kind,
            side,
            offset: 0.5,
        }
    }
    pub fn position(&self, rect: Rect) -> Point {
        match self.side {
            Side::Left => rect.origin + Point::new(0., rect.size.y * self.offset),
            Side::Right => rect.origin + Point::new(rect.size.x, rect.size.y * self.offset),
            Side::Top => rect.origin + Point::new(rect.size.x * self.offset, 0.),
            Side::Bottom => rect.origin + Point::new(rect.size.x * self.offset, rect.size.y),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub pos: Point,
    pub size: Point,
    pub data: serde_json::Value,
    pub node_type: String,
    pub parent: Option<NodeId>,
    pub selected: bool,
    pub draggable: bool,
    pub handles: Vec<Handle>,
}
impl Node {
    pub fn new(id: NodeId, label: impl Into<String>, pos: Point) -> Self {
        Self {
            id,
            pos,
            size: Point::new(160., 72.),
            data: serde_json::Value::String(label.into()),
            node_type: "default".into(),
            parent: None,
            selected: false,
            draggable: true,
            handles: vec![
                Handle::new("in", HandleKind::Target, Side::Left),
                Handle::new("out", HandleKind::Source, Side::Right),
            ],
        }
    }
    /// Coordinates are absolute world coordinates, including for children.
    pub fn rect(&self) -> Rect {
        Rect {
            origin: self.pos,
            size: self.size,
        }
    }
    pub fn label(&self) -> &str {
        self.data.as_str().unwrap_or(&self.node_type)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    #[default]
    Bezier,
    SmoothStep,
    Step,
    Straight,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Marker {
    #[default]
    None,
    Arrow,
    ArrowClosed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub node: NodeId,
    pub handle: String,
}
impl Endpoint {
    pub fn new(node: NodeId, handle: impl Into<String>) -> Self {
        Self {
            node,
            handle: handle.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub source: Endpoint,
    pub target: Endpoint,
    pub edge_type: EdgeType,
    pub label: String,
    pub animated: bool,
    pub marker_start: Marker,
    pub marker_end: Marker,
}
impl Edge {
    pub fn new(id: EdgeId, source: NodeId, target: NodeId) -> Self {
        Self {
            id,
            source: Endpoint::new(source, "out"),
            target: Endpoint::new(target, "in"),
            edge_type: EdgeType::Bezier,
            label: String::new(),
            animated: false,
            marker_start: Marker::None,
            marker_end: Marker::ArrowClosed,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: BTreeMap<NodeId, Node>,
    pub edges: BTreeMap<EdgeId, Edge>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowError {
    InvalidGeometry,
    DuplicateId,
    MissingNode(NodeId),
    MissingHandle(Endpoint),
    WrongHandleDirection,
    ParentCycle,
    SelfConnection,
    DuplicateConnection,
    Cycle,
    Rejected(String),
}
impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FlowError {}
#[derive(Clone, Copy, Debug)]
pub struct ConnectionPolicy {
    pub allow_self: bool,
    pub allow_parallel: bool,
    pub allow_cycles: bool,
}
impl Default for ConnectionPolicy {
    fn default() -> Self {
        Self {
            allow_self: false,
            allow_parallel: false,
            allow_cycles: true,
        }
    }
}
/// Application-specific semantic validation, after structural validation.
pub trait ConnectionValidator {
    fn validate(
        &self,
        graph: &Graph,
        source: &Endpoint,
        target: &Endpoint,
    ) -> Result<(), FlowError>;
}
impl<F> ConnectionValidator for F
where
    F: Fn(&Graph, &Endpoint, &Endpoint) -> Result<(), FlowError>,
{
    fn validate(&self, g: &Graph, s: &Endpoint, t: &Endpoint) -> Result<(), FlowError> {
        self(g, s, t)
    }
}
impl Graph {
    pub fn from_parts(
        nodes: impl IntoIterator<Item = Node>,
        edges: impl IntoIterator<Item = Edge>,
    ) -> Result<Self, FlowError> {
        let mut graph = Self::default();
        for node in nodes {
            if graph.nodes.insert(node.id, node).is_some() {
                return Err(FlowError::DuplicateId);
            }
        }
        for edge in edges {
            if graph.edges.insert(edge.id, edge).is_some() {
                return Err(FlowError::DuplicateId);
            }
        }
        graph.validate()?;
        Ok(graph)
    }
    pub fn handle(&self, endpoint: &Endpoint) -> Result<(&Node, &Handle), FlowError> {
        let node = self
            .nodes
            .get(&endpoint.node)
            .ok_or(FlowError::MissingNode(endpoint.node))?;
        let handle = node
            .handles
            .iter()
            .find(|h| h.id == endpoint.handle)
            .ok_or_else(|| FlowError::MissingHandle(endpoint.clone()))?;
        Ok((node, handle))
    }
    pub fn validate(&self) -> Result<(), FlowError> {
        for (id, node) in &self.nodes {
            if *id != node.id {
                return Err(FlowError::DuplicateId);
            }
            if !node.rect().valid() || node.size.x <= 0. || node.size.y <= 0. {
                return Err(FlowError::InvalidGeometry);
            }
            let mut handles = BTreeSet::new();
            for h in &node.handles {
                if !handles.insert(&h.id) {
                    return Err(FlowError::DuplicateId);
                }
                if !h.offset.is_finite() || !(0. ..=1.).contains(&h.offset) {
                    return Err(FlowError::InvalidGeometry);
                }
            }
        }
        // Three-color traversal visits every parent chain once, including deep hierarchies.
        let mut complete = BTreeSet::new();
        for id in self.nodes.keys() {
            let mut visiting = BTreeSet::new();
            let mut next = Some(*id);
            while let Some(current) = next {
                if complete.contains(&current) {
                    break;
                }
                if !visiting.insert(current) {
                    return Err(FlowError::ParentCycle);
                }
                next = self
                    .nodes
                    .get(&current)
                    .ok_or(FlowError::MissingNode(current))?
                    .parent;
            }
            complete.extend(visiting);
        }
        for (id, edge) in &self.edges {
            if *id != edge.id {
                return Err(FlowError::DuplicateId);
            }
            self.validate_endpoints(&edge.source, &edge.target)?;
        }
        Ok(())
    }
    fn validate_endpoints(&self, source: &Endpoint, target: &Endpoint) -> Result<(), FlowError> {
        if self.handle(source)?.1.kind != HandleKind::Source
            || self.handle(target)?.1.kind != HandleKind::Target
        {
            return Err(FlowError::WrongHandleDirection);
        }
        Ok(())
    }
    pub fn validate_connection(
        &self,
        source: &Endpoint,
        target: &Endpoint,
        policy: ConnectionPolicy,
    ) -> Result<(), FlowError> {
        self.validate_endpoints(source, target)?;
        if !policy.allow_self && source.node == target.node {
            return Err(FlowError::SelfConnection);
        }
        if !policy.allow_parallel
            && self
                .edges
                .values()
                .any(|e| &e.source == source && &e.target == target)
        {
            return Err(FlowError::DuplicateConnection);
        }
        if !policy.allow_cycles {
            let mut adjacency: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
            for e in self.edges.values() {
                adjacency
                    .entry(e.source.node)
                    .or_default()
                    .push(e.target.node);
            }
            let mut seen = BTreeSet::new();
            let mut stack = vec![target.node];
            while let Some(id) = stack.pop() {
                if id == source.node {
                    return Err(FlowError::Cycle);
                }
                if seen.insert(id)
                    && let Some(next) = adjacency.get(&id)
                {
                    stack.extend(next);
                }
            }
        }
        Ok(())
    }
    pub fn bounds(&self) -> Option<Rect> {
        self.nodes.values().map(Node::rect).reduce(Rect::union)
    }
}
