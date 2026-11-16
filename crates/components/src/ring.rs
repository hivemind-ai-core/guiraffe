//! RingProgress: a donut chart of value sections, drawn as stroked arcs.

use gpui_kit::{
    App, Bounds, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, RenderOnce, SharedString,
    Styled, Window, canvas, div, point, prelude::*, px,
};
use guiraffe_core::{ColorName, Variant};

use crate::style::{hsla, surfaces, variant};

/// Turn percentage `values` into `(start, end)` fractions of a turn (0..=1). Negative and
/// non-finite values count as 0; totals over 100 % are scaled down to fit (Mantine clamps).
pub fn arc_spans(values: &[f32]) -> Vec<(f32, f32)> {
    let clean: Vec<f32> = values
        .iter()
        .map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 })
        .collect();
    let total: f32 = clean.iter().sum();
    let scale = if total > 100.0 { 100.0 / total } else { 1.0 };
    let mut at = 0.0f32;
    clean
        .iter()
        .map(|v| {
            let start = at;
            at = (at + v * scale / 100.0).min(1.0);
            (start, at)
        })
        .collect()
}

#[derive(IntoElement)]
pub struct RingProgress {
    sections: Vec<(f32, ColorName)>,
    size: f32,
    thickness: f32,
    label: Option<SharedString>,
}

impl RingProgress {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            size: 120.0,
            thickness: 12.0,
            label: None,
        }
    }
    pub fn section(mut self, value: f32, color: ColorName) -> Self {
        self.sections.push((value, color));
        self
    }
    pub fn size(mut self, px: f32) -> Self {
        self.size = if px.is_finite() { px.max(8.0) } else { 120.0 };
        self
    }
    pub fn thickness(mut self, px: f32) -> Self {
        self.thickness = if px.is_finite() { px.max(1.0) } else { 12.0 };
        self
    }
    pub fn label(mut self, l: impl Into<SharedString>) -> Self {
        self.label = Some(l.into());
        self
    }
}

impl Default for RingProgress {
    fn default() -> Self {
        Self::new()
    }
}

fn arc(
    bounds: Bounds<Pixels>,
    thickness: f32,
    from: f32,
    to: f32,
    color: Hsla,
    window: &mut Window,
) {
    if to - from <= 1e-4 {
        return;
    }
    let c = bounds.center();
    let r = (f32::from(bounds.size.width.min(bounds.size.height)) - thickness) / 2.0;
    if r <= 0.0 {
        return;
    }
    let at = |f: f32| {
        let a = f * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
    };
    // A full circle can't be one arc command; draw two halves.
    let to = to.min(from + 0.9999);
    let mut b = PathBuilder::stroke(px(thickness));
    b.move_to(at(from));
    let mid = (from + to) / 2.0;
    for (seg_end, large) in [(mid, false), (to, false)] {
        b.arc_to(point(px(r), px(r)), px(0.0), large, true, at(seg_end));
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

impl RenderOnce for RingProgress {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let track = hsla(surfaces(cx).hover);
        let spans = arc_spans(&self.sections.iter().map(|(v, _)| *v).collect::<Vec<_>>());
        let colors: Vec<Hsla> = self
            .sections
            .iter()
            .map(|(_, c)| hsla(variant(cx, Some(*c), Variant::Filled).background))
            .collect();
        let thickness = self.thickness.min(self.size / 2.0);
        let side = px(self.size);
        div()
            .relative()
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        arc(bounds, thickness, 0.0, 1.0, track, window);
                        for ((from, to), color) in spans.iter().zip(&colors) {
                            arc(bounds, thickness, *from, *to, *color, window);
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when_some(self.label, |el, l| el.child(l))
    }
}

#[cfg(test)]
mod tests {
    use super::arc_spans;

    #[test]
    fn spans_accumulate_and_clamp() {
        assert_eq!(arc_spans(&[25.0, 25.0]), vec![(0.0, 0.25), (0.25, 0.5)]);
        let over = arc_spans(&[80.0, 80.0]);
        assert!((over[1].1 - 1.0).abs() < 1e-6, "scaled to fit: {over:?}");
        assert!((over[0].1 - 0.5).abs() < 1e-6);
        assert_eq!(
            arc_spans(&[-5.0, f32::NAN, 10.0]),
            vec![(0.0, 0.0), (0.0, 0.0), (0.0, 0.1)]
        );
        assert!(arc_spans(&[]).is_empty());
    }
}
