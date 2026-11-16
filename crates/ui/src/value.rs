//! View data values: cheap to clone (shared strings and lists), compared for change detection.

use std::sync::Arc;

use gpui_kit::SharedString;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Num(f64),
    Str(SharedString),
    List(Arc<[Value]>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_ref()),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }
    /// Text rendering of scalars (numbers without a trailing `.0`).
    pub fn display(&self) -> SharedString {
        match self {
            Value::Null => SharedString::default(),
            Value::Bool(b) => b.to_string().into(),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64).into(),
            Value::Num(n) => n.to_string().into(),
            Value::Str(s) => s.clone(),
            Value::List(l) => format!("[{} items]", l.len()).into(),
        }
    }
    pub fn from_json(v: &serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Number(n) => Value::Num(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => Value::Str(s.clone().into()),
            serde_json::Value::Array(a) => Value::List(a.iter().map(Value::from_json).collect()),
            serde_json::Value::Object(_) => Value::Null,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(SharedString::from(s.to_owned()))
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s.into())
    }
}
impl From<SharedString> for Value {
    fn from(s: SharedString) -> Self {
        Value::Str(s)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Num(n)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Num(n as f64)
    }
}
