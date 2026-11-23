//! Serializable document. Validate untrusted/deserialized documents before use.
use crate::{Point, Rect};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

pub type NodeId = u64;
pub type EdgeId = u64;

/// Immutable, cheaply cloned text for handle ids and node types.
///
/// The defaults used by [`Node::new`] and [`Edge::new`] (`"in"`, `"out"`,
/// `"default"`) share one process-wide allocation, so a default document stores
/// no per-node or per-edge string bytes for them. Serializes as a plain string.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(Arc<str>);

impl Name {
    fn common() -> &'static [Arc<str>; 4] {
        static COMMON: OnceLock<[Arc<str>; 4]> = OnceLock::new();
        COMMON.get_or_init(|| ["".into(), "in".into(), "out".into(), "default".into()])
    }
    fn interned(text: &str) -> Option<Self> {
        Self::common()
            .iter()
            .find(|common| ***common == *text)
            .map(|common| Self(common.clone()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Default for Name {
    fn default() -> Self {
        Self::from("")
    }
}
impl From<&str> for Name {
    fn from(text: &str) -> Self {
        Self::interned(text).unwrap_or_else(|| Self(text.into()))
    }
}
impl From<&String> for Name {
    fn from(text: &String) -> Self {
        Self::from(text.as_str())
    }
}
impl From<String> for Name {
    fn from(text: String) -> Self {
        Self::interned(&text).unwrap_or_else(|| Self(text.into()))
    }
}
impl From<Name> for String {
    fn from(name: Name) -> Self {
        name.as_str().to_owned()
    }
}
impl std::ops::Deref for Name {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl std::borrow::Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        *self.0 == *other
    }
}
impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        *self.0 == **other
    }
}
impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        *self.0 == **other
    }
}
impl std::fmt::Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}
impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&*self.0, f)
    }
}
impl Serialize for Name {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for Name {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

/// Copy-on-write handle list. Nodes built by [`Node::new`] share one default
/// `in`/`out` list; mutable access (`push`, indexing, `iter_mut`) clones it first.
#[derive(Clone)]
pub struct Handles(Arc<Vec<Handle>>);

impl Handles {
    fn defaults() -> Self {
        static DEFAULT: OnceLock<Arc<Vec<Handle>>> = OnceLock::new();
        Self(
            DEFAULT
                .get_or_init(|| {
                    Arc::new(vec![
                        Handle::new("in", HandleKind::Target, Side::Left),
                        Handle::new("out", HandleKind::Source, Side::Right),
                    ])
                })
                .clone(),
        )
    }
    /// Whether both lists share storage (no copy-on-write happened since a clone).
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Default for Handles {
    fn default() -> Self {
        Self(Arc::new(Vec::new()))
    }
}
impl PartialEq for Handles {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.0 == other.0
    }
}
impl std::fmt::Debug for Handles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}
impl std::ops::Deref for Handles {
    type Target = Vec<Handle>;
    fn deref(&self) -> &Vec<Handle> {
        &self.0
    }
}
impl std::ops::DerefMut for Handles {
    fn deref_mut(&mut self) -> &mut Vec<Handle> {
        Arc::make_mut(&mut self.0)
    }
}
impl From<Vec<Handle>> for Handles {
    fn from(handles: Vec<Handle>) -> Self {
        Self(Arc::new(handles))
    }
}
impl From<Handles> for Vec<Handle> {
    fn from(handles: Handles) -> Self {
        Arc::unwrap_or_clone(handles.0)
    }
}
impl FromIterator<Handle> for Handles {
    fn from_iter<I: IntoIterator<Item = Handle>>(iter: I) -> Self {
        Self(Arc::new(iter.into_iter().collect()))
    }
}
impl<'a> IntoIterator for &'a Handles {
    type Item = &'a Handle;
    type IntoIter = std::slice::Iter<'a, Handle>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl Serialize for Handles {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Handles {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<Handle>::deserialize(deserializer).map(Self::from)
    }
}
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
    pub id: Name,
    pub kind: HandleKind,
    pub side: Side,
    pub offset: f32,
}
impl Handle {
    pub fn new(id: impl Into<Name>, kind: HandleKind, side: Side) -> Self {
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
    pub node_type: Name,
    pub parent: Option<NodeId>,
    pub selected: bool,
    pub draggable: bool,
    pub handles: Handles,
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
            handles: Handles::defaults(),
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
    OrthogonalHorizontal,
    OrthogonalVertical,
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
    pub handle: Name,
}
impl Endpoint {
    pub fn new(node: NodeId, handle: impl Into<Name>) -> Self {
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
    pub nodes: imbl::OrdMap<NodeId, Node>,
    pub edges: imbl::OrdMap<EdgeId, Edge>,
    /// High-water mark; never rewound by undo, serialized even after edge deletion.
    #[serde(default)]
    pub edge_id_high_water: EdgeId,
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
/// Bulk-load a persistent map with densely packed leaves.
///
/// `imbl::OrdMap` is a B+ tree with fixed 16-entry leaves that split at the
/// median, so inserting ascending keys leaves every leaf half full: twice the
/// entry size per key. The first pass inserts every second sorted key, so each
/// half-full leaf spans exactly 16 keys of the sorted input. The second pass
/// fills those gaps without splitting, so leaves end full. A changed `imbl`
/// node width affects only the density (`tests/memory.rs` locks it); the map is
/// the same.
fn packed<V: Clone>(
    values: impl IntoIterator<Item = V>,
    key: impl Fn(&V) -> u64,
) -> Result<imbl::OrdMap<u64, V>, FlowError> {
    // `imbl` 7 leaf width and the keys a split leaves in the left leaf.
    const WIDTH: usize = 16;
    const HALF: usize = WIDTH / 2;
    let mut values: Vec<V> = values.into_iter().collect();
    values.sort_unstable_by_key(&key);
    if values.windows(2).any(|pair| key(&pair[0]) == key(&pair[1])) {
        return Err(FlowError::DuplicateId);
    }
    let mut slots: Vec<Option<V>> = values.into_iter().map(Some).collect();
    let mut map = imbl::OrdMap::new();
    // Ascending splits start a leaf at every HALF-th first-pass key, and that key
    // stays in the leaf, so HALF first-pass keys must cover WIDTH sorted positions.
    let len = slots.len();
    let sparse = (0..)
        .map(|step: usize| step * WIDTH / HALF)
        .take_while(|&position| position < len);
    for position in sparse {
        if let Some(value) = slots.get_mut(position).and_then(Option::take) {
            map.insert(key(&value), value);
        }
    }
    for value in slots.into_iter().flatten() {
        map.insert(key(&value), value);
    }
    Ok(map)
}

impl Graph {
    pub fn from_parts(
        nodes: impl IntoIterator<Item = Node>,
        edges: impl IntoIterator<Item = Edge>,
    ) -> Result<Self, FlowError> {
        let mut graph = Self {
            nodes: packed(nodes, |node| node.id)?,
            ..Self::default()
        };
        graph.edges = packed(edges, |edge| edge.id)?;
        graph.edge_id_high_water = graph.edges.keys().next_back().copied().unwrap_or(0);
        graph.validate()?;
        Ok(graph)
    }
    pub fn handle(&self, endpoint: &Endpoint) -> Result<(&Node, &Handle), FlowError> {
        let node = self
            .nodes
            .get(&endpoint.node)
            .ok_or(FlowError::MissingNode(endpoint.node))?;
        let handle = Self::node_handle(node, endpoint)?;
        Ok((node, handle))
    }
    pub(crate) fn node_handle<'a>(
        node: &'a Node,
        endpoint: &Endpoint,
    ) -> Result<&'a Handle, FlowError> {
        node.handles
            .iter()
            .find(|h| h.id == endpoint.handle)
            .ok_or_else(|| FlowError::MissingHandle(endpoint.clone()))
    }
    pub fn validate(&self) -> Result<(), FlowError> {
        self.validated_node_index().map(|_| ())
    }
    pub(crate) fn validated_node_index(&self) -> Result<BTreeMap<NodeId, &Node>, FlowError> {
        // Bulk validation uses a transient read index, avoiding repeated persistent-tree walks.
        let nodes: BTreeMap<_, _> = self.nodes.iter().map(|(id, node)| (*id, node)).collect();
        for (id, node) in &nodes {
            if *id != node.id {
                return Err(FlowError::DuplicateId);
            }
            if !node.rect().valid() || node.size.x <= 0. || node.size.y <= 0. {
                return Err(FlowError::InvalidGeometry);
            }
            let mut handles = BTreeSet::new();
            for (index, h) in node.handles.iter().enumerate() {
                let duplicate = if node.handles.len() <= 8 {
                    node.handles[..index].iter().any(|prior| prior.id == h.id)
                } else {
                    !handles.insert(&h.id)
                };
                if duplicate {
                    return Err(FlowError::DuplicateId);
                }
                if !h.offset.is_finite() || !(0. ..=1.).contains(&h.offset) {
                    return Err(FlowError::InvalidGeometry);
                }
            }
        }
        // Three-color traversal visits every parent chain once, including deep hierarchies.
        let mut complete = BTreeSet::new();
        for (&id, node) in nodes.iter().filter(|(_, node)| node.parent.is_some()) {
            let mut visiting = BTreeSet::from([id]);
            let mut next = node.parent;
            while let Some(current) = next {
                if complete.contains(&current) {
                    break;
                }
                if !visiting.insert(current) {
                    return Err(FlowError::ParentCycle);
                }
                next = nodes
                    .get(&current)
                    .ok_or(FlowError::MissingNode(current))?
                    .parent;
            }
            complete.extend(visiting);
        }
        if self.bounds().is_some_and(|bounds| !bounds.valid()) {
            return Err(FlowError::InvalidGeometry);
        }
        for (id, edge) in &self.edges {
            if *id != edge.id {
                return Err(FlowError::DuplicateId);
            }
            for (endpoint, kind) in [
                (&edge.source, HandleKind::Source),
                (&edge.target, HandleKind::Target),
            ] {
                let node = nodes
                    .get(&endpoint.node)
                    .ok_or(FlowError::MissingNode(endpoint.node))?;
                if Self::node_handle(node, endpoint)?.kind != kind {
                    return Err(FlowError::WrongHandleDirection);
                }
            }
        }
        Ok(nodes)
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

#[cfg(test)]
mod storage_tests {
    use super::*;

    #[test]
    fn packed_maps_match_sequential_insertion_for_any_order() {
        for len in [0_u64, 1, 2, 31, 32, 33, 63, 64, 65, 66, 129, 2_113, 5_000] {
            // A deterministic permutation: 7919 is prime and larger than every length.
            let keys = (0..len).map(|i| (i * 7_919) % len.max(1));
            let map = packed(keys.map(|k| (k, k * 3)), |(k, _)| *k).expect("unique keys");
            let expected: imbl::OrdMap<u64, (u64, u64)> =
                (0..len).map(|k| (k, (k, k * 3))).collect();
            assert_eq!(map, expected, "len {len}");
            assert_eq!(map.len() as u64, len);
        }
        assert_eq!(
            packed([(4, 0), (1, 0), (4, 1)], |(k, _)| *k),
            Err(FlowError::DuplicateId)
        );
    }

    #[test]
    fn names_intern_defaults_and_compare_as_text() {
        let a = Name::from("out");
        let b = Name::from(String::from("out"));
        assert!(Arc::ptr_eq(&a.0, &b.0));
        assert_eq!(a, "out");
        assert_eq!(Name::from("custom"), *"custom");
        assert_eq!(format!("{a:?} {a}"), "\"out\" out");
        let json = serde_json::to_string(&Endpoint::new(3, "in")).expect("serialize");
        assert_eq!(json, r#"{"node":3,"handle":"in"}"#);
        let back: Endpoint = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, Endpoint::new(3, "in"));
        assert!(Arc::ptr_eq(&back.handle.0, &Name::from("in").0));
    }
}
