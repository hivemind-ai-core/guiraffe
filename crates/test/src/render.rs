//! Render counting: prove that an interaction re-renders only what it should
//! (conventions §6.8: every Control has a render-count test).

use std::{cell::Cell, rc::Rc};

/// A cheap, clonable counter. Call [`RenderCounter::hit`] at the top of a view's `render`.
#[derive(Clone, Default, Debug)]
pub struct RenderCounter(Rc<Cell<usize>>);

impl RenderCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hit(&self) {
        self.0.set(self.0.get() + 1);
    }

    pub fn get(&self) -> usize {
        self.0.get()
    }

    /// Renders that happened while running `f`.
    pub fn during<R>(&self, f: impl FnOnce() -> R) -> (R, usize) {
        let before = self.get();
        let r = f();
        (r, self.get().saturating_sub(before))
    }
}

/// A set of named counters, for asserting a whole tree at once.
#[derive(Clone, Default, Debug)]
pub struct RenderLedger(Rc<std::cell::RefCell<Vec<(&'static str, RenderCounter)>>>);

impl RenderLedger {
    pub fn counter(&self, name: &'static str) -> RenderCounter {
        let c = RenderCounter::new();
        self.0.borrow_mut().push((name, c.clone()));
        c
    }

    pub fn snapshot(&self) -> Vec<(&'static str, usize)> {
        self.0.borrow().iter().map(|(n, c)| (*n, c.get())).collect()
    }

    /// Names whose counters changed between two snapshots.
    pub fn changed(
        before: &[(&'static str, usize)],
        after: &[(&'static str, usize)],
    ) -> Vec<&'static str> {
        after
            .iter()
            .filter(|(n, v)| before.iter().find(|(m, _)| m == n).map(|(_, b)| b) != Some(v))
            .map(|(n, _)| *n)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_diffs() {
        let ledger = RenderLedger::default();
        let (a, b) = (ledger.counter("a"), ledger.counter("b"));
        let before = ledger.snapshot();
        let ((), n) = a.during(|| {
            a.hit();
            a.hit();
        });
        assert_eq!(n, 2);
        b.get();
        assert_eq!(
            RenderLedger::changed(&before, &ledger.snapshot()),
            vec!["a"]
        );
    }
}
