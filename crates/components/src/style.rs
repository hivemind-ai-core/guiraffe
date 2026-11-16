//! Shared styling helpers: every component draws through these, so all colors, radii,
//! spacing and shadows come from the theme (conventions §5).

use gpui_kit::{App, BoxShadow, Hsla, Pixels, point, px};
use guiraffe_core::{
    ColorName, GuiraffeTheme, Rgb, Size, Surfaces, Variant, VariantColors, shadow,
};

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

/// GPUI box shadows for a shadow token.
pub fn shadows(size: Size, cx: &App) -> Vec<BoxShadow> {
    let black = if theme(cx).scheme == guiraffe_core::Scheme::Dark {
        Rgb::BLACK
    } else {
        Rgb::hex(0x000000)
    };
    shadow(size)
        .iter()
        .map(|l| BoxShadow {
            color: black
                .alpha(if theme(cx).scheme == guiraffe_core::Scheme::Dark {
                    l.alpha * 4.0
                } else {
                    l.alpha
                })
                .to_hsla(),
            offset: point(px(0.0), px(l.y)),
            blur_radius: px(l.blur),
            spread_radius: px(l.spread),
            inset: false,
        })
        .collect()
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
