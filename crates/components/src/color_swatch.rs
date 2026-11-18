//! ColorSwatch: a round colour chip with an optional selected ring and label.
//!
//! Colors resolve through the theme's palette (`ColorName` + `Shade`), so a swatch follows the
//! scheme instead of carrying literals (conventions §5). An explicit `Rgb` is an escape hatch for
//! app-defined colours.

use gpui_kit::{
    App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, Rgb, Shade, Size};

use crate::style::{hsla, radius, surfaces, theme};

/// How a swatch draws its selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwatchSelected {
    #[default]
    None,
    /// A check mark drawn inside the swatch, in whichever of black/white reads best.
    Check,
    /// A ring around the swatch in the theme's primary colour.
    Ring,
}

/// The colour a check mark should be drawn in on `background` (WCAG contrast, not guesswork).
pub fn check_color(background: Rgb) -> Rgb {
    if background.contrast(Rgb::WHITE) >= background.contrast(Rgb::BLACK) {
        Rgb::WHITE
    } else {
        Rgb::BLACK
    }
}

/// Side length of a swatch at `size`.
pub fn swatch_size(size: Size) -> f32 {
    (size.control_height() * 0.68).round()
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A colour chip: theme palette colour or explicit RGB, optional label and selected state.
///
/// ```
/// use guiraffe_components::{ColorSwatch, SwatchSelected};
/// use guiraffe_core::{ColorName, Shade};
///
/// let teal = ColorSwatch::new("teal", ColorName::Teal.shade(6));
/// let chosen = ColorSwatch::new("chosen", ColorName::Grape.shade(6))
///     .selected(SwatchSelected::Check)
///     .label("Grape");
/// ```
#[derive(IntoElement)]
pub struct ColorSwatch {
    id: ElementId,
    color: Rgb,
    label: Option<SharedString>,
    selected: SwatchSelected,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    disabled: bool,
    on_click: Option<OnClick>,
    style: StyleRefinement,
}

impl ColorSwatch {
    pub fn new(id: impl Into<ElementId>, color: impl Into<Rgb>) -> Self {
        Self {
            id: id.into(),
            color: color.into(),
            label: None,
            selected: SwatchSelected::None,
            size: None,
            disabled: false,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// A palette colour at the accent shade for the current scheme.
    pub fn from_color(id: impl Into<ElementId>, color: ColorName, cx: &App) -> Self {
        let shade = swatch_shade(theme(cx).scheme);
        Self::new(id, color.shade(shade))
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn selected(mut self, selected: SwatchSelected) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

crate::props!(ColorSwatch { size: Size => size, disabled: bool => disabled });
crate::styled!(ColorSwatch);

impl ColorSwatch {
    /// The colour this swatch shows.
    pub fn color(&self) -> Rgb {
        self.color
    }
}

impl RenderOnce for ColorSwatch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let s = surfaces(cx);
        let side = px(swatch_size(size) * scale);
        let ring = hsla(crate::style::theme(cx).primary_color());
        let check = hsla(check_color(self.color));
        let selected = self.selected;
        let disabled = self.disabled;
        let gap = (swatch_size(size) * scale * 0.16).round().max(2.0);
        let chip = div()
            .size(side)
            .rounded(radius(Some(Size::Xs), cx))
            .bg(hsla(self.color))
            .border_1()
            .border_color(hsla(s.border))
            .flex()
            .items_center()
            .justify_center()
            .when(selected == SwatchSelected::Check, |el| {
                el.child(
                    div()
                        .text_size(px(size.font_size() * scale))
                        .text_color(check)
                        .child("✓"),
                )
            });
        // The selected ring sits outside the colour with a gap, so it reads on any colour.
        let chip = if selected == SwatchSelected::Ring {
            div()
                .rounded(radius(Some(Size::Xs), cx))
                .border_2()
                .border_color(ring)
                .p(px(gap))
                .child(chip)
        } else {
            chip
        };
        let mut el = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(size.spacing() / 3.0))
            .text_size(px(size.font_size() * scale))
            .text_color(hsla(s.text))
            .child(chip)
            .when_some(self.label, |el, label| el.child(label));
        if disabled {
            el = el.opacity(0.5);
        } else if let Some(f) = self.on_click {
            el = el
                .cursor_pointer()
                .on_click(move |event, window, cx| f(event, window, cx));
        }
        crate::style::refine(el, &self.style)
    }
}

/// The palette shade a swatch uses in `scheme` (the one `Variant::Filled` uses).
pub fn swatch_shade(scheme: guiraffe_core::Scheme) -> Shade {
    match scheme {
        guiraffe_core::Scheme::Light => 6,
        guiraffe_core::Scheme::Dark => 8,
    }
}

#[cfg(test)]
mod tests {
    use guiraffe_core::{ColorName, Rgb, Scheme};

    use super::{check_color, swatch_shade, swatch_size};

    #[test]
    fn the_check_mark_contrasts_with_the_swatch() {
        assert_eq!(check_color(Rgb::WHITE), Rgb::BLACK, "on white, ink");
        assert_eq!(check_color(Rgb::BLACK), Rgb::WHITE, "on black, paper");
        assert_eq!(check_color(ColorName::Yellow.shade(3)), Rgb::BLACK);
        assert_eq!(check_color(ColorName::Blue.shade(9)), Rgb::WHITE);
        for name in [
            ColorName::Red,
            ColorName::Teal,
            ColorName::Grape,
            ColorName::Lime,
            ColorName::Gray,
        ] {
            let c = check_color(name.shade(6));
            assert!(
                c.contrast(name.shade(6)) >= 4.5,
                "{name:?}: the check stays readable"
            );
        }
    }

    #[test]
    fn sizes_scale_with_the_control() {
        assert!(swatch_size(guiraffe_core::Size::Md) > swatch_size(guiraffe_core::Size::Xs));
        assert!(swatch_size(guiraffe_core::Size::Sm) >= 20.0);
    }

    #[test]
    fn swatches_use_the_variant_shade_per_scheme() {
        assert!(swatch_shade(Scheme::Dark) > swatch_shade(Scheme::Light));
    }
}
