//! Sorting and filtering produce a row *permutation* (`view row -> source row`), never a copy
//! of the data. Pure and `Send`, so the grid runs it on the background executor.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};

use crate::{columns::Comparator, source::CellRef, source::GridSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDir {
    Asc,
    Desc,
}

/// One sort criterion on a column definition index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SortKey {
    pub column: usize,
    pub dir: SortDir,
}

/// A per-column predicate.
pub type Filter = Arc<dyn Fn(CellRef<'_>) -> bool + Send + Sync>;

/// A resolved sort criterion: source column, direction, optional comparator.
#[derive(Clone)]
pub struct SortSpec {
    pub source_col: usize,
    pub dir: SortDir,
    pub comparator: Option<Comparator>,
}

impl SortSpec {
    fn cmp(&self, a: &CellRef<'_>, b: &CellRef<'_>) -> std::cmp::Ordering {
        let o = match &self.comparator {
            Some(c) => c(a, b),
            None => a.total_cmp(b),
        };
        match self.dir {
            SortDir::Asc => o,
            SortDir::Desc => o.reverse(),
        }
    }
}

/// A resolved filter: source column and predicate.
#[derive(Clone)]
pub struct FilterSpec {
    pub source_col: usize,
    pub pred: Filter,
}

/// The row order a grid shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowOrder {
    /// Every source row in source order (no allocation, whatever the row count).
    Identity,
    /// `indices[view_row] = source_row`.
    Indices(Arc<[u32]>),
}

impl RowOrder {
    pub fn len(&self, source_rows: usize) -> usize {
        match self {
            Self::Identity => source_rows,
            Self::Indices(v) => v.len(),
        }
    }
    pub fn is_empty(&self, source_rows: usize) -> bool {
        self.len(source_rows) == 0
    }
    /// Source row for `view_row`.
    pub fn source_row(&self, view_row: usize, source_rows: usize) -> Option<usize> {
        match self {
            Self::Identity => (view_row < source_rows).then_some(view_row),
            Self::Indices(v) => v.get(view_row).map(|&r| r as usize),
        }
    }

    /// `source row -> view row` lookup, for remapping selection and edits after the order
    /// changes. Identity needs no table; a permutation costs one `u32` per source row.
    pub fn inverse(&self, source_rows: usize) -> Inverse {
        self.build_inverse(source_rows, || false)
            .unwrap_or(Inverse::Identity(source_rows))
    }

    pub(crate) fn build_inverse(
        &self,
        source_rows: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<Inverse, Cancelled> {
        if cancelled() {
            return Err(Cancelled);
        }
        Ok(match self {
            Self::Identity => Inverse::Identity(source_rows),
            Self::Indices(v) => {
                let mut inv = vec![u32::MAX; source_rows.min(MAX_ROWS)];
                for (view, &src) in v.iter().enumerate() {
                    if view % 4096 == 0 && cancelled() {
                        return Err(Cancelled);
                    }
                    if let Some(slot) = inv.get_mut(src as usize) {
                        *slot = view as u32;
                    }
                }
                Inverse::Table(inv)
            }
        })
    }
}

/// See [`RowOrder::inverse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inverse {
    Identity(usize),
    Table(Vec<u32>),
}

impl Inverse {
    /// View row showing `source_row`, or `None` when it is filtered out.
    pub fn view_row(&self, source_row: usize) -> Option<usize> {
        match self {
            Self::Identity(n) => (source_row < *n).then_some(source_row),
            Self::Table(t) => t
                .get(source_row)
                .filter(|&&v| v != u32::MAX)
                .map(|&v| v as usize),
        }
    }
}

/// The job was cancelled (a newer sort/filter superseded it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

/// Largest row count addressable by a `u32` permutation.
pub const MAX_ROWS: usize = u32::MAX as usize;

/// Filter then stably sort. Cost: O(n) cell reads for filtering, one primary-key read per
/// row, O(n log n) comparisons; ties on the primary key read later keys on demand. Memory:
/// one `(CellRef, u32)` pair per surviving row while sorting, then a `u32` per row.
pub fn compute_order(
    src: &dyn GridSource,
    filters: &[FilterSpec],
    sort: &[SortSpec],
    cancel: &AtomicBool,
) -> Result<RowOrder, Cancelled> {
    let n = src.row_count().min(MAX_ROWS);
    if filters.is_empty() && sort.is_empty() {
        return Ok(RowOrder::Identity);
    }
    let check = || {
        if cancel.load(AtomicOrdering::Relaxed) {
            Err(Cancelled)
        } else {
            Ok(())
        }
    };
    let mut rows: Vec<u32> = Vec::with_capacity(if filters.is_empty() { n } else { n / 4 });
    const CANCEL_INTERVAL: usize = 65_536;
    for r in 0..n {
        if r % CANCEL_INTERVAL == 0 {
            check()?;
        }
        if filters.iter().all(|f| (f.pred)(src.cell(r, f.source_col))) {
            rows.push(r as u32);
        }
    }
    check()?;
    let Some((primary, rest)) = sort.split_first() else {
        return Ok(RowOrder::Indices(rows.into()));
    };
    let mut keyed = Vec::with_capacity(rows.len());
    for chunk in rows.chunks(CANCEL_INTERVAL) {
        check()?;
        keyed.extend(
            chunk
                .iter()
                .map(|&r| (src.cell(r as usize, primary.source_col), r)),
        );
    }
    drop(rows);
    check()?;
    // `sort_by` is stable: equal keys keep source order.
    keyed.sort_by(|(ka, a), (kb, b)| {
        primary.cmp(ka, kb).then_with(|| {
            for s in rest {
                let o = s.cmp(
                    &src.cell(*a as usize, s.source_col),
                    &src.cell(*b as usize, s.source_col),
                );
                if o.is_ne() {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        })
    });
    check()?;
    Ok(RowOrder::Indices(
        keyed.into_iter().map(|(_, r)| r).collect(),
    ))
}

/// Header-click sort cycling. Without `multi`: asc -> desc -> none on the clicked column,
/// replacing other keys. With `multi` (shift-click): the same cycle on that column while
/// keeping the others.
pub fn toggle_sort(keys: &mut Vec<SortKey>, column: usize, multi: bool) {
    let pos = keys.iter().position(|k| k.column == column);
    let next = match pos.and_then(|p| keys.get(p)).map(|k| k.dir) {
        None => Some(SortDir::Asc),
        Some(SortDir::Asc) => Some(SortDir::Desc),
        Some(SortDir::Desc) => None,
    };
    if !multi {
        let keep_cycle = keys.len() == 1 && pos.is_some();
        keys.clear();
        let dir = if keep_cycle { next } else { Some(SortDir::Asc) };
        if let Some(dir) = dir {
            keys.push(SortKey { column, dir });
        }
        return;
    }
    match (pos, next) {
        (Some(p), Some(dir)) => {
            if let Some(k) = keys.get_mut(p) {
                k.dir = dir;
            }
        }
        (Some(p), None) => {
            keys.remove(p);
        }
        (None, Some(dir)) => keys.push(SortKey { column, dir }),
        (None, None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FnSource;
    use proptest::prelude::*;

    fn src(vals: Vec<(i64, i64)>) -> crate::VecSource<(i64, i64)> {
        crate::VecSource::new(vals)
            .column(|r: &(i64, i64)| CellRef::Int(r.0))
            .column(|r: &(i64, i64)| CellRef::Int(r.1))
    }

    fn spec(c: usize, dir: SortDir) -> SortSpec {
        SortSpec {
            source_col: c,
            dir,
            comparator: None,
        }
    }

    fn indices(o: RowOrder) -> Vec<u32> {
        match o {
            RowOrder::Identity => vec![],
            RowOrder::Indices(v) => v.to_vec(),
        }
    }

    #[test]
    fn identity_without_work() {
        let s = FnSource::new(1_000_000, |r, _| CellRef::Int(r as i64));
        let o = compute_order(&s, &[], &[], &AtomicBool::new(false));
        assert_eq!(o, Ok(RowOrder::Identity));
        assert_eq!(RowOrder::Identity.source_row(5, 10), Some(5));
        assert_eq!(RowOrder::Identity.source_row(10, 10), None);
    }

    #[test]
    fn cancelled_inverse_does_not_publish_a_partial_permutation() {
        let order = RowOrder::Indices((0..10_000).rev().collect());
        let checks = std::cell::Cell::new(0);
        assert_eq!(
            order.build_inverse(10_000, || {
                checks.set(checks.get() + 1);
                checks.get() >= 3
            }),
            Err(Cancelled)
        );
        assert_eq!(checks.get(), 3);
        assert_eq!(order.inverse(10_000).view_row(0), Some(9_999));
    }

    #[test]
    fn cancellation_during_key_reads_avoids_reading_the_remaining_rows() {
        use std::sync::atomic::AtomicUsize;
        let reads = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let source = FnSource::new(262_144, {
            let reads = reads.clone();
            let cancel = cancel.clone();
            move |row, _| {
                reads.fetch_add(1, AtomicOrdering::Relaxed);
                cancel.store(true, AtomicOrdering::Relaxed);
                CellRef::Int(row as i64)
            }
        });
        assert_eq!(
            compute_order(&source, &[], &[spec(0, SortDir::Asc)], &cancel),
            Err(Cancelled),
        );
        assert!(reads.load(AtomicOrdering::Relaxed) <= 65_536);
    }

    #[test]
    fn multi_key_stable_and_desc() {
        let s = src(vec![(2, 1), (1, 9), (2, 0), (1, 9), (3, 5)]);
        let none = AtomicBool::new(false);
        let o = compute_order(&s, &[], &[spec(0, SortDir::Asc)], &none).map(indices);
        assert_eq!(o, Ok(vec![1, 3, 0, 2, 4]));
        let o = compute_order(
            &s,
            &[],
            &[spec(0, SortDir::Desc), spec(1, SortDir::Asc)],
            &none,
        )
        .map(indices);
        assert_eq!(o, Ok(vec![4, 2, 0, 1, 3]));
    }

    #[test]
    fn filter_then_sort_and_cancel() {
        let s = src((0..100).map(|i| (100 - i, i % 7)).collect());
        let f = FilterSpec {
            source_col: 1,
            pred: Arc::new(|c| matches!(c, CellRef::Int(0))),
        };
        let o = compute_order(
            &s,
            std::slice::from_ref(&f),
            &[spec(0, SortDir::Asc)],
            &AtomicBool::new(false),
        )
        .map(indices);
        assert_eq!(
            o,
            Ok(vec![
                98, 91, 84, 77, 70, 63, 56, 49, 42, 35, 28, 21, 14, 7, 0
            ])
        );
        assert_eq!(
            compute_order(&s, &[f], &[], &AtomicBool::new(true)),
            Err(Cancelled)
        );
    }

    #[test]
    fn inverse_maps_back() {
        let o = RowOrder::Indices(vec![3u32, 0, 2].into());
        let inv = o.inverse(4);
        assert_eq!(inv.view_row(3), Some(0));
        assert_eq!(inv.view_row(2), Some(2));
        assert_eq!(inv.view_row(1), None, "filtered out");
        assert_eq!(inv.view_row(9), None);
        assert_eq!(RowOrder::Identity.inverse(4).view_row(2), Some(2));
    }

    #[test]
    fn toggle_cycles() {
        let mut k = vec![];
        toggle_sort(&mut k, 2, false);
        assert_eq!(
            k,
            [SortKey {
                column: 2,
                dir: SortDir::Asc
            }]
        );
        toggle_sort(&mut k, 2, false);
        assert_eq!(k[0].dir, SortDir::Desc);
        toggle_sort(&mut k, 3, true);
        assert_eq!(k.len(), 2);
        toggle_sort(&mut k, 3, true);
        toggle_sort(&mut k, 3, true);
        assert_eq!(
            k,
            [SortKey {
                column: 2,
                dir: SortDir::Desc
            }]
        );
        toggle_sort(&mut k, 2, false);
        assert!(k.is_empty());
        toggle_sort(&mut k, 1, false);
        toggle_sort(&mut k, 4, false);
        assert_eq!(
            k,
            [SortKey {
                column: 4,
                dir: SortDir::Asc
            }]
        );
    }

    proptest! {
        #[test]
        fn sorted_stable_permutation(vals in proptest::collection::vec((0i64..5, 0i64..5), 0..300), desc in any::<bool>()) {
            let s = src(vals.clone());
            let dir = if desc { SortDir::Desc } else { SortDir::Asc };
            let o = compute_order(&s, &[], &[spec(0, dir)], &AtomicBool::new(false)).map(indices);
            let Ok(o) = o else { return Err(TestCaseError::fail("cancelled")) };
            let mut seen = o.clone();
            seen.sort_unstable();
            prop_assert_eq!(seen, (0..vals.len() as u32).collect::<Vec<_>>());
            // Same as std's stable sort of (key, index).
            let mut expect: Vec<u32> = (0..vals.len() as u32).collect();
            expect.sort_by(|a, b| {
                let o = vals[*a as usize].0.cmp(&vals[*b as usize].0);
                if desc { o.reverse() } else { o }
            });
            prop_assert_eq!(o, expect);
        }

        #[test]
        fn filter_sort_sequence_and_inverse_match_reference(
            values in proptest::collection::vec((0i64..5, 0i64..5), 0..200),
            steps in proptest::collection::vec((any::<bool>(), any::<bool>(), 0i64..5), 1..20),
        ) {
            let source = src(values.clone());
            for (filter, descending, minimum) in steps {
                let filters = filter.then(|| FilterSpec {
                    source_col: 1,
                    pred: Arc::new(move |cell| matches!(cell, CellRef::Int(v) if v >= minimum)),
                });
                let direction = if descending { SortDir::Desc } else { SortDir::Asc };
                let result = compute_order(
                    &source, filters.as_slice(),
                    &[spec(0, direction), spec(1, SortDir::Asc)],
                    &AtomicBool::new(false),
                ).map_err(|_| TestCaseError::fail("unexpected cancellation"))?;
                let mut expected: Vec<_> = (0..values.len())
                    .filter(|&row| !filter || values[row].1 >= minimum)
                    .collect();
                expected.sort_by_key(|&row| {
                    let (a, b) = values[row];
                    (if descending { -a } else { a }, b)
                });
                let inverse = result.inverse(values.len());
                prop_assert_eq!(result.len(values.len()), expected.len());
                for (view, &source) in expected.iter().enumerate() {
                    prop_assert_eq!(result.source_row(view, values.len()), Some(source));
                    prop_assert_eq!(inverse.view_row(source), Some(view));
                }
                for (row, &(_, b)) in values.iter().enumerate() {
                    if filter && b < minimum {
                        prop_assert_eq!(inverse.view_row(row), None);
                    }
                }
            }
        }

        #[test]
        fn filter_keeps_exactly_matches(vals in proptest::collection::vec((0i64..5, 0i64..5), 0..300), want in 0i64..5) {
            let s = src(vals.clone());
            let f = FilterSpec { source_col: 1, pred: Arc::new(move |c| matches!(c, CellRef::Int(v) if v == want)) };
            let o = compute_order(&s, &[f], &[], &AtomicBool::new(false)).map(indices);
            let expect: Vec<u32> = vals.iter().enumerate().filter(|(_, v)| v.1 == want).map(|(i, _)| i as u32).collect();
            prop_assert_eq!(o, Ok(expect));
        }
    }
}
