//! How a cable or an edge is stroked: solid, dashed or dotted.
//!
//! Shared by flow edges and by cables drawn between element anchors, so both look alike.
//! Lengths are in screen pixels at zoom 1; a painter scales them with the zoom it draws at.

use serde::{Deserialize, Serialize};

/// A stroke pattern.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StrokeStyle {
    /// One unbroken line.
    #[default]
    Solid,
    /// Dashes `dash` long with `gap` between them.
    Dashed { dash: f32, gap: f32 },
    /// Round dots `gap` apart.
    Dotted { gap: f32 },
}

impl StrokeStyle {
    /// Dashes of 6 with gaps of 4.
    pub const DASHED: StrokeStyle = StrokeStyle::Dashed {
        dash: 6.0,
        gap: 4.0,
    };
    /// Dots 4 apart.
    pub const DOTTED: StrokeStyle = StrokeStyle::Dotted { gap: 4.0 };

    /// The on and off lengths of the pattern at `scale`, or `None` for a solid line.
    /// A dot is a dash as long as the stroke is wide (`width`), drawn with round caps.
    pub fn pattern(self, width: f32, scale: f32) -> Option<(f32, f32)> {
        let clean = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let (on, off) = match self {
            StrokeStyle::Solid => return None,
            StrokeStyle::Dashed { dash, gap } => (clean(dash), clean(gap)),
            StrokeStyle::Dotted { gap } => (clean(width).max(0.5), clean(gap)),
        };
        let scale = clean(scale);
        let (on, off) = (on * scale, off * scale);
        (on > 0.0 && off > 0.0).then_some((on, off))
    }
}

/// Splits the polyline `points` into the dashes of an `on`/`off` pattern starting `offset`
/// into it (for an animated pattern), calling `dash` with the points of each dash in order.
/// Allocates nothing: `scratch` holds the current dash and keeps its room across calls.
pub fn dashes(
    points: impl IntoIterator<Item = (f32, f32)>,
    (on, off): (f32, f32),
    offset: f32,
    scratch: &mut Vec<(f32, f32)>,
    mut dash: impl FnMut(&[(f32, f32)]),
) {
    scratch.clear();
    let period = on + off;
    if !(period > 0.0 && on > 0.0) {
        return;
    }
    // Where in the period the line starts.
    let phase = offset.rem_euclid(period);
    let mut drawing = phase < on;
    let mut left = if drawing { on - phase } else { period - phase };
    let mut points = points.into_iter();
    let Some(mut previous) = points.next() else {
        return;
    };
    if drawing {
        scratch.push(previous);
    }
    for point in points {
        let (dx, dy) = (point.0 - previous.0, point.1 - previous.1);
        let mut length = (dx * dx + dy * dy).sqrt();
        if !length.is_finite() || length == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / length, dy / length);
        let mut at = previous;
        while length >= left {
            at = (at.0 + ux * left, at.1 + uy * left);
            length -= left;
            if drawing {
                scratch.push(at);
                dash(scratch);
                scratch.clear();
                left = off;
            } else {
                scratch.push(at);
                left = on;
            }
            drawing = !drawing;
        }
        left -= length;
        if drawing {
            scratch.push(point);
        }
        previous = point;
    }
    // A dash that began where the line ends has no length.
    if drawing && scratch.windows(2).any(|pair| pair[0] != pair[1]) {
        dash(scratch);
    }
    scratch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(points: &[(f32, f32)], pattern: (f32, f32), offset: f32) -> Vec<Vec<(f32, f32)>> {
        let mut out = Vec::new();
        let mut scratch = Vec::new();
        dashes(points.iter().copied(), pattern, offset, &mut scratch, |d| {
            out.push(d.to_vec())
        });
        out
    }

    #[test]
    fn a_straight_line_splits_into_dashes_and_gaps() {
        let dashes = collect(&[(0.0, 0.0), (20.0, 0.0)], (6.0, 4.0), 0.0);
        assert_eq!(
            dashes,
            vec![vec![(0.0, 0.0), (6.0, 0.0)], vec![(10.0, 0.0), (16.0, 0.0)],]
        );
    }

    #[test]
    fn a_dash_follows_a_corner() {
        let dashes = collect(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)], (6.0, 10.0), 0.0);
        assert_eq!(dashes, vec![vec![(0.0, 0.0), (4.0, 0.0), (4.0, 2.0)]]);
    }

    #[test]
    fn an_offset_shifts_the_pattern() {
        let dashes = collect(&[(0.0, 0.0), (20.0, 0.0)], (6.0, 4.0), 8.0);
        assert_eq!(dashes[0], vec![(2.0, 0.0), (8.0, 0.0)]);
    }

    #[test]
    fn solid_and_degenerate_patterns_draw_no_dashes() {
        assert_eq!(StrokeStyle::Solid.pattern(1.5, 1.0), None);
        assert_eq!(
            StrokeStyle::Dashed {
                dash: 0.0,
                gap: 4.0
            }
            .pattern(1.5, 1.0),
            None
        );
        assert_eq!(
            StrokeStyle::DASHED.pattern(1.5, 2.0),
            Some((12.0, 8.0)),
            "scaled with the zoom"
        );
        assert!(collect(&[(0.0, 0.0), (20.0, 0.0)], (0.0, 0.0), 0.0).is_empty());
    }
}
