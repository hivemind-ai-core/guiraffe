//! ColorSwatch: a round colour chip with an optional selected ring and label.
//!
//! Colors resolve through the theme's palette (`ColorName` + `Shade`), so a swatch follows the
//! scheme instead of carrying literals (conventions §5). An explicit `Rgb` is an escape hatch for
//! app-defined colours.

use gpui_kit::{
    App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, TestSupportExt, Window, div, prelude::*, px,
};
use guiraffe_core::{
    A11yDisabledExt, ColorName, GuiraffeTheme, Press, PressableExt, Rgb, Shade, Size, Variant,
    keyed_focus,
};

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
    /// `None`: a clickable swatch is a tab stop only inside a focus scope (spec/00 §12.3).
    focusable: Option<bool>,
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
            focusable: None,
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

    /// Opt a clickable swatch in or out of the Tab order.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = Some(focusable);
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let s = surfaces(cx);
        let side = px(swatch_size(size) * scale);
        let ring = hsla(crate::style::theme(cx).primary_color());
        let check = hsla(check_color(self.color));
        let selected = self.selected;
        let disabled = self.disabled;
        let gap = px(ring_gap(size, scale));
        let r = radius(Some(Size::Xs), cx);
        let colors = GuiraffeTheme::global(cx).press_colors(Variant::Subtle, None);
        let chip = div()
            .size(side)
            .rounded(r)
            .bg(hsla(self.color))
            .border_1()
            .border_color(hsla(if disabled {
                colors.disabled.border
            } else {
                s.border
            }))
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
        // The selection ring sits outside the colour with a gap, so it reads on any colour.
        // Its room is always reserved (transparent when unselected): selecting never moves
        // layout (spec/00 §12.2).
        let chip = div()
            .rounded(r)
            .border_2()
            .border_color(if selected == SwatchSelected::Ring {
                ring
            } else {
                gpui_kit::transparent_black()
            })
            .p(gap)
            .child(chip);
        let name: SharedString = self.label.clone().unwrap_or_else(|| {
            {
                let c = self.color;
                format!("Colour #{:02x}{:02x}{:02x}", c.r, c.g, c.b)
            }
            .into()
        });
        let text = if disabled {
            colors.disabled.foreground
        } else {
            s.text
        };
        let el = div()
            .id(self.id.clone())
            .a11y_disabled(disabled)
            .test_support()
            .flex()
            .items_center()
            .gap(px(size.spacing() / 3.0))
            .text_size(px(size.font_size() * scale))
            .text_color(hsla(text))
            .rounded(r)
            .child(chip)
            .when_some(self.label, |el, label| el.child(label))
            .aria_label(name.clone());
        let el = match self.on_click {
            Some(f) => {
                let focus = keyed_focus(self.id, window, cx);
                let press = Press::new(colors)
                    .radius(r)
                    .paint(false)
                    .disabled(disabled)
                    .focusable(self.focusable)
                    .selected(selected != SwatchSelected::None)
                    .label(name);
                el.press(&press, &focus, window)
                    .on_press(disabled, move |event, window, cx| f(event, window, cx))
            }
            None => el,
        };
        crate::style::refine(el, &self.style)
    }
}

/// Space between a swatch and its selection ring.
fn ring_gap(size: Size, scale: f32) -> f32 {
    (swatch_size(size) * scale * 0.16).round().max(2.0)
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
