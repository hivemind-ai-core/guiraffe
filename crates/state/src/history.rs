//! Bounded undo history: a count limit and an optional weight (approximate byte) limit.
//!
//! Each entry carries a weight computed **once**, when it is recorded (provided by a weigher
//! or estimated from the snapshot's size), never per frame. When either limit is exceeded the
//! oldest entries are dropped first. The newest entry is always kept, even when it alone is
//! heavier than the weight limit, so the last change can still be undone.
//!
//! Grouping: between [`Engine::begin_undo_group`](crate::Engine::begin_undo_group) and
//! [`Engine::end_undo_group`](crate::Engine::end_undo_group) only the first change records an
//! entry (the state before the group), so a group is one step and is dropped as a unit.

use std::collections::VecDeque;

/// How much undo history to keep. `max_steps == 0` disables history.
///
/// The same type and field names are used by every GUIraffe undo stack (store history, the
/// datagrid and sheet undo log, the flow editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HistoryLimits {
    /// Most undo steps kept.
    pub max_steps: usize,
    /// Most total entry weight kept (approximate bytes), or no weight limit.
    pub max_weight: Option<u64>,
}

impl HistoryLimits {
    /// History disabled.
    pub const DISABLED: Self = Self::steps(0);

    /// Keep up to `max_steps` steps, with no weight limit.
    pub const fn steps(max_steps: usize) -> Self {
        Self {
            max_steps,
            max_weight: None,
        }
    }

    /// Also cap the total weight of kept entries.
    pub const fn with_max_weight(self, max_weight: u64) -> Self {
        Self {
            max_weight: Some(max_weight),
            ..self
        }
    }

    pub const fn is_enabled(&self) -> bool {
        self.max_steps > 0
    }
}

impl From<usize> for HistoryLimits {
    fn from(max_steps: usize) -> Self {
        Self::steps(max_steps)
    }
}

/// A deque of weighted entries kept within [`HistoryLimits`], oldest at the front.
#[derive(Debug, Clone)]
pub struct BoundedHistory<T> {
    entries: VecDeque<(T, u64)>,
    weight: u64,
    limits: HistoryLimits,
}

impl<T> BoundedHistory<T> {
    pub fn new(limits: HistoryLimits) -> Self {
        Self {
            entries: VecDeque::new(),
            weight: 0,
            limits,
        }
    }

    pub fn limits(&self) -> HistoryLimits {
        self.limits
    }

    /// Change the limits, dropping the oldest entries that no longer fit.
    pub fn set_limits(&mut self, limits: HistoryLimits) {
        self.limits = limits;
        if !limits.is_enabled() {
            self.clear();
        }
        self.trim();
    }

    /// Record `entry` as the newest, then drop the oldest entries over a limit. Returns how
    /// many were dropped. A no-op when history is disabled.
    pub fn push(&mut self, entry: T, weight: u64) -> usize {
        if !self.limits.is_enabled() {
            return 0;
        }
        self.entries.push_back((entry, weight));
        self.weight = self.weight.saturating_add(weight);
        self.trim()
    }

    fn trim(&mut self) -> usize {
        let mut dropped = 0;
        while self.entries.len() > 1 && self.over_limit() {
            if let Some((_, w)) = self.entries.pop_front() {
                self.weight -= w;
                dropped += 1;
            }
        }
        dropped
    }

    fn over_limit(&self) -> bool {
        self.entries.len() > self.limits.max_steps
            || self.limits.max_weight.is_some_and(|max| self.weight > max)
    }

    /// Remove the newest entry.
    pub fn pop(&mut self) -> Option<T> {
        let (entry, w) = self.entries.pop_back()?;
        self.weight -= w;
        Some(entry)
    }

    pub fn newest(&self) -> Option<&T> {
        self.entries.back().map(|(e, _)| e)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.weight = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total weight of the kept entries.
    pub fn weight(&self) -> u64 {
        self.weight
    }

    /// Entries oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|(e, _)| e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(h: &BoundedHistory<u32>) -> Vec<u32> {
        h.iter().copied().collect()
    }

    #[test]
    fn step_limit_drops_oldest_first() {
        let mut h = BoundedHistory::new(HistoryLimits::steps(2));
        for i in 0..4 {
            h.push(i, 1);
        }
        assert_eq!(kept(&h), [2, 3]);
        assert_eq!(h.weight(), 2);
    }

    #[test]
    fn weight_limit_drops_oldest_first() {
        let mut h = BoundedHistory::new(HistoryLimits::steps(100).with_max_weight(10));
        assert_eq!(h.push(0, 4), 0);
        assert_eq!(h.push(1, 4), 0);
        // 4 + 4 + 4 > 10: the oldest goes.
        assert_eq!(h.push(2, 4), 1);
        assert_eq!(kept(&h), [1, 2]);
        assert_eq!(h.weight(), 8);
    }

    #[test]
    fn weight_exactly_at_the_cap_is_kept() {
        let mut h = BoundedHistory::new(HistoryLimits::steps(100).with_max_weight(10));
        h.push(0, 6);
        h.push(1, 4);
        assert_eq!(kept(&h), [0, 1]);
        assert_eq!(h.weight(), 10);
    }

    #[test]
    fn an_entry_heavier_than_the_cap_is_kept_alone() {
        let mut h = BoundedHistory::new(HistoryLimits::steps(100).with_max_weight(10));
        h.push(0, 3);
        h.push(1, 3);
        assert_eq!(h.push(2, 50), 2);
        assert_eq!(kept(&h), [2]);
        // The next light entry evicts the heavy one.
        h.push(3, 3);
        assert_eq!(kept(&h), [3]);
        assert_eq!(h.weight(), 3);
    }

    #[test]
    fn both_limits_apply_whichever_binds_first() {
        let mut h = BoundedHistory::new(HistoryLimits::steps(3).with_max_weight(100));
        for i in 0..5 {
            h.push(i, 1);
        }
        assert_eq!(kept(&h), [2, 3, 4]);
        h.set_limits(HistoryLimits::steps(3).with_max_weight(2));
        assert_eq!(kept(&h), [3, 4]);
    }

    #[test]
    fn pop_and_clear_keep_the_weight_in_step() {
        let mut h = BoundedHistory::new(HistoryLimits::steps(5).with_max_weight(100));
        h.push(0, 7);
        h.push(1, 5);
        assert_eq!(h.pop(), Some(1));
        assert_eq!(h.weight(), 7);
        h.clear();
        assert_eq!(h.weight(), 0);
        assert!(h.is_empty());
    }

    #[test]
    fn disabled_history_records_nothing() {
        let mut h = BoundedHistory::new(HistoryLimits::DISABLED);
        assert_eq!(h.push(0, 1), 0);
        assert!(h.is_empty());
        let mut h = BoundedHistory::new(HistoryLimits::steps(3));
        h.push(0, 1);
        h.set_limits(HistoryLimits::DISABLED);
        assert!(h.is_empty());
    }

    #[test]
    fn usize_converts_to_a_step_limit() {
        assert_eq!(HistoryLimits::from(7), HistoryLimits::steps(7));
    }
}
