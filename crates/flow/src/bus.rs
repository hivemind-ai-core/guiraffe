//! The flow canvas on the control bus: `move_node` and `move_nodes` domain actions, and the
//! shared `select` action, which sets the selection by node and edge id.
//!
//! Both go through the editor's move, so each node's [`core::AxisLock`] holds exactly as it
//! does for a pointer drag. `move_nodes` moves several nodes as one edit (one undo step).
use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::{App, Context, Entity, SharedString, Window};
use guiraffe_core::{
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source,
    },
    widget_value::Value,
};

use crate::{FlowState, core};
use core::Point as WorldPoint;

/// Erased action names (the payload is a record).
pub mod action_names {
    /// `{node, x, y}`: move one node to a world position.
    pub const MOVE_NODE: &str = "move_node";
    /// `{nodes: [{node, x, y}, ...]}`: move several nodes as one edit.
    pub const MOVE_NODES: &str = "move_nodes";
    /// `{nodes: [id, ...], edges: [id, ...]}`: set the selection (either list may be
    /// omitted, for none). The shared `select` name.
    pub const SELECT: &str = guiraffe_core::protocol::names::SELECT;
}

/// A typed flow action.
#[derive(Clone, Debug, PartialEq)]
pub enum FlowAction {
    /// Move nodes to world positions; children follow their parents.
    MoveNodes(BTreeMap<core::NodeId, WorldPoint>),
    /// Select exactly these nodes and edges.
    Select {
        nodes: Vec<core::NodeId>,
        edges: Vec<core::EdgeId>,
    },
}

impl FlowAction {
    /// Move one node.
    pub fn move_node(node: core::NodeId, to: WorldPoint) -> Self {
        Self::MoveNodes(BTreeMap::from([(node, to)]))
    }

    /// The erased form a bus client sends.
    pub fn to_action(&self) -> Action {
        let moves = match self {
            Self::MoveNodes(moves) => moves,
            Self::Select { nodes, edges } => {
                let ids = |ids: &[u64]| {
                    Value::List(ids.iter().map(|id| Value::Num(*id as f64)).collect())
                };
                return Action {
                    name: SharedString::new_static(action_names::SELECT),
                    payload: record(&[("nodes", ids(nodes)), ("edges", ids(edges))]),
                };
            }
        };
        let entry = |(node, to): (&core::NodeId, &WorldPoint)| {
            record(&[
                ("node", Value::Num(*node as f64)),
                ("x", Value::Num(f64::from(to.x))),
                ("y", Value::Num(f64::from(to.y))),
            ])
        };
        if moves.len() == 1
            && let Some(only) = moves.iter().next()
        {
            return Action {
                name: SharedString::new_static(action_names::MOVE_NODE),
                payload: entry(only),
            };
        }
        let list: Arc<[Value]> = moves.iter().map(entry).collect();
        Action {
            name: SharedString::new_static(action_names::MOVE_NODES),
            payload: record(&[("nodes", Value::List(list))]),
        }
    }

    /// Decode the erased form.
    pub fn decode(action: &Action) -> Result<Self, ActionError> {
        match action.name.as_ref() {
            action_names::MOVE_NODE => {
                let (node, to) = one_move(&action.payload)?;
                Ok(Self::move_node(node, to))
            }
            action_names::MOVE_NODES => {
                let nodes = action.payload.field(&["nodes".into()]);
                let len = nodes.list_len().ok_or(ActionError::Invalid("nodes"))?;
                let mut moves = BTreeMap::new();
                for index in 0..len {
                    let entry = nodes.list_get(index).ok_or(ActionError::Invalid("nodes"))?;
                    let (node, to) = one_move(entry)?;
                    moves.insert(node, to);
                }
                Ok(Self::MoveNodes(moves))
            }
            action_names::SELECT => Ok(Self::Select {
                nodes: ids(&action.payload, "nodes")?,
                edges: ids(&action.payload, "edges")?,
            }),
            _ => Err(ActionError::Unsupported),
        }
    }
}

fn record(fields: &[(&str, Value)]) -> Value {
    Value::Record(Arc::new(
        fields
            .iter()
            .map(|(k, v)| (SharedString::from(k.to_string()), v.clone()))
            .collect(),
    ))
}

fn num(value: &Value, name: &'static str) -> Result<f64, ActionError> {
    value
        .field(&[SharedString::new_static(name)])
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(ActionError::Invalid(name))
}

/// A list of ids at `name`; absent or null is none.
fn ids(value: &Value, name: &'static str) -> Result<Vec<u64>, ActionError> {
    let list = value.field(&[SharedString::new_static(name)]);
    if matches!(list, Value::Null) {
        return Ok(Vec::new());
    }
    let len = list.list_len().ok_or(ActionError::Invalid(name))?;
    (0..len)
        .map(|index| {
            list.list_get(index)
                .and_then(Value::as_f64)
                .filter(|id| *id >= 0.0 && id.fract() == 0.0 && *id <= 9_007_199_254_740_992.0)
                .map(|id| id as u64)
                .ok_or(ActionError::Invalid(name))
        })
        .collect()
}

fn one_move(value: &Value) -> Result<(core::NodeId, WorldPoint), ActionError> {
    let node = num(value, "node")?;
    if !(node >= 0.0 && node.fract() == 0.0 && node <= 9_007_199_254_740_992.0) {
        return Err(ActionError::Invalid("node"));
    }
    let x = num(value, "x")? as f32;
    let y = num(value, "y")? as f32;
    if !(x.is_finite() && y.is_finite()) {
        return Err(ActionError::Invalid("x"));
    }
    Ok((node as core::NodeId, WorldPoint::new(x, y)))
}

/// Whether the move changed the document.
pub struct FlowOutcome(pub bool);

impl From<FlowOutcome> for Outcome {
    fn from(FlowOutcome(changed): FlowOutcome) -> Self {
        Outcome(Value::Bool(changed))
    }
}

impl FlowState {
    /// Put this flow on the control bus at `path`, driven by [`FlowAction`]s.
    pub fn register(view: &Entity<Self>, path: &str, window: &Window, cx: &mut App) -> ControlId {
        ActionBus::register(path, view, "flow", Capabilities::empty(), window, cx)
    }

    /// Apply a flow action, as the bus does. Node moves keep each node's axis lock.
    pub fn perform(
        &mut self,
        action: FlowAction,
        cx: &mut Context<Self>,
    ) -> Result<bool, ActionError> {
        let moves = match action {
            FlowAction::MoveNodes(moves) => moves,
            FlowAction::Select { nodes, edges } => {
                let source = ActionBus::current_source(cx).unwrap_or(Source::Host);
                return Ok(self.set_selection(nodes, edges, source, cx));
            }
        };
        for id in moves.keys() {
            if !self.graph().nodes.contains_key(id) {
                return Err(ActionError::Invalid("node"));
            }
        }
        self.move_nodes(&moves, cx)
            .map_err(|_| ActionError::Invalid("node"))
    }
}

impl Observe for FlowState {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        let selected: Vec<usize> = self
            .selected_nodes()
            .iter()
            .map(|id| *id as usize)
            .collect();
        ControlSnapshot {
            selected: (!selected.is_empty()).then(|| selected.into()),
            ..Default::default()
        }
    }
}

impl Drive for FlowState {
    type Action = FlowAction;
    type Output = FlowOutcome;
    fn decode(action: &Action) -> Result<FlowAction, ActionError> {
        FlowAction::decode(action)
    }
    fn apply(
        &mut self,
        action: FlowAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<FlowOutcome, ActionError> {
        self.perform(action, cx).map(FlowOutcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erased_actions_round_trip() {
        let one = FlowAction::move_node(3, WorldPoint::new(10., -4.5));
        assert_eq!(one.to_action().name.as_ref(), action_names::MOVE_NODE);
        assert_eq!(FlowAction::decode(&one.to_action()), Ok(one));
        let many = FlowAction::MoveNodes(BTreeMap::from([
            (1, WorldPoint::new(1., 2.)),
            (2, WorldPoint::new(3., 4.)),
        ]));
        assert_eq!(many.to_action().name.as_ref(), action_names::MOVE_NODES);
        assert_eq!(FlowAction::decode(&many.to_action()), Ok(many));
        let select = FlowAction::Select {
            nodes: vec![1, 3],
            edges: vec![2],
        };
        assert_eq!(select.to_action().name.as_ref(), "select");
        assert_eq!(FlowAction::decode(&select.to_action()), Ok(select));
        let none = Action {
            name: action_names::SELECT.into(),
            payload: Value::Null,
        };
        assert_eq!(
            FlowAction::decode(&none),
            Ok(FlowAction::Select {
                nodes: vec![],
                edges: vec![],
            })
        );
    }

    #[test]
    fn bad_payloads_are_rejected() {
        let action = Action {
            name: action_names::MOVE_NODE.into(),
            payload: record(&[
                ("node", Value::Num(1.5)),
                ("x", Value::Num(0.)),
                ("y", Value::Num(0.)),
            ]),
        };
        assert_eq!(
            FlowAction::decode(&action),
            Err(ActionError::Invalid("node"))
        );
    }
}
