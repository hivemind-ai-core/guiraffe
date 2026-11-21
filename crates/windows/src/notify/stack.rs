//! Pure stacking geometry for notification windows.
//!
//! All cards of one corner live in one borderless surface (one window) anchored to that screen
//! corner. Cards have deterministic heights ([`CardMetrics::height`]: the body is clamped to a
//! fixed number of lines), so the surface size and every card's slot are known before anything
//! renders; nothing is measured after layout.
use serde::{Deserialize, Serialize};

use crate::placement::Rect;

/// The screen corner notifications stack in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Corner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];
    pub fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }
    pub fn is_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }
    /// The horizontal direction cards slide in from: `+1` from the right edge, `-1` from the left.
    pub fn slide_sign(self) -> f32 {
        if self.is_left() { -1.0 } else { 1.0 }
    }
}

/// Stack configuration. Values are logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StackConfig {
    pub corner: Corner,
    /// Distance from the screen (work-area) edges.
    pub margin: f32,
    /// Space between cards.
    pub gap: f32,
    /// Card width.
    pub width: f32,
    /// Cards shown at once; more wait in a queue (FIFO) and appear as others close.
    pub max_visible: usize,
    /// Transparent room around the cards inside the surface, so card shadows aren't clipped.
    /// Taken out of `margin`, so cards still sit `margin` from the screen edge.
    pub shadow_pad: f32,
}

impl Default for StackConfig {
    fn default() -> Self {
        Self {
            corner: Corner::TopRight,
            margin: 16.0,
            gap: 8.0,
            width: 360.0,
            max_visible: 4,
            shadow_pad: 12.0,
        }
    }
}

impl StackConfig {
    /// The config with every value clamped to something drawable.
    pub fn sanitized(mut self) -> Self {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
        self.margin = finite(self.margin, 16.0).clamp(0.0, 512.0);
        self.gap = finite(self.gap, 8.0).clamp(0.0, 128.0);
        self.width = finite(self.width, 360.0).clamp(160.0, 1024.0);
        self.max_visible = self.max_visible.clamp(1, 32);
        self.shadow_pad = finite(self.shadow_pad, 12.0).clamp(0.0, 64.0);
        self
    }
    /// Distance from the screen edge to the surface edge.
    fn surface_margin(&self) -> f32 {
        (self.margin - self.shadow_pad).max(0.0)
    }
}

/// Fixed card section heights, from the theme (see `card::metrics`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardMetrics {
    /// Padding + title line + clamped body lines.
    pub base: f32,
    /// Extra height when the card has no body.
    pub no_body: f32,
    /// The actions row, when there are actions.
    pub actions: f32,
    /// The progress row, when there is a progress value.
    pub progress: f32,
}

impl CardMetrics {
    /// The height of a card with these parts. `no_body` is negative (it removes the body lines).
    pub fn height(&self, has_body: bool, has_actions: bool, has_progress: bool) -> f32 {
        let mut h = self.base;
        if !has_body {
            h += self.no_body;
        }
        if has_actions {
            h += self.actions;
        }
        if has_progress {
            h += self.progress;
        }
        h.max(1.0)
    }
}

/// The surface size and each card's slot inside it (surface-local coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct StackLayout {
    pub width: f32,
    pub height: f32,
    /// One rect per input height, same order. Index 0 (the newest) sits nearest the corner.
    pub slots: Vec<Rect>,
}

impl StackLayout {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// Lay out cards of `heights` (newest first) for `config`. Top corners stack downwards from the
/// top edge of the surface, bottom corners upwards from the bottom edge.
pub fn layout(config: &StackConfig, heights: &[f32]) -> StackLayout {
    let config = config.sanitized();
    let heights: Vec<f32> = heights
        .iter()
        .map(|h| if h.is_finite() { h.max(1.0) } else { 1.0 })
        .collect();
    if heights.is_empty() {
        return StackLayout {
            width: 0.0,
            height: 0.0,
            slots: Vec::new(),
        };
    }
    let pad = config.shadow_pad;
    let total = heights.iter().sum::<f32>() + config.gap * heights.len().saturating_sub(1) as f32;
    let mut slots = Vec::with_capacity(heights.len());
    let mut offset = 0.0;
    for h in &heights {
        let y = if config.corner.is_top() {
            offset
        } else {
            total - offset - h
        };
        slots.push(Rect::new(pad, pad + y, config.width, *h));
        offset += h + config.gap;
    }
    StackLayout {
        width: config.width + 2.0 * pad,
        height: total + 2.0 * pad,
        slots,
    }
}

/// Where a surface of `width`×`height` goes in `work_area` (screen coordinates), for platforms
/// that position windows themselves (X11, macOS, Windows). The surface is clamped inside the
/// work area when it is larger than the space available.
pub fn surface_origin(
    config: &StackConfig,
    work_area: Rect,
    width: f32,
    height: f32,
) -> (f32, f32) {
    let config = config.sanitized();
    let m = config.surface_margin();
    let x = if config.corner.is_left() {
        work_area.x + m
    } else {
        work_area.right() - m - width
    };
    let y = if config.corner.is_top() {
        work_area.y + m
    } else {
        work_area.bottom() - m - height
    };
    (x.max(work_area.x), y.max(work_area.y))
}

/// Layer-shell anchoring for a corner: (top, right, bottom, left) edges to anchor, and the
/// margins (CSS order) the compositor keeps from them. The compositor keeps the corner fixed
/// when the surface is resized.
pub fn layer_anchor(config: &StackConfig) -> ([bool; 4], [f32; 4]) {
    let config = config.sanitized();
    let c = config.corner;
    let anchor = [c.is_top(), !c.is_left(), !c.is_top(), c.is_left()];
    let m = config.surface_margin();
    let margins = anchor.map(|a| if a { m } else { 0.0 });
    (anchor, margins)
}

/// Whether moving from `old` to `new` surface size keeps the surface anchored at its corner when
/// the platform resizes from the top-left origin: true when the origin would not move.
pub fn resize_keeps_corner(corner: Corner, old: (f32, f32), new: (f32, f32)) -> bool {
    let same_w = (old.0 - new.0).abs() < 0.5;
    let same_h = (old.1 - new.1).abs() < 0.5;
    (corner.is_left() || same_w) && (corner.is_top() || same_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(corner: Corner) -> StackConfig {
        StackConfig {
            corner,
            margin: 10.0,
            gap: 5.0,
            width: 300.0,
            max_visible: 3,
            shadow_pad: 0.0,
        }
    }

    #[test]
    fn top_corners_stack_downwards_newest_first() {
        let l = layout(&cfg(Corner::TopRight), &[80.0, 100.0, 60.0]);
        assert_eq!(l.width, 300.0);
        assert_eq!(l.height, 80.0 + 100.0 + 60.0 + 10.0);
        let ys: Vec<f32> = l.slots.iter().map(|r| r.y).collect();
        assert_eq!(ys, [0.0, 85.0, 190.0]);
    }

    #[test]
    fn bottom_corners_stack_upwards_newest_at_the_bottom() {
        let l = layout(&cfg(Corner::BottomLeft), &[80.0, 100.0]);
        assert_eq!(l.height, 185.0);
        assert_eq!(l.slots[0], Rect::new(0.0, 105.0, 300.0, 80.0));
        assert_eq!(l.slots[1], Rect::new(0.0, 0.0, 300.0, 100.0));
        assert_eq!(
            l.slots[0].bottom(),
            l.height,
            "newest touches the corner edge"
        );
    }

    #[test]
    fn slots_never_overlap() {
        for corner in Corner::ALL {
            let l = layout(&cfg(corner), &[40.0, 70.0, 55.0, 90.0]);
            for (i, a) in l.slots.iter().enumerate() {
                for b in &l.slots[i + 1..] {
                    assert_eq!(a.overlap_area(b), 0.0, "{corner:?}");
                }
                assert!(a.y >= 0.0 && a.bottom() <= l.height + 0.001);
            }
        }
    }

    #[test]
    fn shadow_pad_surrounds_cards_and_comes_out_of_the_margin() {
        let c = StackConfig {
            shadow_pad: 8.0,
            ..cfg(Corner::BottomRight)
        };
        let l = layout(&c, &[50.0, 50.0]);
        assert_eq!((l.width, l.height), (316.0, 121.0));
        assert_eq!(l.slots[1], Rect::new(8.0, 8.0, 300.0, 50.0));
        assert_eq!(l.slots[0].bottom(), 113.0);
        let area = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let (x, y) = surface_origin(&c, area, l.width, l.height);
        // Card edge = surface edge + pad = margin from the screen edge.
        assert_eq!(x + l.slots[0].right(), 990.0);
        assert_eq!(y + l.slots[0].bottom(), 990.0);
        assert_eq!(layer_anchor(&c).1, [0.0, 2.0, 2.0, 0.0]);
    }

    #[test]
    fn empty_stack_has_no_size() {
        let l = layout(&cfg(Corner::TopLeft), &[]);
        assert!(l.is_empty());
        assert_eq!((l.width, l.height), (0.0, 0.0));
    }

    #[test]
    fn bad_values_are_clamped() {
        let c = StackConfig {
            margin: f32::NAN,
            gap: -4.0,
            width: 1.0,
            max_visible: 0,
            shadow_pad: f32::NAN,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (c.margin, c.gap, c.width, c.max_visible),
            (16.0, 0.0, 160.0, 1)
        );
        assert_eq!(c.shadow_pad, 12.0);
        let l = layout(&c, &[f32::INFINITY, -3.0]);
        assert!(l.slots.iter().all(|r| r.h == 1.0));
    }

    #[test]
    fn origin_sits_in_the_corner_with_margin() {
        let area = Rect::new(100.0, 50.0, 1000.0, 800.0);
        let (w, h) = (300.0, 200.0);
        assert_eq!(
            surface_origin(&cfg(Corner::TopLeft), area, w, h),
            (110.0, 60.0)
        );
        assert_eq!(
            surface_origin(&cfg(Corner::TopRight), area, w, h),
            (790.0, 60.0)
        );
        assert_eq!(
            surface_origin(&cfg(Corner::BottomLeft), area, w, h),
            (110.0, 640.0)
        );
        assert_eq!(
            surface_origin(&cfg(Corner::BottomRight), area, w, h),
            (790.0, 640.0)
        );
        // Taller than the work area: pinned to its top instead of going off screen.
        assert_eq!(
            surface_origin(&cfg(Corner::BottomRight), area, w, 2000.0).1,
            50.0
        );
    }

    #[test]
    fn layer_anchor_matches_corner() {
        let (a, m) = layer_anchor(&cfg(Corner::BottomRight));
        assert_eq!(a, [false, true, true, false]);
        assert_eq!(m, [0.0, 10.0, 10.0, 0.0]);
        let (a, m) = layer_anchor(&cfg(Corner::TopLeft));
        assert_eq!(a, [true, false, false, true]);
        assert_eq!(m, [10.0, 0.0, 0.0, 10.0]);
    }

    #[test]
    fn only_growth_away_from_the_origin_keeps_the_corner() {
        assert!(resize_keeps_corner(
            Corner::TopLeft,
            (300.0, 100.0),
            (300.0, 400.0)
        ));
        assert!(resize_keeps_corner(
            Corner::TopRight,
            (300.0, 100.0),
            (300.0, 400.0)
        ));
        assert!(!resize_keeps_corner(
            Corner::BottomRight,
            (300.0, 100.0),
            (300.0, 400.0)
        ));
        assert!(resize_keeps_corner(
            Corner::BottomRight,
            (300.0, 100.0),
            (300.0, 100.0)
        ));
    }

    #[test]
    fn card_heights_add_sections() {
        let m = CardMetrics {
            base: 80.0,
            no_body: -20.0,
            actions: 36.0,
            progress: 14.0,
        };
        assert_eq!(m.height(true, false, false), 80.0);
        assert_eq!(m.height(false, false, false), 60.0);
        assert_eq!(m.height(true, true, true), 130.0);
    }
}
