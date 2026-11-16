//! Pure navigation history (one per window).

#[derive(Debug, Clone, PartialEq)]
pub struct History<R> {
    entries: Vec<R>,
    index: usize,
    cap: usize,
}

impl<R: Clone + PartialEq> History<R> {
    /// A history starting at `initial`, keeping at most `cap` entries (minimum 1).
    pub fn new(initial: R, cap: usize) -> Self {
        Self {
            entries: vec![initial],
            index: 0,
            cap: cap.max(1),
        }
    }

    pub fn current(&self) -> &R {
        // Invariant: entries is never empty and index < len.
        &self.entries[self.index]
    }

    pub fn entries(&self) -> &[R] {
        &self.entries
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Push a new entry, dropping any forward history. Pushing the current route is a no-op.
    pub fn push(&mut self, route: R) -> bool {
        if *self.current() == route {
            return false;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(route);
        if self.entries.len() > self.cap {
            let excess = self.entries.len() - self.cap;
            self.entries.drain(..excess);
        }
        self.index = self.entries.len() - 1;
        true
    }

    /// Replace the current entry (redirects, search-param edits).
    pub fn replace(&mut self, route: R) -> bool {
        if *self.current() == route {
            return false;
        }
        self.entries[self.index] = route;
        true
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    pub fn back(&mut self) -> bool {
        let ok = self.can_back();
        if ok {
            self.index -= 1;
        }
        ok
    }

    pub fn forward(&mut self) -> bool {
        let ok = self.can_forward();
        if ok {
            self.index += 1;
        }
        ok
    }

    /// The route `delta` steps away (negative = back), if any.
    pub fn peek(&self, delta: isize) -> Option<&R> {
        let i = self.index.checked_add_signed(delta)?;
        self.entries.get(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_back_forward_and_truncation() {
        let mut h = History::new("a", 10);
        assert!(h.push("b"));
        assert!(h.push("c"));
        assert!(!h.push("c"), "same route is a no-op");
        assert!(h.back());
        assert_eq!(*h.current(), "b");
        assert!(h.push("d"), "pushing drops forward history");
        assert!(!h.can_forward());
        assert_eq!(h.entries(), &["a", "b", "d"]);
        assert!(h.back() && h.back() && !h.back());
        assert_eq!(*h.current(), "a");
        assert_eq!(h.peek(2), Some(&"d"));
        assert_eq!(h.peek(-1), None);
    }

    #[test]
    fn cap_drops_oldest_and_keeps_index_valid() {
        let mut h = History::new(0, 3);
        for i in 1..10 {
            h.push(i);
        }
        assert_eq!(h.entries(), &[7, 8, 9]);
        assert_eq!((*h.current(), h.index()), (9, 2));
        let mut one = History::new(0, 0);
        one.push(1);
        assert_eq!(one.entries(), &[1], "cap clamps to 1");
        assert!(!one.can_back());
    }

    #[test]
    fn replace_keeps_position() {
        let mut h = History::new("a", 10);
        h.push("b");
        assert!(h.replace("B"));
        assert_eq!(h.entries(), &["a", "B"]);
        assert!(!h.replace("B"));
    }
}
