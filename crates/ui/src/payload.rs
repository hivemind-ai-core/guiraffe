//! Compact inputs and live item addresses. Scoped callbacks resolve their address at dispatch,
//! so a cached handler keeps targeting its item after insertion or reorder.
use crate::{SlotId, Value};
use gpui_kit::SharedString;
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq)]
pub struct ItemContext {
    pub slot: SlotId,
    pub key: Value,
    pub key_path: Arc<[SharedString]>,
    pub index: usize,
    /// Stable identity within this host; disambiguates duplicate or missing keys.
    pub identity: u64,
    pub repeat: u32,
    pub parent: Option<Arc<ItemContext>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ItemChange {
    pub item: ItemContext,
    pub path: Arc<[SharedString]>,
    pub value: Value,
}
impl ItemChange {
    pub fn with_value(&self, value: impl Into<Value>) -> Self {
        Self {
            value: value.into(),
            ..self.clone()
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedPayload {
    pub item: ItemContext,
    pub input: Payload,
    pub change: Option<Arc<[SharedString]>>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    Unit,
    Text(SharedString),
    Bool(bool),
    Selection(Option<SharedString>),
    Value(Value),
    Scoped(Arc<ScopedPayload>),
}
impl Payload {
    pub fn input(&self) -> &Payload {
        match self {
            Self::Scoped(scope) => scope.input.input(),
            input => input,
        }
    }
    pub fn item(&self) -> Option<&ItemContext> {
        match self {
            Self::Scoped(scope) => Some(&scope.item),
            _ => None,
        }
    }
    pub fn item_change(&self) -> Option<ItemChange> {
        let Self::Scoped(scope) = self else {
            return None;
        };
        Some(ItemChange {
            item: scope.item.clone(),
            path: scope.change.clone()?,
            value: scope.input.value(),
        })
    }
    /// The input value, preserving the legacy global Value-based logic boundary.
    /// Override Logic::on_payload to inspect item context.
    pub fn value(&self) -> Value {
        match self.input() {
            Self::Unit | Self::Selection(None) => Value::Null,
            Self::Text(v) | Self::Selection(Some(v)) => Value::Str(v.clone()),
            Self::Bool(v) => Value::Bool(*v),
            Self::Value(v) => v.clone(),
            Self::Scoped(_) => Value::Null,
        }
    }
}
impl From<Value> for Payload {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChangeTarget {
    Slot(SlotId),
    Item(Arc<[SharedString]>),
}
pub(crate) struct InputRequest {
    pub event: Option<crate::EventId>,
    pub target: Option<ChangeTarget>,
    pub scope: u64,
    pub payload: Payload,
    pub generation: u32,
}
