//! The slot table: view data addressed by index, updated through patches.
//!
//! Logic never sends views, only [`Patch`]es (data motion ∝ change, spec/15). Applying a
//! patch that doesn't change the value is a no-op, so nothing re-renders for it.

use crate::value::Value;

/// A compact slot address (resolved from a name when the document is compiled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Patch {
    Set(SlotId, Value),
}

/// Reusable patch buffer handed to logic (no allocation per event once warm).
#[derive(Debug, Default)]
pub struct PatchBuf {
    patches: Vec<Patch>,
}

impl PatchBuf {
    pub fn set(&mut self, slot: SlotId, value: impl Into<Value>) {
        self.patches.push(Patch::Set(slot, value.into()));
    }
    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }
    pub fn len(&self) -> usize {
        self.patches.len()
    }
    pub(crate) fn drain(&mut self) -> std::vec::Drain<'_, Patch> {
        self.patches.drain(..)
    }
}

#[derive(Debug, Default, Clone)]
pub struct SlotTable {
    values: Vec<Value>,
    /// Bumped per changed slot; nodes bound to a slot re-render only when it moves.
    versions: Vec<u64>,
    generation: u64,
}

impl SlotTable {
    pub fn with_defaults(defaults: &[Value]) -> Self {
        Self {
            values: defaults.to_vec(),
            versions: vec![0; defaults.len()],
            generation: 0,
        }
    }

    pub fn with_len(n: usize) -> Self {
        Self {
            values: vec![Value::Null; n],
            versions: vec![0; n],
            generation: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, id: SlotId) -> &Value {
        static NULL: Value = Value::Null;
        self.values.get(id.0 as usize).unwrap_or(&NULL)
    }

    pub fn version(&self, id: SlotId) -> u64 {
        self.versions.get(id.0 as usize).copied().unwrap_or(0)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Apply patches; returns how many slots actually changed. Unknown slots are ignored.
    pub fn apply(&mut self, buf: &mut PatchBuf) -> usize {
        self.apply_tracked(buf, &mut Vec::new())
    }

    /// Like [`SlotTable::apply`], also recording which slots changed (for targeted invalidation).
    pub fn apply_tracked(&mut self, buf: &mut PatchBuf, changed_slots: &mut Vec<SlotId>) -> usize {
        let mut changed = 0;
        for p in buf.drain() {
            match p {
                Patch::Set(id, v) => {
                    let i = id.0 as usize;
                    if let Some(slot) = self.values.get_mut(i)
                        && *slot != v
                    {
                        *slot = v;
                        self.versions[i] += 1;
                        changed += 1;
                        if !changed_slots.contains(&id) {
                            changed_slots.push(id);
                        }
                    }
                }
            }
        }
        if changed > 0 {
            self.generation += 1;
        }
        changed
    }

    /// Carry values across a re-compile (hot reload) for slots with the same name.
    pub fn migrate(
        &self,
        old_names: &[String],
        new_names: &[String],
        new_defaults: &[Value],
    ) -> SlotTable {
        let old: std::collections::HashMap<&str, usize> = old_names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        let mut t = SlotTable {
            values: Vec::with_capacity(new_names.len()),
            versions: Vec::with_capacity(new_names.len()),
            generation: self.generation + 1,
        };
        for (ni, name) in new_names.iter().enumerate() {
            match old.get(name.as_str()) {
                Some(&oi) => {
                    t.values.push(self.values[oi].clone());
                    t.versions.push(self.versions[oi]);
                }
                None => {
                    t.values
                        .push(new_defaults.get(ni).cloned().unwrap_or_default());
                    t.versions.push(1);
                }
            }
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_apply_and_noops_dont_count() {
        let mut t = SlotTable::with_len(2);
        let mut b = PatchBuf::default();
        b.set(SlotId(0), "a");
        b.set(SlotId(1), 3i64);
        b.set(SlotId(9), "out of range ignored");
        assert_eq!(t.apply(&mut b), 2);
        assert!(b.is_empty(), "buffer drained for reuse");
        let g = t.generation();
        b.set(SlotId(0), "a");
        assert_eq!(t.apply(&mut b), 0, "same value: no change");
        assert_eq!(t.generation(), g);
        assert_eq!(t.version(SlotId(0)), 1);
        assert_eq!(t.get(SlotId(1)).display().as_ref(), "3");
        assert_eq!(t.get(SlotId(42)), &Value::Null);
    }

    #[test]
    fn migrate_keeps_values_by_name() {
        let mut t = SlotTable::with_len(2);
        let mut b = PatchBuf::default();
        b.set(SlotId(0), "kept");
        b.set(SlotId(1), "dropped");
        t.apply(&mut b);
        let m = t.migrate(
            &["title".into(), "gone".into()],
            &["new".into(), "title".into()],
            &["fresh".into(), Value::Null],
        );
        assert_eq!(
            m.get(SlotId(1)).as_str(),
            Some("kept"),
            "live value survives reload"
        );
        assert_eq!(
            m.get(SlotId(0)).as_str(),
            Some("fresh"),
            "new slot gets its default"
        );
    }
}
