use crate::NodeId;
use std::ops::Range;

/// Stores one prefix sum per visible override. Ordinary rows need no storage.
#[derive(Default)]
pub(crate) struct RowGeometry {
    overrides: Vec<(NodeId, f64)>,
    offsets: Vec<(usize, f64, f64)>,
}

impl RowGeometry {
    pub fn set(&mut self, overrides: impl IntoIterator<Item = (NodeId, f32)>) {
        self.overrides = overrides
            .into_iter()
            .filter(|(_, height)| height.is_finite() && *height > 0.)
            .map(|(id, height)| (id, f64::from(height.clamp(24., 8_192.))))
            .collect();
        self.overrides.sort_by_key(|(id, _)| *id);
        self.overrides.dedup_by_key(|(id, _)| *id);
    }

    pub fn rebuild(
        &mut self,
        base: f64,
        editor: Option<(NodeId, f64)>,
        rank: impl Fn(NodeId) -> Option<usize>,
    ) {
        self.offsets.clear();
        self.offsets
            .extend(self.overrides.iter().filter_map(|&(id, height)| {
                let height = editor
                    .filter(|(row, _)| *row == id)
                    .map_or(height, |(_, edit)| height.max(edit));
                Some((rank(id)?, height - base, 0.))
            }));
        if let Some((id, height)) = editor
            && self
                .overrides
                .binary_search_by_key(&id, |&(id, _)| id)
                .is_err()
            && let Some(rank) = rank(id)
        {
            self.offsets.push((rank, height.max(base) - base, 0.));
        }
        self.offsets.sort_unstable_by_key(|&(rank, _, _)| rank);
        let mut sum = 0.;
        for (rank, delta, bottom) in &mut self.offsets {
            sum += *delta;
            *delta = sum;
            *bottom = (*rank + 1) as f64 * base + sum;
        }
    }

    pub fn top(&self, rank: usize, base: f64) -> f64 {
        let index = self.offsets.partition_point(|&(row, _, _)| row < rank);
        let delta = index.checked_sub(1).map_or(0., |i| self.offsets[i].1);
        rank as f64 * base + delta
    }

    pub fn window(&self, count: usize, scroll: f64, height: f64, base: f64) -> Range<usize> {
        if self.offsets.is_empty() {
            return ((scroll / base).floor() as usize).min(count)
                ..(((scroll + height) / base).ceil() as usize).min(count);
        }
        let at = |y| {
            let split = self.offsets.partition_point(|&(_, _, bottom)| bottom <= y);
            let delta = split.checked_sub(1).map_or(0., |i| self.offsets[i].1);
            let rank = ((y - delta) / base).floor() as usize;
            rank.min(self.offsets.get(split).map_or(count, |entry| entry.0))
        };
        let start = at(scroll);
        let limit = scroll + height;
        let end = at(limit);
        start..(end + usize::from(end < count && self.top(end, base) < limit))
    }
}

#[cfg(test)]
mod tests {
    #![allow(unsafe_code)]
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    struct CountingAllocator;

    thread_local! {
        static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    }

    // SAFETY: Allocation and deallocation forward the original arguments to
    // System. The const TLS counter never allocates and tolerates TLS teardown.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|count| {
                if let Some(value) = count.get() {
                    count.set(Some(value + 1));
                }
            });
            // SAFETY: The caller supplies a valid GlobalAlloc layout.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            // SAFETY: The caller supplies the original System allocation.
            unsafe { System.dealloc(pointer, layout) };
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    #[test]
    fn sparse_viewport_has_the_same_rows_and_storage_at_both_scales() {
        let geometry = RowGeometry {
            offsets: vec![(2, 24., 132.), (3, 8., 152.)],
            ..Default::default()
        };
        assert_eq!(geometry.top(3, 36.), 132.);
        assert_eq!(geometry.top(4, 36.), 152.);
        for count in [10_000, 10_000_000] {
            assert_eq!(geometry.window(count, 72., 80., 36.), 2..4);
            assert_eq!(geometry.window(count, 73., 80., 36.), 2..5);
            assert_eq!(geometry.window(count, 1_000., 144., 36.).len(), 5);
        }
        assert_eq!(geometry.offsets.len(), 2);
    }

    #[test]
    fn warm_sparse_viewports_allocate_nothing() {
        let geometry = RowGeometry {
            offsets: vec![(2, 24., 132.), (3, 8., 152.)],
            ..Default::default()
        };
        for count in [10_000, 10_000_000] {
            ALLOCATIONS.with(|allocations| allocations.set(Some(0)));
            for scroll in 0..1000 {
                let range =
                    std::hint::black_box(geometry.window(count, f64::from(scroll), 288., 36.));
                for rank in range {
                    std::hint::black_box(geometry.top(rank, 36.));
                }
            }
            assert_eq!(
                ALLOCATIONS.with(|allocations| allocations.replace(None)),
                Some(0)
            );
        }
    }
}
