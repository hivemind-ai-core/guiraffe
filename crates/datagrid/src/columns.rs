//! Column definitions and the pure column layout (pinning, offsets, hit testing, reorder).

use std::{cmp::Ordering, ops::Range, sync::Arc};

use gpui_kit::SharedString;

use crate::source::CellRef;

/// Horizontal alignment of a column's cells. `Auto` right-aligns numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Auto,
    Start,
    Center,
    End,
}

/// Where a column sits: scrolling with the content, or pinned to an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pin {
    #[default]
    None,
    Left,
    Right,
}

/// Writes a cell's display text into a reused buffer.
pub type Formatter = Arc<dyn Fn(CellRef<'_>, &mut String) + Send + Sync>;
/// Orders two cells of one column (for sorting custom payloads or locale-aware text).
pub type Comparator = Arc<dyn Fn(&CellRef<'_>, &CellRef<'_>) -> Ordering + Send + Sync>;

/// One column. `source` is the source column index passed to [`crate::GridSource::cell`].
#[derive(Clone)]
pub struct ColumnDef {
    pub id: SharedString,
    pub title: SharedString,
    pub source: usize,
    pub width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub align: Align,
    pub formatter: Option<Formatter>,
    pub comparator: Option<Comparator>,
    pub sortable: bool,
    pub resizable: bool,
    pub editable: bool,
    pub visible: bool,
    pub pin: Pin,
}

impl std::fmt::Debug for ColumnDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnDef")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("width", &self.width)
            .field("pin", &self.pin)
            .field("visible", &self.visible)
            .finish_non_exhaustive()
    }
}

impl ColumnDef {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>, source: usize) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            source,
            width: 140.,
            min_width: 48.,
            max_width: 1200.,
            align: Align::Auto,
            formatter: None,
            comparator: None,
            sortable: true,
            resizable: true,
            editable: false,
            visible: true,
            pin: Pin::None,
        }
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = w;
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn formatter(mut self, f: impl Fn(CellRef<'_>, &mut String) + Send + Sync + 'static) -> Self {
        self.formatter = Some(Arc::new(f));
        self
    }
    pub fn comparator(
        mut self,
        f: impl Fn(&CellRef<'_>, &CellRef<'_>) -> Ordering + Send + Sync + 'static,
    ) -> Self {
        self.comparator = Some(Arc::new(f));
        self
    }
    pub fn sortable(mut self, v: bool) -> Self {
        self.sortable = v;
        self
    }
    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }
    pub fn editable(mut self, v: bool) -> Self {
        self.editable = v;
        self
    }
    pub fn visible(mut self, v: bool) -> Self {
        self.visible = v;
        self
    }
    pub fn pin(mut self, p: Pin) -> Self {
        self.pin = p;
        self
    }

    /// Clamp a requested width to `[min, max]` (bad bounds are repaired, never panic).
    pub fn clamp_width(&self, w: f32) -> f32 {
        let min = self.min_width.max(8.);
        let max = self.max_width.max(min);
        if w.is_finite() { w.clamp(min, max) } else { min }
    }

    /// Display text for `cell` into `out` (formatter, else the default).
    pub fn format(&self, cell: CellRef<'_>, out: &mut String) {
        match &self.formatter {
            Some(f) => f(cell, out),
            None => cell.write_display(out),
        }
    }

    pub fn compare(&self, a: &CellRef<'_>, b: &CellRef<'_>) -> Ordering {
        match &self.comparator {
            Some(c) => c(a, b),
            None => a.total_cmp(b),
        }
    }
}

/// A placed column: its definition index, x offset within its group, width and display index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub def: usize,
    pub x: f64,
    pub width: f32,
    pub display: usize,
}

/// Which band a display column belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Left,
    Center,
    Right,
}

/// Visible columns split into pinned-left, scrolling and pinned-right bands, with offsets.
/// Display indices run left band, then center, then right.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColumnLayout {
    pub left: Vec<Placed>,
    pub center: Vec<Placed>,
    pub right: Vec<Placed>,
    pub left_width: f32,
    pub right_width: f32,
    pub center_width: f64,
}

impl ColumnLayout {
    /// Lay out `defs` in user `order` (indices into `defs`; unknown indices are skipped).
    pub fn compute(defs: &[ColumnDef], order: &[usize]) -> Self {
        let mut l = Self::default();
        for band in [Pin::Left, Pin::None, Pin::Right] {
            let mut x = 0.0f64;
            for &d in order {
                let Some(def) = defs.get(d) else { continue };
                if !def.visible || def.pin != band {
                    continue;
                }
                let width = def.clamp_width(def.width);
                let p = Placed {
                    def: d,
                    x,
                    width,
                    display: 0,
                };
                x += f64::from(width);
                match band {
                    Pin::Left => l.left.push(p),
                    Pin::None => l.center.push(p),
                    Pin::Right => l.right.push(p),
                }
            }
            match band {
                Pin::Left => l.left_width = x as f32,
                Pin::None => l.center_width = x,
                Pin::Right => l.right_width = x as f32,
            }
        }
        let mut i = 0;
        for p in l.left.iter_mut().chain(&mut l.center).chain(&mut l.right) {
            p.display = i;
            i += 1;
        }
        l
    }

    pub fn display_count(&self) -> usize {
        self.left.len() + self.center.len() + self.right.len()
    }

    /// Column at display index `i` and its band.
    pub fn display(&self, i: usize) -> Option<(Band, &Placed)> {
        let (nl, nc) = (self.left.len(), self.center.len());
        if i < nl {
            self.left.get(i).map(|p| (Band::Left, p))
        } else if i < nl + nc {
            self.center.get(i - nl).map(|p| (Band::Center, p))
        } else {
            self.right.get(i - nl - nc).map(|p| (Band::Right, p))
        }
    }

    /// Display index of definition `def`, if visible.
    pub fn display_of(&self, def: usize) -> Option<usize> {
        self.left
            .iter()
            .chain(&self.center)
            .chain(&self.right)
            .find(|p| p.def == def)
            .map(|p| p.display)
    }

    /// Width available to the scrolling band in a grid `grid_w` wide.
    pub fn center_viewport(&self, grid_w: f32) -> f32 {
        (grid_w - self.left_width - self.right_width).max(0.)
    }

    /// Largest horizontal scroll offset.
    pub fn max_scroll_x(&self, grid_w: f32) -> f64 {
        (self.center_width - f64::from(self.center_viewport(grid_w))).max(0.)
    }

    /// Center-band columns intersecting the scrolled viewport (indices into `center`).
    /// Binary search: O(log columns).
    pub fn visible_center(&self, scroll_x: f64, grid_w: f32) -> Range<usize> {
        let w = f64::from(self.center_viewport(grid_w));
        let start = self
            .center
            .partition_point(|p| p.x + f64::from(p.width) <= scroll_x);
        let end = self.center.partition_point(|p| p.x < scroll_x + w);
        start..end.max(start)
    }

    /// Screen x (relative to the grid's left edge) of display column `i`.
    pub fn screen_x(&self, i: usize, scroll_x: f64, grid_w: f32) -> Option<f32> {
        let (band, p) = self.display(i)?;
        Some(match band {
            Band::Left => p.x as f32,
            Band::Center => (f64::from(self.left_width) + p.x - scroll_x) as f32,
            Band::Right => grid_w - self.right_width + p.x as f32,
        })
    }

    /// Display column under grid-relative `x`.
    pub fn hit(&self, x: f32, scroll_x: f64, grid_w: f32) -> Option<usize> {
        if x < 0. || x >= grid_w {
            return None;
        }
        let find = |band: &[Placed], bx: f64| {
            let i = band.partition_point(|p| p.x + f64::from(p.width) <= bx);
            band.get(i).filter(|p| p.x <= bx).map(|p| p.display)
        };
        if x < self.left_width {
            return find(&self.left, f64::from(x));
        }
        let right_start = grid_w - self.right_width;
        if x >= right_start {
            return find(&self.right, f64::from(x - right_start));
        }
        find(
            &self.center,
            f64::from(x - self.left_width) + scroll_x,
        )
    }

    /// Display column whose right edge is within `slop` px of `x` (a resize handle).
    pub fn resize_edge(&self, x: f32, scroll_x: f64, grid_w: f32, slop: f32) -> Option<usize> {
        (0..self.display_count()).find(|&i| {
            let (Some(sx), Some((band, p))) = (self.screen_x(i, scroll_x, grid_w), self.display(i))
            else {
                return false;
            };
            let edge = sx + p.width;
            let visible = match band {
                Band::Center => {
                    edge > self.left_width && edge <= grid_w - self.right_width + slop
                }
                _ => true,
            };
            visible && (x - edge).abs() <= slop
        })
    }

    /// Scroll offset that brings display column `i` fully into view (pinned columns are
    /// always visible).
    pub fn scroll_into_view(&self, i: usize, scroll_x: f64, grid_w: f32) -> f64 {
        let Some((Band::Center, p)) = self.display(i) else {
            return scroll_x;
        };
        let w = f64::from(self.center_viewport(grid_w));
        let right = p.x + f64::from(p.width);
        let s = if p.x < scroll_x {
            p.x
        } else if right > scroll_x + w {
            right - w
        } else {
            scroll_x
        };
        s.clamp(0., self.max_scroll_x(grid_w))
    }
}

/// Move definition `def` in `order` so it sits before `before` (or last when `None`).
/// Returns whether the order changed.
pub fn reorder(order: &mut Vec<usize>, def: usize, before: Option<usize>) -> bool {
    if before == Some(def) {
        return false;
    }
    let Some(from) = order.iter().position(|&d| d == def) else {
        return false;
    };
    let old = order.clone();
    order.remove(from);
    let to = before
        .and_then(|b| order.iter().position(|&d| d == b))
        .unwrap_or(order.len());
    order.insert(to, def);
    *order != old
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn defs() -> Vec<ColumnDef> {
        vec![
            ColumnDef::new("a", "A", 0).width(100.).pin(Pin::Left),
            ColumnDef::new("b", "B", 1).width(100.),
            ColumnDef::new("c", "C", 2).width(150.),
            ColumnDef::new("d", "D", 3).width(50.).visible(false),
            ColumnDef::new("e", "E", 4).width(80.).pin(Pin::Right),
            ColumnDef::new("f", "F", 5).width(200.),
        ]
    }

    #[test]
    fn bands_and_display_order() {
        let l = ColumnLayout::compute(&defs(), &[0, 1, 2, 3, 4, 5]);
        assert_eq!(l.left.len(), 1);
        assert_eq!(l.center.iter().map(|p| p.def).collect::<Vec<_>>(), [1, 2, 5]);
        assert_eq!(l.right[0].def, 4);
        assert_eq!(l.display_count(), 5);
        assert_eq!(l.display(4).map(|(b, p)| (b, p.def)), Some((Band::Right, 4)));
        assert_eq!(l.center_width, 450.);
        assert_eq!(l.display_of(3), None);
    }

    #[test]
    fn hit_and_visible_center() {
        let l = ColumnLayout::compute(&defs(), &[0, 1, 2, 3, 4, 5]);
        // grid 400 wide: center viewport 220.
        assert_eq!(l.hit(10., 0., 400.), Some(0));
        assert_eq!(l.hit(150., 0., 400.), Some(1));
        assert_eq!(l.hit(150., 100., 400.), Some(2));
        assert_eq!(l.hit(399., 0., 400.), Some(4));
        assert_eq!(l.visible_center(0., 400.), 0..2);
        assert_eq!(l.visible_center(230., 400.), 1..3);
        assert_eq!(l.max_scroll_x(400.), 230.);
        assert_eq!(l.scroll_into_view(3, 0., 400.), 230.);
        assert_eq!(l.resize_edge(200., 0., 400., 4.), Some(1));
    }

    #[test]
    fn reorder_moves() {
        let mut o = vec![0, 1, 2, 3];
        assert!(reorder(&mut o, 3, Some(1)));
        assert_eq!(o, [0, 3, 1, 2]);
        assert!(reorder(&mut o, 0, None));
        assert_eq!(o, [3, 1, 2, 0]);
        assert!(!reorder(&mut o, 2, Some(0)));
        assert!(!reorder(&mut o, 9, None));
    }

    #[test]
    fn clamp_repairs_bad_bounds() {
        let d = ColumnDef::new("x", "X", 0).min_width(100.).max_width(10.);
        assert_eq!(d.clamp_width(50.), 100.);
        assert_eq!(d.clamp_width(f32::NAN), 100.);
    }

    proptest! {
        #[test]
        fn hit_matches_screen_x(widths in proptest::collection::vec(20f32..300., 1..30),
                                pins in proptest::collection::vec(0u8..3, 30),
                                scroll in 0f64..5000., x in 0f32..900.) {
            let defs: Vec<_> = widths.iter().enumerate().map(|(i, w)| {
                ColumnDef::new("c", "c", i).width(*w).pin(match pins[i] { 0 => Pin::None, 1 => Pin::Left, _ => Pin::Right })
            }).collect();
            let order: Vec<_> = (0..defs.len()).collect();
            let l = ColumnLayout::compute(&defs, &order);
            let gw = 900f32;
            let scroll = scroll.min(l.max_scroll_x(gw));
            if let Some(i) = l.hit(x, scroll, gw) {
                let sx = l.screen_x(i, scroll, gw).unwrap_or(f32::NAN);
                let w = l.display(i).map(|(_, p)| p.width).unwrap_or(0.);
                prop_assert!(sx <= x + 0.01 && x < sx + w + 0.01, "{sx} {w} {x}");
            }
        }

        #[test]
        fn reorder_is_a_permutation(n in 1usize..20, from in 0usize..20, before in proptest::option::of(0usize..20)) {
            let mut o: Vec<_> = (0..n).collect();
            reorder(&mut o, from, before);
            let mut s = o.clone();
            s.sort_unstable();
            prop_assert_eq!(s, (0..n).collect::<Vec<_>>());
        }
    }
}
