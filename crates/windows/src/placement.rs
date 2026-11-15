//! Pure window placement: restore saved bounds onto the displays that exist now.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    fn is_finite(&self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
    }
    /// Area of the overlap with `other` (0 when disjoint).
    pub fn overlap_area(&self, other: &Rect) -> f32 {
        let w = self.right().min(other.right()) - self.x.max(other.x);
        let h = self.bottom().min(other.bottom()) - self.y.max(other.y);
        if w <= 0.0 || h <= 0.0 { 0.0 } else { w * h }
    }
    /// A rect of `size` centered in `self`.
    pub fn centered(&self, w: f32, h: f32) -> Rect {
        Rect::new(
            self.x + (self.w - w) / 2.0,
            self.y + (self.h - h) / 2.0,
            w,
            h,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    /// Stable identity across runs (platform UUID) when known.
    pub uuid: Option<String>,
    pub bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacementRules {
    pub min_w: f32,
    pub min_h: f32,
    pub default_w: f32,
    pub default_h: f32,
}

impl Default for PlacementRules {
    fn default() -> Self {
        Self {
            min_w: 320.0,
            min_h: 240.0,
            default_w: 1100.0,
            default_h: 760.0,
        }
    }
}

/// Where to open a window that was last at `saved` (on display `saved_display`).
///
/// - No displays known: the saved rect (sanitized) or the default size at the origin.
/// - Saved display still connected: keep the rect, clamped onto that display.
/// - Otherwise: keep the size if it fits, move onto the display it overlaps most,
///   or center on the primary (first) display.
///
/// The result always has finite coordinates, at least the minimum size, and no
/// larger than the target display.
pub fn place(
    saved: Option<(Rect, Option<&str>)>,
    displays: &[Display],
    rules: &PlacementRules,
) -> Rect {
    let sanitize = |r: Rect| -> Option<Rect> {
        r.is_finite().then(|| Rect {
            w: r.w.max(rules.min_w),
            h: r.h.max(rules.min_h),
            ..r
        })
    };
    let saved = saved.and_then(|(r, d)| sanitize(r).map(|r| (r, d)));
    let Some(primary) = displays.first() else {
        return saved.map(|(r, _)| r).unwrap_or(Rect::new(
            0.0,
            0.0,
            rules.default_w,
            rules.default_h,
        ));
    };
    let Some((rect, uuid)) = saved else {
        let (w, h) = (
            rules.default_w.min(primary.bounds.w),
            rules.default_h.min(primary.bounds.h),
        );
        return primary.bounds.centered(w, h);
    };
    let target = uuid
        .and_then(|u| displays.iter().find(|d| d.uuid.as_deref() == Some(u)))
        .or_else(|| {
            displays
                .iter()
                .map(|d| (d, d.bounds.overlap_area(&rect)))
                .filter(|(_, a)| *a > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(d, _)| d)
        });
    match target {
        Some(d) => clamp_onto(rect, &d.bounds, rules),
        None => {
            let (w, h) = (rect.w.min(primary.bounds.w), rect.h.min(primary.bounds.h));
            primary.bounds.centered(w, h)
        }
    }
}

/// Shrink `r` to fit `area` and move it so it lies fully within `area`.
fn clamp_onto(r: Rect, area: &Rect, rules: &PlacementRules) -> Rect {
    let w = r.w.min(area.w).max(rules.min_w.min(area.w));
    let h = r.h.min(area.h).max(rules.min_h.min(area.h));
    let x = r.x.clamp(area.x, area.right() - w);
    let y = r.y.clamp(area.y, area.bottom() - h);
    Rect::new(x, y, w, h)
}

/// Cascade offset for the `n`th extra window of the same kind, wrapping so new
/// windows never walk off-screen.
pub fn cascade(base: Rect, n: usize, area: Option<&Rect>) -> Rect {
    const STEP: f32 = 28.0;
    let steps = match area {
        Some(a) => {
            let room = ((a.right() - base.right()).min(a.bottom() - base.bottom()) / STEP).floor();
            if room >= 1.0 {
                n % (room as usize + 1)
            } else {
                0
            }
        }
        None => n % 10,
    };
    let off = STEP * steps as f32;
    Rect {
        x: base.x + off,
        y: base.y + off,
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disp(uuid: &str, x: f32, w: f32) -> Display {
        Display {
            uuid: Some(uuid.into()),
            bounds: Rect::new(x, 0.0, w, 1080.0),
        }
    }
    const R: PlacementRules = PlacementRules {
        min_w: 320.0,
        min_h: 240.0,
        default_w: 1100.0,
        default_h: 760.0,
    };

    #[test]
    fn no_saved_centers_on_primary() {
        let r = place(None, &[disp("a", 0.0, 1920.0)], &R);
        assert_eq!(r, Rect::new(410.0, 160.0, 1100.0, 760.0));
    }

    #[test]
    fn no_displays_keeps_saved_or_default() {
        assert_eq!(place(None, &[], &R), Rect::new(0.0, 0.0, 1100.0, 760.0));
        let s = Rect::new(5.0, 6.0, 700.0, 500.0);
        assert_eq!(place(Some((s, None)), &[], &R), s);
    }

    #[test]
    fn restores_on_same_display_unchanged() {
        let s = Rect::new(2000.0, 100.0, 800.0, 600.0);
        let ds = [disp("a", 0.0, 1920.0), disp("b", 1920.0, 2560.0)];
        assert_eq!(place(Some((s, Some("b"))), &ds, &R), s);
    }

    #[test]
    fn display_gone_moves_to_primary() {
        let s = Rect::new(4000.0, 100.0, 800.0, 600.0);
        let r = place(Some((s, Some("gone"))), &[disp("a", 0.0, 1920.0)], &R);
        assert_eq!(r, Rect::new(560.0, 240.0, 800.0, 600.0));
    }

    #[test]
    fn partially_offscreen_is_pulled_in() {
        let s = Rect::new(1700.0, -50.0, 800.0, 600.0);
        let r = place(Some((s, Some("a"))), &[disp("a", 0.0, 1920.0)], &R);
        assert_eq!(r, Rect::new(1120.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn unknown_uuid_uses_most_overlapping_display() {
        let s = Rect::new(1800.0, 0.0, 800.0, 600.0); // mostly on b
        let ds = [disp("a", 0.0, 1920.0), disp("b", 1920.0, 1920.0)];
        let r = place(Some((s, None)), &ds, &R);
        assert_eq!(r.x, 1920.0);
    }

    #[test]
    fn oversized_and_tiny_and_nan_are_sanitized() {
        let ds = [disp("a", 0.0, 1280.0)];
        let big = place(
            Some((Rect::new(0.0, 0.0, 5000.0, 5000.0), Some("a"))),
            &ds,
            &R,
        );
        assert_eq!((big.w, big.h), (1280.0, 1080.0));
        let tiny = place(Some((Rect::new(10.0, 10.0, 1.0, 1.0), Some("a"))), &ds, &R);
        assert_eq!((tiny.w, tiny.h), (320.0, 240.0));
        let nan = place(
            Some((Rect::new(f32::NAN, 0.0, 100.0, 100.0), Some("a"))),
            &ds,
            &R,
        );
        assert!(nan.x.is_finite());
        assert_eq!(nan, place(None, &ds, &R));
    }

    #[test]
    fn display_smaller_than_min_size() {
        let ds = [Display {
            uuid: None,
            bounds: Rect::new(0.0, 0.0, 200.0, 100.0),
        }];
        let r = place(Some((Rect::new(0.0, 0.0, 800.0, 600.0), None)), &ds, &R);
        assert_eq!(r, Rect::new(0.0, 0.0, 200.0, 100.0));
    }

    #[test]
    fn cascade_wraps_within_area() {
        let area = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let base = Rect::new(0.0, 0.0, 900.0, 900.0); // room for 3 steps
        assert_eq!(cascade(base, 0, Some(&area)).x, 0.0);
        assert_eq!(cascade(base, 3, Some(&area)).x, 84.0);
        assert_eq!(cascade(base, 4, Some(&area)).x, 0.0);
        let full = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        assert_eq!(cascade(full, 7, Some(&area)).x, 0.0);
    }
}
