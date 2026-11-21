//! Bounded broad phase for flowchart collision queries.

use std::collections::HashMap;

const CELL_SIZE: f32 = 96.0;
const MAX_CELLS: i64 = 128;

pub(super) type Bounds = (f32, f32, f32, f32);
type Cell = (i32, i32);

pub(super) fn segment_bounds(a: (f32, f32), b: (f32, f32), pad: f32) -> Bounds {
    (
        a.0.min(b.0) - pad,
        a.1.min(b.1) - pad,
        a.0.max(b.0) + pad,
        a.1.max(b.1) + pad,
    )
}

fn cells((left, top, right, bottom): Bounds) -> Option<impl Iterator<Item = Cell>> {
    if ![left, top, right, bottom].into_iter().all(f32::is_finite) {
        return None;
    }
    let cell = |coordinate: f32| (coordinate / CELL_SIZE).floor() as i32;
    let (left, top, right, bottom) = (cell(left), cell(top), cell(right), cell(bottom));
    let columns = i64::from(right) - i64::from(left) + 1;
    let rows = i64::from(bottom) - i64::from(top) + 1;
    if columns <= 0 || rows <= 0 || columns.saturating_mul(rows) > MAX_CELLS {
        return None;
    }
    Some((top..=bottom).flat_map(move |y| (left..=right).map(move |x| (x, y))))
}

#[derive(Default)]
pub(super) struct SpatialIndex {
    buckets: HashMap<Cell, Vec<usize>>,
    global: Vec<usize>,
    bounds: Vec<Option<Bounds>>,
}

impl SpatialIndex {
    pub(super) fn push(&mut self, bounds: Option<Bounds>) {
        let index = self.bounds.len();
        self.bounds.push(bounds);
        if let Some(cells) = bounds.and_then(cells) {
            for cell in cells {
                self.buckets.entry(cell).or_default().push(index);
            }
        } else {
            self.global.push(index);
        }
    }

    /// Return candidates in insertion order, preserving floating-point score sums.
    pub(super) fn query(&self, bounds: Option<Bounds>, candidates: &mut Vec<usize>) {
        candidates.clear();
        let overlaps = |&i: &usize| {
            bounds.is_none_or(|(left, top, right, bottom)| {
                self.bounds[i].is_none_or(|(x0, y0, x1, y1)| {
                    !(right < x0 || bottom < y0 || left > x1 || top > y1)
                })
            })
        };
        if let Some(cells) = bounds.and_then(cells) {
            candidates.extend(self.global.iter().copied().filter(overlaps));
            for cell in cells {
                if let Some(bucket) = self.buckets.get(&cell) {
                    candidates.extend(bucket.iter().copied().filter(overlaps));
                }
            }
            candidates.sort_unstable();
            candidates.dedup();
        } else {
            candidates.extend((0..self.bounds.len()).filter(overlaps));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_local_query_has_the_same_candidates_as_distant_data_grows() {
        for count in [100, 10_000] {
            let mut index = SpatialIndex::default();
            // The local object spans cells and must be reported exactly once.
            index.push(Some((-5.0, -5.0, 5.0, 5.0)));
            for i in 0..count {
                let x = 96.0 * (i + 1) as f32;
                index.push(Some((x, x, x + 1.0, x + 1.0)));
            }
            let mut candidates = Vec::new();
            index.query(
                Some(segment_bounds((-10.0, 0.0), (10.0, 0.0), 0.0)),
                &mut candidates,
            );
            assert_eq!(candidates, [0]);
        }
    }

    #[test]
    fn long_and_unclassified_entries_keep_original_order() {
        let mut index = SpatialIndex::default();
        index.push(Some((-50_000.0, 0.0, 50_000.0, 0.0)));
        index.push(Some((0.0, 0.0, 1.0, 1.0)));
        index.push(None);
        let mut candidates = Vec::new();
        index.query(Some((0.0, 0.0, 0.0, 0.0)), &mut candidates);
        assert_eq!(candidates, [0, 1, 2]);
        index.query(Some((70_000.0, 0.0, 70_001.0, 0.0)), &mut candidates);
        assert_eq!(candidates, [2]);
        index.query(None, &mut candidates);
        assert_eq!(candidates, [0, 1, 2]);
    }
}
