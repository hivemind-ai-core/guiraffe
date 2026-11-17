//! Virtual-window math for rows. Offsets are `f64`: at 1,000,000 rows × 36 px the content is
//! 36M px tall, past f32's 2^24 exact-integer range, so f32 offsets would jitter.
//! Everything here is allocation-free.

use std::ops::Range;

/// Largest vertical scroll offset.
pub fn max_offset(count: usize, row_h: f64, viewport_h: f64) -> f64 {
    (count as f64 * row_h - viewport_h).max(0.)
}

/// Clamp `offset` into `[0, max]` (NaN becomes 0).
pub fn clamp_offset(offset: f64, count: usize, row_h: f64, viewport_h: f64) -> f64 {
    if offset.is_nan() {
        return 0.;
    }
    offset.clamp(0., max_offset(count, row_h, viewport_h))
}

/// Rows intersecting the viewport, extended by `overscan` on both sides.
pub fn visible_rows(
    offset: f64,
    row_h: f64,
    viewport_h: f64,
    count: usize,
    overscan: usize,
) -> Range<usize> {
    if count == 0 || row_h <= 0. || viewport_h <= 0. {
        return 0..0;
    }
    let first = (offset / row_h).floor().max(0.) as usize;
    let last = ((offset + viewport_h) / row_h).ceil().max(0.) as usize;
    let start = first.min(count).saturating_sub(overscan);
    let end = last.saturating_add(overscan).min(count);
    start..end.max(start)
}

/// How many row slots a viewport ever needs (worst-case partial rows plus overscan).
pub fn pool_size(row_h: f64, viewport_h: f64, overscan: usize) -> usize {
    if row_h <= 0. || viewport_h <= 0. {
        return 0;
    }
    (viewport_h / row_h).ceil() as usize + 1 + 2 * overscan
}

/// Row under viewport-relative `y`.
pub fn row_at(y: f64, offset: f64, row_h: f64, count: usize) -> Option<usize> {
    if y < 0. || row_h <= 0. {
        return None;
    }
    let r = ((offset + y) / row_h).floor();
    (r >= 0. && (r as usize) < count).then_some(r as usize)
}

/// Offset that brings `row` fully into view with minimal movement.
pub fn scroll_into_view(offset: f64, row: usize, row_h: f64, viewport_h: f64, count: usize) -> f64 {
    let top = row as f64 * row_h;
    let bottom = top + row_h;
    let o = if top < offset {
        top
    } else if bottom > offset + viewport_h {
        bottom - viewport_h
    } else {
        offset
    };
    clamp_offset(o, count, row_h, viewport_h)
}

/// Viewport-relative top of `row` as f32 (small, so precise).
pub fn row_top(row: usize, offset: f64, row_h: f64) -> f32 {
    (row as f64 * row_h - offset) as f32
}

/// Slot of `row` in a pool of `pool` recycled row views. Visible rows are contiguous and
/// never more than `pool`, so they map to distinct slots; scrolling by one row reassigns
/// exactly one slot.
pub fn slot_of(row: usize, pool: usize) -> usize {
    if pool == 0 { 0 } else { row % pool }
}

/// Rows of `new` that were not in `old` (their slots need a re-render).
pub fn entering(old: Range<usize>, new: Range<usize>) -> impl Iterator<Item = usize> {
    new.filter(move |r| !old.contains(r))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn million_rows_precise() {
        let n = 1_000_000;
        let h = 36.;
        let max = max_offset(n, h, 700.);
        assert_eq!(visible_rows(max, h, 700., n, 0).end, n);
        let r = visible_rows(999_000. * h + 0.25, h, 700., n, 2);
        assert_eq!(r.start, 998_998);
        assert!((row_top(999_000, 999_000. * h + 0.25, h) + 0.25).abs() < 1e-6);
        assert_eq!(row_at(10., 999_000. * h, h, n), Some(999_000));
        assert_eq!(row_at(10., max + 1000., h, n), None);
    }

    #[test]
    fn into_view() {
        assert_eq!(scroll_into_view(0., 30, 20., 200., 100), 420.);
        assert_eq!(scroll_into_view(500., 3, 20., 200., 100), 60.);
        assert_eq!(scroll_into_view(500., 26, 20., 200., 100), 500.);
    }

    #[test]
    fn degenerate() {
        assert_eq!(visible_rows(0., 0., 100., 10, 1), 0..0);
        assert_eq!(visible_rows(0., 10., 100., 0, 1), 0..0);
        assert_eq!(clamp_offset(f64::NAN, 10, 10., 50.), 0.);
        assert_eq!(pool_size(10., 0., 2), 0);
    }

    proptest! {
        #[test]
        fn visible_fits_pool(offset in 0f64..4e7, h in 12f64..80., vh in 1f64..2000., n in 0usize..2_000_000, os in 0usize..6) {
            let o = clamp_offset(offset, n, h, vh);
            let r = visible_rows(o, h, vh, n, os);
            prop_assert!(r.len() <= pool_size(h, vh, os));
            prop_assert!(r.end <= n);
            // distinct slots
            let p = pool_size(h, vh, os).max(1);
            let mut seen = vec![false; p];
            for row in r.clone() {
                let s = slot_of(row, p);
                prop_assert!(!seen[s]);
                seen[s] = true;
            }
            // every on-screen row is inside
            if let Some(row) = row_at(vh / 2., o, h, n) {
                prop_assert!(r.contains(&row));
            }
        }

        #[test]
        fn into_view_contains(row in 0usize..10_000, offset in 0f64..400_000., h in 10f64..60., vh in 60f64..900.) {
            let n = 10_000;
            let o = scroll_into_view(clamp_offset(offset, n, h, vh), row, h, vh, n);
            let top = row as f64 * h - o;
            prop_assert!(top >= -1e-6 && top + h <= vh + 1e-6);
        }
    }
}
