//! Text and Title with the theme's type scale.

use gpui_kit::{
    App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, px,
};
use guiraffe_core::{ColorName, Size};

use crate::style::{hsla, surfaces, theme};

#[derive(IntoElement)]
pub struct Text {
    text: SharedString,
    size: Size,
    color: Option<ColorName>,
    dimmed: bool,
    weight: Option<FontWeight>,
    truncate: bool,
    style: StyleRefinement,
}

impl Text {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            size: Size::Md,
            color: None,
            dimmed: false,
            weight: None,
            truncate: false,
            style: StyleRefinement::default(),
        }
    }
}

crate::props!(Text { size: Size => size, dimmed: bool => dimmed, truncate: bool => truncate });

impl Text {
    pub fn c(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }
    pub fn fw(mut self, w: FontWeight) -> Self {
        self.weight = Some(w);
        self
    }
}

impl Styled for Text {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Text {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let t = theme(cx);
        let color = match (self.color, self.dimmed) {
            (Some(c), _) => {
                t.variant(Some(c), guiraffe_core::Variant::Subtle)
                    .foreground
            }
            (None, true) => s.dimmed,
            (None, false) => s.text,
        };
        let fs = self.size.font_size() * t.scale;
        let mut el = div()
            .text_size(px(fs))
            .line_height(px(fs * self.size.line_height()))
            .text_color(hsla(color));
        if let Some(w) = self.weight {
            el = el.font_weight(w);
        }
        if self.truncate {
            el = el.truncate();
        }
        let el = gpui_kit::base::StyledExt::refine_style(el, &self.style);
        el.child(self.text)
    }
}

/// Heading `order` 1..=6 (clamped), sized like Mantine's h1–h6.
#[derive(IntoElement)]
pub struct Title {
    text: SharedString,
    order: u8,
    style: StyleRefinement,
}

impl Title {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            order: 1,
            style: StyleRefinement::default(),
        }
    }
    pub fn order(mut self, order: u8) -> Self {
        self.order = order.clamp(1, 6);
        self
    }
}

/// (font size px, line height factor) for heading orders 1..=6.
pub fn heading_metrics(order: u8) -> (f32, f32) {
    match order.clamp(1, 6) {
        1 => (34.0, 1.3),
        2 => (26.0, 1.35),
        3 => (22.0, 1.4),
        4 => (18.0, 1.45),
        5 => (16.0, 1.5),
        _ => (14.0, 1.5),
    }
}

impl Styled for Title {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Title {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (fs, lh) = heading_metrics(self.order);
        let fs = fs * theme(cx).scale;
        let el = div()
            .text_size(px(fs))
            .line_height(px(fs * lh))
            .font_weight(FontWeight::BOLD)
            .text_color(hsla(surfaces(cx).text));
        let el = gpui_kit::base::StyledExt::refine_style(el, &self.style);
        el.child(self.text)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn heading_scale_is_monotonic_and_clamped() {
        for o in 1..6 {
            assert!(super::heading_metrics(o).0 > super::heading_metrics(o + 1).0);
        }
        assert_eq!(super::heading_metrics(0), super::heading_metrics(1));
        assert_eq!(super::heading_metrics(99), super::heading_metrics(6));
    }
}
