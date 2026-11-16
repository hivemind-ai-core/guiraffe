//! The slot table: view data addressed by index, updated through patches.
//!
//! Logic never sends views, only [`Patch`]es (data motion ∝ change, spec/15). Applying a
//! patch that doesn't change the value is a no-op, so nothing re-renders for it.

use crate::value::Value;
use std::{ops::Range, sync::Arc};

/// A compact slot address (resolved from a name when the document is compiled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Patch {
    Set(SlotId, Value),
    Splice(SlotId, Range<usize>, Arc<[Value]>),
}

/// Reusable patch buffer handed to logic (no allocation per event once warm).
#[derive(Debug, Default)]
pub struct PatchBuf {
    patches: Vec<Patch>,
    veto: bool,
}

impl PatchBuf {
    pub fn veto(&mut self) {
        self.veto = true;
    }
    pub(crate) fn take_veto(&mut self) -> bool {
        std::mem::take(&mut self.veto)
    }
    pub fn set(&mut self, slot: impl Into<SlotId>, value: impl Into<Value>) {
        self.patches.push(Patch::Set(slot.into(), value.into()));
    }
    pub fn splice(&mut self, slot: impl Into<SlotId>, range: Range<usize>, values: Arc<[Value]>) {
        self.patches.push(Patch::Splice(slot.into(), range, values));
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
    styles: Vec<(
        Option<guiraffe_core::Size>,
        Option<guiraffe_core::ColorName>,
    )>,
}

impl SlotTable {
    pub fn with_defaults(defaults: &[Value]) -> Self {
        Self {
            styles: defaults.iter().map(style).collect(),
            values: defaults.iter().cloned().map(Value::persistent).collect(),
            versions: vec![0; defaults.len()],
            generation: 0,
        }
    }

    pub fn with_len(n: usize) -> Self {
        Self {
            styles: vec![(None, None); n],
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

    pub fn size(&self, id: SlotId) -> Option<guiraffe_core::Size> {
        self.styles.get(id.0 as usize).and_then(|s| s.0)
    }
    pub fn color(&self, id: SlotId) -> Option<guiraffe_core::ColorName> {
        self.styles.get(id.0 as usize).and_then(|s| s.1)
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
        self.apply_detailed(buf, changed_slots, &mut Vec::new())
    }

    pub(crate) fn apply_detailed(
        &mut self,
        buf: &mut PatchBuf,
        changed_slots: &mut Vec<SlotId>,
        edits: &mut Vec<Patch>,
    ) -> usize {
        let mut changed = 0;
        for p in buf.drain() {
            match p {
                Patch::Set(id, v) => {
                    let i = id.0 as usize;
                    if let Some(slot) = self.values.get_mut(i)
                        && *slot != v
                    {
                        if let Some(cache) = self.styles.get_mut(i) {
                            *cache = style(&v);
                        }
                        *slot = v.persistent();
                        edits.push(Patch::Set(id, slot.clone()));
                        if let Some(version) = self.versions.get_mut(i) {
                            *version = version.wrapping_add(1);
                        }
                        changed += 1;
                        if !changed_slots.contains(&id) {
                            changed_slots.push(id);
                        }
                    }
                }
                Patch::Splice(id, range, values) => {
                    let Some(slot) = self.values.get_mut(id.0 as usize) else {
                        continue;
                    };
                    let Some(len) = slot.list_len() else { continue };
                    if range.start > range.end || range.end > len {
                        continue;
                    }
                    if range.len() == values.len()
                        && values
                            .iter()
                            .enumerate()
                            .all(|(i, v)| slot.list_get(range.start + i) == Some(v))
                    {
                        continue;
                    }
                    if !matches!(slot, Value::Sequence(_)) {
                        *slot = slot.clone().persistent();
                    }
                    if let Value::Sequence(items) = slot {
                        if range.len() == values.len() {
                            for (offset, value) in values.iter().enumerate() {
                                items.set(range.start + offset, value.clone());
                            }
                        } else {
                            let suffix = items.split_off(range.end);
                            items.truncate(range.start);
                            items.extend(values.iter().cloned());
                            items.append(suffix);
                        }
                    }
                    if let Some(version) = self.versions.get_mut(id.0 as usize) {
                        *version = version.wrapping_add(1);
                    }
                    changed += 1;
                    if !changed_slots.contains(&id) {
                        changed_slots.push(id);
                    }
                    edits.push(Patch::Splice(id, range, values));
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
            styles: Vec::with_capacity(new_names.len()),
            values: Vec::with_capacity(new_names.len()),
            versions: Vec::with_capacity(new_names.len()),
            generation: self.generation + 1,
        };
        for (ni, name) in new_names.iter().enumerate() {
            match old.get(name.as_str()) {
                Some(&oi) => {
                    t.values
                        .push(self.values.get(oi).cloned().unwrap_or_default());
                    t.styles
                        .push(self.styles.get(oi).copied().unwrap_or_default());
                    t.versions.push(self.versions.get(oi).copied().unwrap_or(0));
                }
                None => {
                    t.values.push(
                        new_defaults
                            .get(ni)
                            .cloned()
                            .unwrap_or_default()
                            .persistent(),
                    );
                    t.styles
                        .push(style(t.values.last().unwrap_or(&Value::Null)));
                    t.versions.push(1);
                }
            }
        }
        t
    }
}

fn style(
    value: &Value,
) -> (
    Option<guiraffe_core::Size>,
    Option<guiraffe_core::ColorName>,
) {
    match value.as_str() {
        Some(s) => (
            crate::template::parse_size(s),
            guiraffe_core::ColorName::from_name(s),
        ),
        None => (None, None),
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
    #[test]
    fn all_small_splices_match_a_vec_oracle_without_touching_invalid_ranges() {
        for len in 0..8 {
            for start in 0..=len + 1 {
                for end in 0..=len + 1 {
                    for insert in 0..4 {
                        let initial: Vec<Value> = (0..len).map(|i| Value::from(i as i64)).collect();
                        let values: Arc<[Value]> =
                            (0..insert).map(|i| Value::from(100 + i as i64)).collect();
                        let mut expected = initial.clone();
                        if start <= end && end <= len {
                            expected.splice(start..end, values.iter().cloned());
                        }
                        let mut table =
                            SlotTable::with_defaults(&[Value::List(initial.clone().into())]);
                        let mut out = PatchBuf::default();
                        out.splice(SlotId(0), start..end, values);
                        let changed = usize::from(initial != expected);
                        assert_eq!(
                            table.apply(&mut out),
                            changed,
                            "len={len}, range={start}..{end}, insert={insert}"
                        );
                        assert_eq!(table.get(SlotId(0)), &Value::List(expected.into()));
                        assert_eq!(table.version(SlotId(0)), changed as u64);
                    }
                }
            }
        }
    }
}
