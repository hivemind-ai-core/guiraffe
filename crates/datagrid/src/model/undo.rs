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

/// How much undo history to keep: a step count and an optional weight (approximate bytes).
///
/// Mirrors `guiraffe_state::HistoryLimits` (same names) for this GPUI-free model; every
/// GUIraffe undo stack is configured the same way. The log keeps at least one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HistoryLimits {
    /// Most transactions kept.
    pub max_steps: usize,
    /// Most total transaction weight kept, or no weight limit.
    pub max_weight: Option<u64>,
}

impl HistoryLimits {
    /// Keep up to `max_steps` transactions, with no weight limit.
    pub const fn steps(max_steps: usize) -> Self {
        Self {
            max_steps,
            max_weight: None,
        }
    }

    /// Also cap the total weight of kept transactions.
    pub const fn with_max_weight(self, max_weight: u64) -> Self {
        Self {
            max_weight: Some(max_weight),
            ..self
        }
    }
}

impl From<usize> for HistoryLimits {
    fn from(max_steps: usize) -> Self {
        Self::steps(max_steps)
    }
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self::steps(UNDO_LIMIT)
    }
}

/// Weighs a transaction once, when it is recorded.
pub type Weigher<K, V, L> = fn(&Transaction<K, V, L>) -> u64;

/// The default weight: the transaction and its change records, ignoring heap data the
/// values own. Supply a [`Weigher`] that counts it (e.g. string lengths) when it matters.
pub fn estimate_weight<K, V, L>(t: &Transaction<K, V, L>) -> u64 {
    (size_of::<Transaction<K, V, L>>() + t.changes.len() * size_of::<Change<K, V>>()) as u64
}

/// Bounded undo and redo stacks. Over a limit, the oldest transactions are dropped first; the
/// newest is always kept, even when it alone is heavier than the weight limit.
#[derive(Debug, Clone)]
pub struct UndoLog<K, V, L> {
    done: VecDeque<Transaction<K, V, L>>,
    done_weight: VecDeque<u64>,
    undone: Vec<(Transaction<K, V, L>, u64)>,
    weight: u64,
    limits: HistoryLimits,
    weigh: Weigher<K, V, L>,
}

/// Default number of transactions kept.
pub const UNDO_LIMIT: usize = 200;

impl<K, V: PartialEq, L> Default for UndoLog<K, V, L> {
    fn default() -> Self {
        Self::new(UNDO_LIMIT)
    }
}

impl<K, V: PartialEq, L> UndoLog<K, V, L> {
    /// A log within `limits` (a count converts; at least one transaction is kept), weighing
    /// transactions with [`estimate_weight`].
    pub fn new(limits: impl Into<HistoryLimits>) -> Self {
        let mut limits = limits.into();
        limits.max_steps = limits.max_steps.max(1);
        Self {
            done: VecDeque::new(),
            done_weight: VecDeque::new(),
            undone: Vec::new(),
            weight: 0,
            limits,
            weigh: estimate_weight,
        }
    }

    /// Weigh new transactions with `weigh` instead of [`estimate_weight`].
    pub fn with_weigher(mut self, weigh: Weigher<K, V, L>) -> Self {
        self.weigh = weigh;
        self
    }

    pub fn limits(&self) -> HistoryLimits {
        self.limits
    }

    /// Change the limits, dropping the oldest transactions that no longer fit.
    pub fn set_limits(&mut self, limits: impl Into<HistoryLimits>) {
        let mut limits = limits.into();
        limits.max_steps = limits.max_steps.max(1);
        self.limits = limits;
        self.trim();
    }

    /// Total weight of the transactions available to undo.
    pub fn weight(&self) -> u64 {
        self.weight
    }

    /// Record a finished action. No-op changes (`old == new`) are dropped; an empty
    /// transaction is not recorded. Clears the redo stack. Returns whether it was recorded.
    pub fn push(&mut self, label: L, mut changes: Vec<Change<K, V>>) -> bool {
        changes.retain(|c| c.old != c.new);
        if changes.is_empty() {
            return false;
        }
        self.undone.clear();
        let t = Transaction { label, changes };
        let w = (self.weigh)(&t);
        self.push_done(t, w);
        true
    }

    fn push_done(&mut self, t: Transaction<K, V, L>, w: u64) {
        self.done.push_back(t);
        self.done_weight.push_back(w);
        self.weight = self.weight.saturating_add(w);
        self.trim();
    }

    fn trim(&mut self) {
        while self.done.len() > 1
            && (self.done.len() > self.limits.max_steps
                || self.limits.max_weight.is_some_and(|m| self.weight > m))
        {
            self.done.pop_front();
            if let Some(w) = self.done_weight.pop_front() {
                self.weight -= w;
            }
        }
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
        let w = self.done_weight.pop_back().unwrap_or(0);
        self.weight -= w;
        self.undone.push((t, w));
        self.undone.last().map(|(t, _)| t)
    }

    /// Move the newest undone transaction back and return it; the caller writes
    /// [`Transaction::redo_steps`].
    pub fn redo(&mut self) -> Option<&Transaction<K, V, L>> {
        let (t, w) = self.undone.pop()?;
        self.push_done(t, w);
        self.done.back()
    }

    /// The transaction [`UndoLog::undo`] would return, without moving it (apply first, then
    /// move, so a refused restore leaves the stacks untouched).
    pub fn peek_undo(&self) -> Option<&Transaction<K, V, L>> {
        self.done.back()
    }

    /// The transaction [`UndoLog::redo`] would return, without moving it.
    pub fn peek_redo(&self) -> Option<&Transaction<K, V, L>> {
        self.undone.last().map(|(t, _)| t)
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.done_weight.clear();
        self.weight = 0;
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

    fn one(key: u8, n: usize) -> Vec<Change<u8, u8>> {
        (0..n)
            .map(|i| Change {
                key: key.wrapping_add(i as u8),
                old: 0,
                new: 1,
            })
            .collect()
    }

    fn labels(log: &mut UndoLog<u8, u8, u8>) -> Vec<u8> {
        let mut seen = Vec::new();
        while let Some(t) = log.undo() {
            seen.push(t.label);
        }
        seen
    }

    #[test]
    fn weight_limit_drops_oldest_transactions() {
        // Weigh one per change: a cap of 5 keeps the newest transactions totalling <= 5.
        let mut log = UndoLog::new(HistoryLimits::steps(100).with_max_weight(5))
            .with_weigher(|t: &Transaction<u8, u8, u8>| t.changes.len() as u64);
        log.push(0, one(0, 2));
        log.push(1, one(0, 2));
        log.push(2, one(0, 1));
        assert_eq!(log.weight(), 5, "exactly at the cap is kept");
        log.push(3, one(0, 2));
        assert_eq!((log.len(), log.weight()), (3, 5));
        assert_eq!(labels(&mut log), [3, 2, 1]);
    }

    #[test]
    fn a_transaction_heavier_than_the_cap_is_kept_alone_and_stays_whole() {
        // A 10-cell paste is one transaction, so it straddles a cap of 4 without splitting.
        let mut log = UndoLog::new(HistoryLimits::steps(100).with_max_weight(4))
            .with_weigher(|t: &Transaction<u8, u8, u8>| t.changes.len() as u64);
        log.push(0, one(0, 1));
        log.push(1, one(0, 10));
        assert_eq!((log.len(), log.weight()), (1, 10));
        assert_eq!(log.peek_undo().map(|t| t.changes.len()), Some(10));
        // Undo then redo keeps the weight accounting.
        assert!(log.undo().is_some());
        assert_eq!(log.weight(), 0);
        assert!(log.redo().is_some());
        assert_eq!(log.weight(), 10);
        log.push(2, one(0, 1));
        assert_eq!(labels(&mut log), [2]);
    }

    #[test]
    fn default_weight_estimates_change_records_and_limits_reconfigure() {
        let mut log: UndoLog<u8, u8, u8> = UndoLog::new(10);
        log.push(0, one(0, 3));
        let t = log.peek_undo().map(estimate_weight);
        assert_eq!(Some(log.weight()), t);
        for i in 1..6 {
            log.push(i, one(0, 1));
        }
        log.set_limits(2);
        assert_eq!(log.limits(), HistoryLimits::steps(2));
        assert_eq!(labels(&mut log), [5, 4]);
        log.set_limits(0);
        assert_eq!(log.limits().max_steps, 1, "the log keeps at least one step");
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
