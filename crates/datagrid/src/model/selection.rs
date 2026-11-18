//! The selection model and keyboard navigation, in view coordinates (view row, display col).
//!
//! Row selections are stored as sorted disjoint ranges, so "select all" on a million rows is
//! one range, not a million entries.

use std::ops::Range;

/// What clicks and keys select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum SelectionMode {
    /// Nothing is selected (a cursor still moves for keyboard navigation).
    None,
    #[default]
    SingleRow,
    /// Ctrl-click toggles, Shift-click extends.
    MultiRow,
    /// One cell.
    Cell,
    /// A rectangular cell range (Shift extends).
    Range,
}

/// A view-coordinate cell: view row, display column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct CellPos {
    pub row: usize,
    pub col: usize,
}

impl CellPos {
    pub const fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

/// Keyboard movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    /// First column of the row.
    Home,
    /// Last column of the row.
    End,
    /// First row, first column (Ctrl+Home).
    First,
    /// Last row, last column (Ctrl+End).
    Last,
}

impl Nav {
    /// Where the cursor goes from `from` in a `rows` × `cols` grid with `page` rows per page.
    pub fn apply(self, from: CellPos, rows: usize, cols: usize, page: usize) -> CellPos {
        let (lr, lc) = (rows.saturating_sub(1), cols.saturating_sub(1));
        let r = from.row.min(lr);
        let c = from.col.min(lc);
        let page = page.max(1);
        match self {
            Nav::Up => CellPos::new(r.saturating_sub(1), c),
            Nav::Down => CellPos::new((r + 1).min(lr), c),
            Nav::Left => CellPos::new(r, c.saturating_sub(1)),
            Nav::Right => CellPos::new(r, (c + 1).min(lc)),
            Nav::PageUp => CellPos::new(r.saturating_sub(page), c),
            Nav::PageDown => CellPos::new(r.saturating_add(page).min(lr), c),
            Nav::Home => CellPos::new(r, 0),
            Nav::End => CellPos::new(r, lc),
            Nav::First => CellPos::new(0, 0),
            Nav::Last => CellPos::new(lr, lc),
        }
    }
}

/// Commit-and-move direction after an edit: Enter / Shift+Enter move down / up, Tab /
/// Shift+Tab move right / left and wrap to the next / previous row at the row ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Advance {
    Down,
    Up,
    Right,
    Left,
}

impl Advance {
    /// The cell after `from` in a `rows` × `cols` grid, or `None` when the move would leave
    /// the grid (first/last cell for Tab, first/last row for Enter), so the caller can let
    /// focus move on.
    pub fn apply(self, from: CellPos, rows: usize, cols: usize) -> Option<CellPos> {
        if rows == 0 || cols == 0 {
            return None;
        }
        let r = from.row.min(rows - 1);
        let c = from.col.min(cols - 1);
        match self {
            Advance::Down => (r + 1 < rows).then(|| CellPos::new(r + 1, c)),
            Advance::Up => r.checked_sub(1).map(|r| CellPos::new(r, c)),
            Advance::Right if c + 1 < cols => Some(CellPos::new(r, c + 1)),
            Advance::Right => (r + 1 < rows).then(|| CellPos::new(r + 1, 0)),
            Advance::Left if c > 0 => Some(CellPos::new(r, c - 1)),
            Advance::Left => r.checked_sub(1).map(|r| CellPos::new(r, cols - 1)),
        }
    }
}

/// Sorted, disjoint, non-adjacent row ranges.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowSet(Vec<Range<usize>>);

impl RowSet {
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.0
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.iter().map(|r| r.len()).sum()
    }
    pub fn contains(&self, row: usize) -> bool {
        let i = self.0.partition_point(|r| r.end <= row);
        self.0.get(i).is_some_and(|r| r.start <= row)
    }
    /// Union with `range`, merging touching ranges.
    pub fn insert(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let (mut s, mut e) = (range.start, range.end);
        let first = self.0.partition_point(|r| r.end < s);
        let mut last = first;
        while let Some(r) = self.0.get(last) {
            if r.start > e {
                break;
            }
            s = s.min(r.start);
            e = e.max(r.end);
            last += 1;
        }
        self.0.splice(first..last, std::iter::once(s..e));
    }
    pub fn remove(&mut self, row: usize) {
        let i = self.0.partition_point(|r| r.end <= row);
        let Some(r) = self.0.get(i).cloned() else {
            return;
        };
        if r.start > row {
            return;
        }
        let mut parts = Vec::with_capacity(2);
        if r.start < row {
            parts.push(r.start..row);
        }
        if row + 1 < r.end {
            parts.push(row + 1..r.end);
        }
        self.0.splice(i..=i, parts);
    }
    pub fn toggle(&mut self, row: usize) {
        if self.contains(row) {
            self.remove(row);
        } else {
            self.insert(row..row + 1);
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().flat_map(|r| r.clone())
    }
}

/// Current selection plus keyboard cursor and anchor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    pub mode: SelectionMode,
    pub cursor: Option<CellPos>,
    pub anchor: Option<CellPos>,
    pub rows: RowSet,
}

/// The cells a copy covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Region {
    /// Whole rows (all display columns).
    Rows(Vec<Range<usize>>),
    /// A rectangle: rows × display columns.
    Rect(Range<usize>, Range<usize>),
}

impl Selection {
    pub fn new(mode: SelectionMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.cursor = None;
        self.anchor = None;
    }

    /// Pointer press on `pos` with Shift / Ctrl(Cmd) state.
    pub fn click(&mut self, pos: CellPos, shift: bool, ctrl: bool) {
        match self.mode {
            SelectionMode::None => {
                self.cursor = Some(pos);
                self.anchor = Some(pos);
            }
            SelectionMode::SingleRow => self.select_row(pos),
            SelectionMode::MultiRow => {
                if shift && let Some(a) = self.anchor {
                    if !ctrl {
                        self.rows.clear();
                    }
                    self.rows.insert(a.row.min(pos.row)..a.row.max(pos.row) + 1);
                    self.cursor = Some(pos);
                } else if ctrl {
                    self.rows.toggle(pos.row);
                    self.cursor = Some(pos);
                    self.anchor = Some(pos);
                } else {
                    self.select_row(pos);
                }
            }
            SelectionMode::Cell => {
                self.cursor = Some(pos);
                self.anchor = Some(pos);
            }
            SelectionMode::Range => {
                self.cursor = Some(pos);
                if !(shift && self.anchor.is_some()) {
                    self.anchor = Some(pos);
                }
            }
        }
    }

    fn select_row(&mut self, pos: CellPos) {
        self.rows.clear();
        self.rows.insert(pos.row..pos.row + 1);
        self.cursor = Some(pos);
        self.anchor = Some(pos);
    }

    /// Keyboard move; `extend` is Shift. Returns the new cursor.
    pub fn navigate(
        &mut self,
        nav: Nav,
        extend: bool,
        rows: usize,
        cols: usize,
        page: usize,
    ) -> Option<CellPos> {
        if rows == 0 || cols == 0 {
            return None;
        }
        let to = match self.cursor {
            Some(c) => nav.apply(c, rows, cols, page),
            None => CellPos::new(0, 0),
        };
        if extend && matches!(self.mode, SelectionMode::MultiRow | SelectionMode::Range) {
            let a = self.anchor.unwrap_or(to);
            self.anchor = Some(a);
            self.cursor = Some(to);
            if self.mode == SelectionMode::MultiRow {
                self.rows.clear();
                self.rows.insert(a.row.min(to.row)..a.row.max(to.row) + 1);
            }
        } else {
            self.click(to, false, false);
        }
        Some(to)
    }

    /// Ctrl+A. Returns whether anything changed.
    pub fn select_all(&mut self, rows: usize, cols: usize) -> bool {
        if rows == 0 || cols == 0 {
            return false;
        }
        let before = self.clone();
        match self.mode {
            SelectionMode::MultiRow => {
                self.rows.clear();
                self.rows.insert(0..rows);
            }
            SelectionMode::Range => {
                self.anchor = Some(CellPos::new(0, 0));
                self.cursor = Some(CellPos::new(rows - 1, cols - 1));
            }
            _ => {}
        }
        *self != before
    }

    /// Rows and columns of the cell rectangle (cell and range modes).
    pub fn rect(&self) -> Option<(Range<usize>, Range<usize>)> {
        match self.mode {
            SelectionMode::Cell => self.cursor.map(|c| (c.row..c.row + 1, c.col..c.col + 1)),
            SelectionMode::Range => {
                let c = self.cursor?;
                let a = self.anchor.unwrap_or(c);
                Some((
                    a.row.min(c.row)..a.row.max(c.row) + 1,
                    a.col.min(c.col)..a.col.max(c.col) + 1,
                ))
            }
            _ => None,
        }
    }

    pub fn is_row_selected(&self, row: usize) -> bool {
        matches!(
            self.mode,
            SelectionMode::SingleRow | SelectionMode::MultiRow
        ) && self.rows.contains(row)
    }

    pub fn is_cell_selected(&self, row: usize, col: usize) -> bool {
        match self.rect() {
            Some((r, c)) => r.contains(&row) && c.contains(&col),
            None => self.is_row_selected(row),
        }
    }

    /// The region Ctrl+C copies (cursor cell when nothing else is selected).
    pub fn region(&self) -> Option<Region> {
        if let Some((r, c)) = self.rect() {
            return Some(Region::Rect(r, c));
        }
        if !self.rows.is_empty() {
            return Some(Region::Rows(self.rows.ranges().to_vec()));
        }
        self.cursor
            .map(|c| Region::Rect(c.row..c.row + 1, c.col..c.col + 1))
    }

    /// Move the selection to new view rows after the row order changed: `map(old_view_row)`
    /// gives the new view row of the same source row, or `None` when it was filtered out.
    /// Cursor and anchor follow their rows; selected rows follow individually.
    pub fn remap(&self, map: impl Fn(usize) -> Option<usize>) -> Selection {
        let pos =
            |p: Option<CellPos>| p.and_then(|p| map(p.row).map(|row| CellPos::new(row, p.col)));
        let mut rows: Vec<usize> = self.rows.iter().filter_map(&map).collect();
        rows.sort_unstable();
        let mut set = RowSet::default();
        let mut run: Option<Range<usize>> = None;
        for r in rows {
            run = match run {
                Some(cur) if cur.end == r => Some(cur.start..r + 1),
                Some(cur) => {
                    set.0.push(cur);
                    Some(r..r + 1)
                }
                None => Some(r..r + 1),
            };
        }
        if let Some(cur) = run {
            set.0.push(cur);
        }
        Selection {
            mode: self.mode,
            cursor: pos(self.cursor),
            anchor: pos(self.anchor),
            rows: set,
        }
    }

    /// Whether anything is selected or a cursor exists.
    pub fn is_empty(&self) -> bool {
        self.cursor.is_none() && self.anchor.is_none() && self.rows.is_empty()
    }

    /// Drop everything at or past `rows` (after filtering shrinks the view).
    pub fn clamp(&mut self, rows: usize, cols: usize) {
        let ok = |p: &CellPos| p.row < rows && p.col < cols;
        if !self.cursor.as_ref().is_none_or(ok) {
            self.cursor = None;
        }
        if !self.anchor.as_ref().is_none_or(ok) {
            self.anchor = None;
        }
        let kept: Vec<_> = self
            .rows
            .ranges()
            .iter()
            .filter(|r| r.start < rows)
            .map(|r| r.start..r.end.min(rows))
            .collect();
        self.rows = RowSet(kept);
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn multi_row_clicks() {
        let mut s = Selection::new(SelectionMode::MultiRow);
        s.click(CellPos::new(3, 0), false, false);
        s.click(CellPos::new(6, 1), true, false);
        assert_eq!(s.rows.ranges(), &[3..7][..]);
        s.click(CellPos::new(5, 0), false, true);
        assert_eq!(s.rows.ranges(), [3..5, 6..7]);
        s.click(CellPos::new(9, 0), true, true);
        assert_eq!(s.rows.ranges(), &[3..10][..]);
        s.click(CellPos::new(1, 0), false, false);
        assert_eq!(s.rows.ranges(), &[1..2][..]);
        assert!(s.is_row_selected(1) && !s.is_row_selected(2));
    }

    #[test]
    fn keyboard_navigation() {
        let mut s = Selection::new(SelectionMode::Range);
        assert_eq!(
            s.navigate(Nav::Down, false, 100, 5, 10),
            Some(CellPos::new(0, 0))
        );
        s.navigate(Nav::PageDown, false, 100, 5, 10);
        s.navigate(Nav::End, true, 100, 5, 10);
        s.navigate(Nav::Down, true, 100, 5, 10);
        assert_eq!(s.rect(), Some((10..12, 0..5)));
        s.navigate(Nav::Last, false, 100, 5, 10);
        assert_eq!(s.cursor, Some(CellPos::new(99, 4)));
        s.navigate(Nav::First, false, 100, 5, 10);
        assert_eq!(s.rect(), Some((0..1, 0..1)));
        assert!(s.select_all(100, 5));
        assert_eq!(s.region(), Some(Region::Rect(0..100, 0..5)));
        let mut m = Selection::new(SelectionMode::MultiRow);
        assert!(m.select_all(1_000_000, 3));
        assert_eq!(m.rows.len(), 1_000_000);
        assert_eq!(m.rows.ranges().len(), 1);
    }

    #[test]
    fn single_row_extends_nothing() {
        let mut s = Selection::new(SelectionMode::SingleRow);
        s.navigate(Nav::Down, false, 10, 2, 3);
        s.navigate(Nav::Down, true, 10, 2, 3);
        assert_eq!(s.rows.ranges(), &[1..2][..]);
        assert!(!s.select_all(10, 2));
    }

    #[test]
    fn remap_follows_rows() {
        let mut s = Selection::new(SelectionMode::MultiRow);
        s.rows.insert(1..3);
        s.rows.insert(5..6);
        s.cursor = Some(CellPos::new(5, 2));
        // Reverse 0..10, drop old row 2.
        let m = s.remap(|r| (r != 2).then_some(9 - r));
        assert_eq!(m.rows.ranges(), &[4..5, 8..9][..]);
        assert_eq!(m.cursor, Some(CellPos::new(4, 2)));
    }

    #[test]
    fn clamp_after_filter() {
        let mut s = Selection::new(SelectionMode::MultiRow);
        s.rows.insert(2..8);
        s.cursor = Some(CellPos::new(7, 0));
        s.clamp(5, 3);
        assert_eq!(s.rows.ranges(), &[2..5][..]);
        assert_eq!(s.cursor, None);
    }

    #[test]
    fn advance_wraps_at_row_ends() {
        let p = CellPos::new;
        assert_eq!(Advance::Right.apply(p(0, 1), 3, 3), Some(p(0, 2)));
        assert_eq!(Advance::Right.apply(p(0, 2), 3, 3), Some(p(1, 0)));
        assert_eq!(Advance::Left.apply(p(1, 0), 3, 3), Some(p(0, 2)));
        assert_eq!(Advance::Left.apply(p(0, 0), 3, 3), None);
        assert_eq!(Advance::Right.apply(p(2, 2), 3, 3), None);
        assert_eq!(Advance::Down.apply(p(2, 1), 3, 3), None);
        assert_eq!(Advance::Up.apply(p(1, 1), 3, 3), Some(p(0, 1)));
        assert_eq!(Advance::Down.apply(p(0, 0), 0, 3), None);
    }

    proptest! {
        #[test]
        fn advance_right_then_left_returns(r in 0usize..20, c in 0usize..20, rows in 1usize..20, cols in 1usize..20) {
            let from = CellPos::new(r.min(rows - 1), c.min(cols - 1));
            if let Some(to) = Advance::Right.apply(from, rows, cols) {
                prop_assert!(to.row < rows && to.col < cols);
                prop_assert_eq!(Advance::Left.apply(to, rows, cols), Some(from));
            }
        }

        #[test]
        fn rowset_matches_model(ops in proptest::collection::vec((0usize..60, 0usize..8, 0u8..3), 0..80)) {
            let mut set = RowSet::default();
            let mut model = [false; 80];
            for (start, len, op) in ops {
                match op {
                    0 => { set.insert(start..start + len); for m in &mut model[start..start + len] { *m = true; } }
                    1 => { set.remove(start); model[start] = false; }
                    _ => { set.toggle(start); model[start] = !model[start]; }
                }
                for (i, m) in model.iter().enumerate() {
                    prop_assert_eq!(set.contains(i), *m);
                }
                for w in set.ranges().windows(2) {
                    prop_assert!(w[0].end < w[1].start, "not merged: {:?}", set.ranges());
                }
                prop_assert!(set.ranges().iter().all(|r| !r.is_empty()));
            }
        }

        #[test]
        fn nav_stays_in_bounds(r in 0usize..1000, c in 0usize..60, rows in 1usize..500, cols in 1usize..50, page in 0usize..40, k in 0usize..10) {
            let navs = [Nav::Up, Nav::Down, Nav::Left, Nav::Right, Nav::PageUp, Nav::PageDown, Nav::Home, Nav::End, Nav::First, Nav::Last];
            let p = navs[k].apply(CellPos::new(r, c), rows, cols, page);
            prop_assert!(p.row < rows && p.col < cols);
        }
    }
}
