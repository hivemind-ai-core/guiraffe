//! Compact input payloads. Strings and large values remain shared handles.
use crate::Value;
use gpui_kit::SharedString;
#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    Unit,
    Text(SharedString),
    Bool(bool),
    Selection(Option<SharedString>),
    Value(Value),
}
impl Payload {
    pub fn value(&self) -> Value {
        match self {
            Self::Unit | Self::Selection(None) => Value::Null,
            Self::Text(v) | Self::Selection(Some(v)) => Value::Str(v.clone()),
            Self::Bool(v) => Value::Bool(*v),
            Self::Value(v) => v.clone(),
        }
    }
}
impl From<Value> for Payload {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}
