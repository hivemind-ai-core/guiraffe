//! Incremental keyed reconciliation. No GPUI types or window operations.
use crate::Value;
use gpui_kit::SharedString;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Text(SharedString),
    Number(u64),
    Bool(bool),
}
impl Key {
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Str(value) => Some(Self::Text(value.clone())),
            Value::Num(value) if value.is_finite() => {
                Some(Self::Number(if *value == 0. { 0 } else { value.to_bits() }))
            }
            Value::Bool(value) => Some(Self::Bool(*value)),
            _ => None,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Entry<T: Clone> {
    pub value: Value,
    pub state: T,
    key: Option<Key>,
}
pub(crate) struct KeyedList<T: Clone> {
    pub rows: im::Vector<Entry<T>>,
    keys: HashSet<Key>,
    path: Box<[SharedString]>,
}
impl<T: Clone> KeyedList<T> {
    pub fn new(path: Box<[SharedString]>) -> Self {
        Self {
            rows: im::Vector::new(),
            keys: HashSet::new(),
            path,
        }
    }
    pub fn splice(
        &mut self,
        range: Range<usize>,
        values: &Arc<[Value]>,
        mut create: impl FnMut(&Value) -> T,
        mut update: impl FnMut(&T, &Value),
        mut remove: impl FnMut(T),
    ) {
        if range.start > range.end || range.end > self.rows.len() {
            return;
        }
        let replace = range.len() == values.len();
        let suffix = if replace {
            im::Vector::new()
        } else {
            self.rows.split_off(range.end)
        };
        let removed: Vec<_> = if replace {
            range
                .clone()
                .filter_map(|i| self.rows.get(i).cloned())
                .collect()
        } else {
            self.rows.split_off(range.start).into_iter().collect()
        };
        let mut reuse = HashMap::new();
        let mut unkeyed = Vec::new();
        for entry in removed {
            if let Some(key) = entry.key {
                self.keys.remove(&key);
                reuse.insert(key, (entry.value, entry.state));
            } else {
                unkeyed.push(entry.state);
            }
        }
        for (offset, value) in values.iter().enumerate() {
            let key =
                Key::from_value(value.field(&self.path)).filter(|key| !self.keys.contains(key));
            let state =
                if let Some((previous, state)) = key.as_ref().and_then(|key| reuse.remove(key)) {
                    if &previous != value {
                        update(&state, value);
                    }
                    state
                } else {
                    create(value)
                };
            if let Some(key) = &key {
                self.keys.insert(key.clone());
            }
            let entry = Entry {
                value: value.clone(),
                state,
                key,
            };
            if replace {
                self.rows.set(range.start + offset, entry);
            } else {
                self.rows.push_back(entry);
            }
        }
        if !replace {
            self.rows.append(suffix);
        }
        for (_, (_, state)) in reuse {
            remove(state);
        }
        for state in unkeyed {
            remove(state);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn value(id: &str, title: &str) -> Value {
        Value::item_from_json(&serde_json::json!({"id":id,"title":title})).expect("item")
    }
    #[test]
    fn keyed_splice_updates_only_replaced_items_and_preserves_reorder_identity() {
        let mut list = KeyedList::new(vec!["id".into()].into());
        let next = Cell::new(0);
        let create = |_: &Value| {
            let id = next.get();
            next.set(id + 1);
            id
        };
        list.splice(
            0..0,
            &Arc::from([value("a", "A"), value("b", "B"), value("c", "C")]),
            create,
            |_, _| panic!("initial"),
            |_| panic!("initial"),
        );
        let mut changed = Vec::new();
        list.splice(
            1..2,
            &Arc::from([value("b", "new")]),
            create,
            |id, _| changed.push(*id),
            |_| panic!("reused"),
        );
        assert_eq!(changed, vec![1]);
        assert_eq!(next.get(), 3);
        let mut removed = Vec::new();
        list.splice(
            0..3,
            &Arc::from([value("c", "C"), value("a", "A")]),
            create,
            |_, _| panic!("same"),
            |id| removed.push(id),
        );
        assert_eq!(
            list.rows.iter().map(|e| e.state).collect::<Vec<_>>(),
            vec![2, 0]
        );
        assert_eq!(removed, vec![1]);
    }
    #[test]
    fn duplicate_and_missing_keys_never_alias_and_invalid_ranges_are_inert() {
        let mut list = KeyedList::new(vec!["id".into()].into());
        let next = Cell::new(0);
        let create = |_: &Value| {
            let id = next.get();
            next.set(id + 1);
            id
        };
        list.splice(
            0..0,
            &Arc::from([value("a", "A"), value("a", "duplicate"), Value::Null]),
            create,
            |_, _| {},
            |_| {},
        );
        assert_eq!(
            list.rows.iter().map(|e| e.state).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        list.splice(
            9..10,
            &Arc::from([]),
            create,
            |_, _| {},
            |_| panic!("invalid"),
        );
        assert_eq!(list.rows.len(), 3);
    }
    #[test]
    fn numeric_keys_reuse_and_do_not_alias_string_keys() {
        let number =
            Value::item_from_json(&serde_json::json!({"id":1,"title":"number"})).expect("item");
        let string = value("1", "string");
        let mut list = KeyedList::new(vec!["id".into()].into());
        let next = Cell::new(0);
        let create = |_: &Value| {
            let id = next.get();
            next.set(id + 1);
            id
        };
        list.splice(
            0..0,
            &Arc::from([number.clone(), string.clone()]),
            create,
            |_, _| {},
            |_| {},
        );
        list.splice(
            0..2,
            &Arc::from([string, number]),
            create,
            |_, _| panic!("unchanged"),
            |_| panic!("reused"),
        );
        assert_eq!(
            list.rows
                .iter()
                .map(|entry| entry.state)
                .collect::<Vec<_>>(),
            vec![1, 0]
        );
    }
}
