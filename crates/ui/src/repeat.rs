//! Incremental keyed reconciliation. No GPUI types or window operations.
use crate::{
    Value,
    position::{Position, Positions},
};
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
    pub position: Position,
}
pub(crate) struct KeyedList<T: Clone> {
    pub rows: im::Vector<Entry<T>>,
    keys: HashSet<Key>,
    path: Box<[SharedString]>,
    positions: Positions,
    duplicates: bool,
}
impl<T: Clone> KeyedList<T> {
    pub fn new(path: Box<[SharedString]>) -> Self {
        Self {
            rows: im::Vector::new(),
            keys: HashSet::new(),
            path,
            positions: Positions::default(),
            duplicates: false,
        }
    }
    pub fn splice(
        &mut self,
        range: Range<usize>,
        values: &Arc<[Value]>,
        mut create: impl FnMut(&Value, Position) -> T,
        mut update: impl FnMut(&T, &Value),
        mut remove: impl FnMut(T),
    ) -> bool {
        if range.start > range.end || range.end > self.rows.len() {
            return false;
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
        let old_positions: Vec<_> = removed.iter().map(|entry| entry.position.id()).collect();
        let mut new_positions = Vec::with_capacity(values.len());
        let mut reuse = HashMap::new();
        let mut unkeyed = HashMap::new();
        for (offset, entry) in removed.into_iter().enumerate() {
            if let Some(key) = entry.key {
                self.keys.remove(&key);
                reuse.insert(key, (entry.value, entry.state, entry.position));
            } else {
                unkeyed.insert(offset, (entry.value, entry.state, entry.position));
            }
        }
        for (offset, value) in values.iter().enumerate() {
            let key =
                Key::from_value(value.field(&self.path)).filter(|key| !self.keys.contains(key));
            let positional = if replace
                && key.is_none()
                && unkeyed.get(&offset).is_some_and(|(previous, _, _)| {
                    Key::from_value(previous.field(&self.path))
                        == Key::from_value(value.field(&self.path))
                }) {
                unkeyed.remove(&offset)
            } else {
                None
            };
            let (state, position) = if let Some((previous, state, position)) = key
                .as_ref()
                .and_then(|key| reuse.remove(key))
                .or(positional)
            {
                if &previous != value {
                    update(&state, value);
                }
                (state, position)
            } else {
                let position = self.positions.create();
                (create(value, position.clone()), position)
            };
            if let Some(key) = &key {
                self.keys.insert(key.clone());
            }
            if key.is_none() && Key::from_value(value.field(&self.path)).is_some() {
                self.duplicates = true;
            }
            new_positions.push(position.clone());
            let entry = Entry {
                value: value.clone(),
                state,
                key,
                position,
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
        let structural = old_positions
            .iter()
            .copied()
            .ne(new_positions.iter().map(Position::id));
        if structural {
            self.positions.splice(range, &new_positions);
        }
        for (_, (_, state, _)) in reuse {
            remove(state);
        }
        for (_, (_, state, _)) in unkeyed {
            remove(state);
        }
        if self.duplicates {
            self.duplicates = false;
            for i in 0..self.rows.len() {
                let Some(mut entry) = self.rows.get(i).cloned() else {
                    continue;
                };
                if entry.key.is_none()
                    && let Some(key) = Key::from_value(entry.value.field(&self.path))
                {
                    if self.keys.insert(key.clone()) {
                        entry.key = Some(key);
                        self.rows.set(i, entry);
                    } else {
                        self.duplicates = true;
                    }
                }
            }
        }
        structural
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
        let create = |_: &Value, _: Position| {
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
        let create = |_: &Value, _: Position| {
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
        let create = |_: &Value, _: Position| {
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
    #[test]
    fn surviving_duplicate_recovers_its_key_and_identity() {
        let mut list = KeyedList::new(vec!["id".into()].into());
        let next = Cell::new(0);
        let create = |_: &Value, _: Position| {
            let n = next.get();
            next.set(n + 1);
            n
        };
        list.splice(
            0..0,
            &Arc::from([value("a", "first"), value("a", "second")]),
            create,
            |_, _| {},
            |_| {},
        );
        list.splice(0..1, &Arc::from([]), create, |_, _| {}, |_| {});
        list.splice(
            0..1,
            &Arc::from([value("a", "updated")]),
            create,
            |id, _| assert_eq!(*id, 1),
            |_| panic!("recovered key must reuse"),
        );
        assert_eq!(list.rows.front().map(|row| row.state), Some(1));
        assert_eq!(next.get(), 2);
    }
    #[test]
    fn positional_field_edits_keep_unkeyed_and_duplicate_input_state() {
        let mut list = KeyedList::new(vec!["id".into()].into());
        let next = Cell::new(0);
        let create = |_: &Value, _: Position| {
            let n = next.get();
            next.set(n + 1);
            n
        };
        list.splice(
            0..0,
            &Arc::from([
                value("a", "first"),
                value("a", "second"),
                Value::from("scalar"),
            ]),
            create,
            |_, _| {},
            |_| {},
        );
        list.splice(
            1..2,
            &Arc::from([value("a", "typed")]),
            create,
            |id, _| assert_eq!(*id, 1),
            |_| panic!("retain duplicate"),
        );
        list.splice(
            2..3,
            &Arc::from([Value::from("edited")]),
            create,
            |id, _| assert_eq!(*id, 2),
            |_| panic!("retain scalar"),
        );
        assert_eq!(next.get(), 3);
    }
}
