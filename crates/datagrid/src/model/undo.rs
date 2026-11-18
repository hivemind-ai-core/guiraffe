//! Undo/redo as a log of transactions of minimal cell diffs (`key`, `old`, `new`), never row
//! copies. One user action (an edit, a paste, a cut, a clear) is one [`Transaction`]. No GPUI
//! dependency; shared by the grid and the sheet.

use std::collections::VecDeque;

/// One cell's change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change<K, V> {
    pub key: K,
    pub old: V,
    pub new: V,
}

/// Changes made by one user action, in application order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction<K, V, L> {
    pub label: L,
    pub changes: Vec<Change<K, V>>,
}

impl<K, V, L> Transaction<K, V, L> {
    /// What to write to undo it: `(key, old)` in reverse order (so repeated keys restore the
    /// oldest value).
    pub fn undo_steps(&self) -> impl Iterator<Item = (&K, &V)> {
        self.changes.iter().rev().map(|c| (&c.key, &c.old))
    }

    /// What to write to redo it: `(key, new)` in application order.
    pub fn redo_steps(&self) -> impl Iterator<Item = (&K, &V)> {
        self.changes.iter().map(|c| (&c.key, &c.new))
    }
}

/// Bounded undo and redo stacks.
#[derive(Debug, Clone)]
pub struct UndoLog<K, V, L> {
    done: VecDeque<Transaction<K, V, L>>,
    undone: Vec<Transaction<K, V, L>>,
    limit: usize,
}

/// Default number of transactions kept.
pub const UNDO_LIMIT: usize = 200;

impl<K, V: PartialEq, L> Default for UndoLog<K, V, L> {
    fn default() -> Self {
        Self::new(UNDO_LIMIT)
    }
}

impl<K, V: PartialEq, L> UndoLog<K, V, L> {
    /// A log keeping at most `limit` transactions (oldest dropped first).
    pub fn new(limit: usize) -> Self {
        Self {
            done: VecDeque::new(),
            undone: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Record a finished action. No-op changes (`old == new`) are dropped; an empty
    /// transaction is not recorded. Clears the redo stack. Returns whether it was recorded.
    pub fn push(&mut self, label: L, mut changes: Vec<Change<K, V>>) -> bool {
        changes.retain(|c| c.old != c.new);
        if changes.is_empty() {
            return false;
        }
        self.undone.clear();
        self.done.push_back(Transaction { label, changes });
        while self.done.len() > self.limit {
            self.done.pop_front();
        }
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Move the newest transaction to the redo stack and return it; the caller writes
    /// [`Transaction::undo_steps`].
    pub fn undo(&mut self) -> Option<&Transaction<K, V, L>> {
        let t = self.done.pop_back()?;
        self.undone.push(t);
        self.undone.last()
    }

    /// Move the newest undone transaction back and return it; the caller writes
    /// [`Transaction::redo_steps`].
    pub fn redo(&mut self) -> Option<&Transaction<K, V, L>> {
        let t = self.undone.pop()?;
        self.done.push_back(t);
        self.done.back()
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }

    /// Transactions available to undo.
    pub fn len(&self) -> usize {
        self.done.len()
    }

    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashMap;

    type Grid = HashMap<(u8, u8), u16>;

    /// Apply `writes` to `grid`, recording old values like the grid does.
    fn apply(grid: &mut Grid, writes: &[((u8, u8), u16)]) -> Vec<Change<(u8, u8), u16>> {
        writes
            .iter()
            .map(|&(k, v)| {
                let old = grid.insert(k, v).unwrap_or(0);
                Change {
                    key: k,
                    old,
                    new: v,
                }
            })
            .collect()
    }

    fn write<'a>(grid: &mut Grid, steps: impl Iterator<Item = (&'a (u8, u8), &'a u16)>) {
        for (k, v) in steps {
            grid.insert(*k, *v);
        }
    }

    fn norm(g: &Grid) -> Vec<((u8, u8), u16)> {
        let mut v: Vec<_> = g
            .iter()
            .filter(|e| *e.1 != 0)
            .map(|(k, v)| (*k, *v))
            .collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn noop_and_limit() {
        let mut log: UndoLog<u8, u8, ()> = UndoLog::new(2);
        assert!(!log.push(
            (),
            vec![Change {
                key: 0,
                old: 1,
                new: 1
            }]
        ));
        for i in 0..3 {
            assert!(log.push(
                (),
                vec![Change {
                    key: i,
                    old: 0,
                    new: 1
                }]
            ));
        }
        assert_eq!(log.len(), 2, "oldest dropped");
        assert!(log.undo().is_some());
        assert!(log.can_redo());
        log.push(
            (),
            vec![Change {
                key: 9,
                old: 0,
                new: 1,
            }],
        );
        assert!(!log.can_redo(), "a new action clears redo");
    }

    proptest! {
        /// Applying transactions then undoing them all restores the original; redoing them
        /// all restores the final state. Repeated keys inside one transaction included.
        #[test]
        fn undo_is_the_inverse(
            start in proptest::collection::vec(((0u8..4, 0u8..4), 1u16..9), 0..10),
            txs in proptest::collection::vec(proptest::collection::vec(((0u8..4, 0u8..4), 0u16..9), 1..8), 1..8),
        ) {
            let mut grid: Grid = start.into_iter().collect();
            let original = norm(&grid);
            let mut log: UndoLog<(u8, u8), u16, usize> = UndoLog::default();
            let mut states = vec![original.clone()];
            for (i, t) in txs.iter().enumerate() {
                let changes = apply(&mut grid, t);
                if log.push(i, changes) {
                    states.push(norm(&grid));
                }
            }
            let last = norm(&grid);
            while let Some(t) = log.undo() {
                let steps: Vec<_> = t.undo_steps().map(|(k, v)| (*k, *v)).collect();
                write(&mut grid, steps.iter().map(|(k, v)| (k, v)));
                states.pop();
                prop_assert_eq!(Some(&norm(&grid)), states.last());
            }
            prop_assert_eq!(norm(&grid), original);
            while let Some(t) = log.redo() {
                let steps: Vec<_> = t.redo_steps().map(|(k, v)| (*k, *v)).collect();
                write(&mut grid, steps.iter().map(|(k, v)| (k, v)));
            }
            prop_assert_eq!(norm(&grid), last);
        }
    }
}
