//! View data values: cheap to clone (shared strings and lists), compared for change detection.

use std::sync::Arc;

use gpui_kit::SharedString;

#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Num(f64),
    Str(SharedString),
    List(Arc<[Value]>),
    /// Persistent list storage used after a splice (shares unaffected tree nodes).
    Sequence(im::Vector<Value>),
    /// Record fields for item-relative bindings.
    Record(Arc<std::collections::BTreeMap<SharedString, Value>>),
}

/// Equality tuned for change detection: numbers compare by bit pattern (so NaN == NaN and a
/// NaN patch is a no-op), and lists short-circuit on pointer identity before comparing items.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a.to_bits() == b.to_bits() || (a == b),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::List(a), Value::List(b)) => Arc::ptr_eq(a, b) || a[..] == b[..],
            (Value::Sequence(a), Value::Sequence(b)) => a.ptr_eq(b) || a == b,
            (Value::Record(a), Value::Record(b)) => Arc::ptr_eq(a, b) || a == b,
            (a, b) if a.list_len().is_some() && a.list_len() == b.list_len() => {
                (0..a.list_len().unwrap_or(0)).all(|i| a.list_get(i) == b.list_get(i))
            }
            _ => false,
        }
    }
}

impl Value {
    pub fn list_len(&self) -> Option<usize> {
        match self {
            Self::List(v) => Some(v.len()),
            Self::Sequence(v) => Some(v.len()),
            _ => None,
        }
    }
    pub fn list_get(&self, index: usize) -> Option<&Value> {
        match self {
            Self::List(v) => v.get(index),
            Self::Sequence(v) => v.get(index),
            _ => None,
        }
    }
    pub fn field(&self, path: &[SharedString]) -> &Value {
        static NULL: Value = Value::Null;
        path.iter()
            .try_fold(self, |value, key| match value {
                Self::Record(fields) => fields.get(key),
                _ => None,
            })
            .unwrap_or(&NULL)
    }
    /// Copy only record fields along the compiled path; a missing final field may be inserted.
    /// Invalid intermediate shapes are rejected without mutation.
    pub fn with_field(&self, path: &[SharedString], value: Value) -> Option<Self> {
        let Some((head, tail)) = path.split_first() else {
            return Some(value);
        };
        let Self::Record(fields) = self else {
            return None;
        };
        let child = if tail.is_empty() {
            value
        } else {
            fields.get(head)?.with_field(tail, value)?
        };
        if fields.get(head) == Some(&child) {
            return Some(self.clone());
        }
        let mut fields = fields.as_ref().clone();
        fields.insert(head.clone(), child);
        Some(Self::Record(Arc::new(fields)))
    }
    pub(crate) fn persistent(self) -> Self {
        match self {
            Self::List(values) => Self::Sequence(values.iter().cloned().collect()),
            value => value,
        }
    }
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
    /// Text rendering of scalars (integral numbers without a trailing `.0`).
    pub fn display(&self) -> SharedString {
        match self {
            Value::Null => SharedString::default(),
            Value::Bool(b) => b.to_string().into(),
            Value::Num(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 => {
                format!("{}", *n as i64).into()
            }
            // Like JavaScript: exponent form for huge/tiny magnitudes, never 300 digits.
            Value::Num(n) if n.is_finite() && *n != 0.0 && (n.abs() >= 1e21 || n.abs() < 1e-6) => {
                format!("{n:e}").into()
            }
            Value::Num(n) => n.to_string().into(),
            Value::Str(s) => s.clone(),
            Value::List(l) => format!("[{} items]", l.len()).into(),
            Value::Sequence(l) => format!("[{} items]", l.len()).into(),
            Value::Record(_) => "[record]".into(),
        }
    }
    /// Records are accepted inside list items, while direct object-valued props remain errors.
    pub fn item_from_json(v: &serde_json::Value) -> Result<Self, &'static str> {
        if let serde_json::Value::Object(fields) = v {
            Ok(Self::Record(Arc::new(
                fields
                    .iter()
                    .map(|(key, value)| Ok((key.clone().into(), Self::item_from_json(value)?)))
                    .collect::<Result<_, &'static str>>()?,
            )))
        } else {
            Self::try_from_json(v)
        }
    }
    /// Convert JSON; objects are not values (`Err` lets the compiler report them).
    pub fn try_from_json(v: &serde_json::Value) -> Result<Value, &'static str> {
        Ok(match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Number(n) => Value::Num(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => Value::Str(s.clone().into()),
            serde_json::Value::Array(a) => Value::List(
                a.iter()
                    .map(Value::item_from_json)
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            serde_json::Value::Object(_) => return Err("objects are not supported as prop values"),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_and_shared_lists_compare_equal() {
        assert_eq!(
            Value::Num(f64::NAN),
            Value::Num(f64::NAN),
            "NaN patch is a no-op"
        );
        let l: Arc<[Value]> = vec![Value::from(1i64)].into();
        assert_eq!(Value::List(l.clone()), Value::List(l));
        assert_ne!(Value::Num(1.0), Value::Str("1".into()));
        assert_eq!(
            Value::Num(0.0),
            Value::Num(-0.0),
            "0 == -0 for display purposes"
        );
    }

    #[test]
    fn json_objects_are_rejected_and_big_numbers_display_safely() {
        assert!(Value::try_from_json(&serde_json::json!({"a": 1})).is_err());
        assert!(Value::try_from_json(&serde_json::json!([1, {"a": 1}])).is_ok());
        assert_eq!(Value::Num(1e300).display().as_ref(), "1e300");
        assert_eq!(Value::Num(42.0).display().as_ref(), "42");
        assert_eq!(Value::Num(f64::NAN).display().as_ref(), "NaN");
    }
}
