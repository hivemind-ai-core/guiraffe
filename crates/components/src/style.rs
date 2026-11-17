//! Shared styling helpers: every component draws through these, so all colors, radii,
//! spacing and shadows come from the theme (conventions §5).

use gpui_kit::{App, BoxShadow, Hsla, Pixels, point, px};
use guiraffe_core::{ColorName, GuiraffeTheme, Rgb, Size, Surfaces, Variant, VariantColors};

pub fn theme(cx: &App) -> &GuiraffeTheme {
    GuiraffeTheme::global(cx)
}

pub fn surfaces(cx: &App) -> Surfaces {
    theme(cx).surfaces()
}

pub fn hsla(c: Rgb) -> Hsla {
    c.to_hsla()
}

pub fn variant(cx: &App, color: Option<ColorName>, v: Variant) -> VariantColors {
    theme(cx).variant(color, v)
}

pub fn radius(r: Option<Size>, cx: &App) -> Pixels {
    px(r.unwrap_or(theme(cx).default_radius).radius())
}

/// GPUI box shadows for a shadow token, resolved for the current scheme.
pub fn shadows(size: Size, cx: &App) -> Vec<BoxShadow> {
    theme(cx)
        .shadow(size)
        .iter()
        .map(|l| BoxShadow {
            color: Rgb::BLACK.alpha(l.alpha).to_hsla(),
            offset: point(px(0.0), px(l.y)),
            blur_radius: px(l.blur),
            spread_radius: px(l.spread),
            inset: false,
        })
        .collect()
}

/// Metrics derived from the [`Size`] scale, so components scale together instead of
/// carrying their own pixel constants.
pub mod metrics {
    use guiraffe_core::Size;

    /// Small inner gaps (icon to label, pill content).
    pub fn gap(size: Size) -> f32 {
        (size.spacing() / 3.0).round()
    }
    /// Pill / badge height.
    pub fn pill_height(size: Size) -> f32 {
        (size.control_height() * 0.6).round()
    }
    /// Indicator dot diameter, and the height of a labelled indicator.
    pub fn dot(size: Size) -> f32 {
        (size.font_size() * 0.625).round()
    }
    pub fn badge_height(size: Size) -> f32 {
        size.font_size()
    }
    /// Inset of tracks (segmented control padding).
    pub fn track_pad(size: Size) -> f32 {
        (size.spacing() / 3.0).round()
    }
    /// Timeline bullet diameter (Mantine: 12 px at the default size).
    pub fn bullet(size: Size) -> f32 {
        (size.font_size() * 0.75).round().max(8.0)
    }
    /// Timeline connector width.
    pub fn line_width(size: Size) -> f32 {
        (size.radius() / 2.0).max(1.0)
    }
}

/// Builder methods shared by components, generated so every component spells props alike.
#[macro_export]
#[doc(hidden)]
macro_rules! props {
    ($ty:ty { $($name:ident : $t:ty => $field:ident),* $(,)? }) => {
        impl $ty {
            $(
                pub fn $name(mut self, value: $t) -> Self {
                    self.$field = value.into();
                    self
                }
            )*
        }
    };
}

/// Implement `Styled` for a component with a `style: StyleRefinement` field; the component
/// applies it last in `render` (callers can always override; conventions §5).
#[macro_export]
#[doc(hidden)]
macro_rules! styled {
    ($($ty:ty),*) => {
        $(
            impl gpui_kit::Styled for $ty {
                fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
                    &mut self.style
                }
            }
        )*
    };
}

/// Apply caller style overrides to a root element.
pub fn refine<E: gpui_kit::Styled>(el: E, style: &gpui_kit::StyleRefinement) -> E {
    gpui_kit::base::StyledExt::refine_style(el, style)
}

#[cfg(test)]
mod tests {
    use guiraffe_core::Size;

    #[test]
    fn metrics_scale_monotonically() {
        for w in Size::ALL.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(super::metrics::pill_height(a) < super::metrics::pill_height(b));
            assert!(super::metrics::dot(a) <= super::metrics::dot(b));
            assert!(super::metrics::gap(a) <= super::metrics::gap(b));
        }
        assert_eq!(super::metrics::track_pad(Size::Sm), 4.0);
    }
}
