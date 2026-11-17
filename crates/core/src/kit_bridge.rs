//! Pure mapping from [`GuiraffeTheme`] to the GPUI Controls (Kit) theme fields that
//! [`crate::apply`] writes. Kept GPUI-free so it is unit-tested without a window.

use crate::{
    color::Rgb,
    palette::ColorName,
    theme::{GuiraffeTheme, Scheme},
    tokens::Size,
    variant::Variant,
};

/// Every Kit theme color GUIraffe owns. Kit copies several of these from `primary` only
/// when a theme config loads, so they must be written explicitly or Kit buttons, sliders
/// and progress bars keep Kit's stock primary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KitColors {
    pub primary: Rgb,
    pub primary_hover: Rgb,
    pub primary_foreground: Rgb,
    pub selection: Rgb,
    pub background: Rgb,
    pub foreground: Rgb,
    pub border: Rgb,
    pub muted: Rgb,
    pub muted_foreground: Rgb,
    pub popover: Rgb,
    pub accent: Rgb,
    pub charts: [Rgb; 5],
    pub radius: f32,
}

impl KitColors {
    pub fn from_theme(t: &GuiraffeTheme) -> Self {
        let s = t.surfaces();
        let primary = t.primary_color();
        let chart_shade = match t.scheme {
            Scheme::Light => 6,
            Scheme::Dark => 5,
        };
        // Kit draws single-series charts (line, bar, pie) in `chart_2`, so the primary goes
        // there; the rest are distinct hues that skip the primary.
        let mut others = [
            ColorName::Teal,
            ColorName::Orange,
            ColorName::Grape,
            ColorName::Yellow,
            ColorName::Pink,
        ]
        .into_iter()
        .filter(|c| *c != t.primary)
        .map(|c| c.shade(chart_shade));
        let mut next = || others.next().unwrap_or(primary);
        let charts = [next(), primary, next(), next(), next()];
        Self {
            primary,
            primary_hover: t.variant(None, Variant::Filled).hover,
            primary_foreground: primary.readable_foreground(),
            selection: primary.alpha(0.3).over(s.body),
            background: s.body,
            foreground: s.text,
            border: s.border,
            muted: s.hover,
            muted_foreground: s.dimmed,
            popover: s.raised,
            accent: s.hover,
            charts,
            radius: t.default_radius.radius(),
        }
    }
}

/// Map a GUIraffe size onto Kit's four-step `Sizable` scale, so wrappers can pass
/// `StyleContext::current().control_size` to Kit controls.
pub fn kit_size(size: Size) -> gpui_kit::component::Size {
    use gpui_kit::component::Size as K;
    match size {
        Size::Xs => K::XSmall,
        Size::Sm => K::Small,
        Size::Md => K::Medium,
        Size::Lg | Size::Xl => K::Large,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_family_follows_theme() {
        let t = GuiraffeTheme {
            primary: ColorName::Grape,
            ..Default::default()
        };
        let k = KitColors::from_theme(&t);
        assert_eq!(k.primary, ColorName::Grape.shade(6));
        assert_eq!(k.charts[1], k.primary);
        assert!(k.primary_foreground.contrast(k.primary) >= 3.0);
        assert_ne!(k.selection, k.primary, "selection is a tint");
    }

    #[test]
    fn surfaces_follow_scheme() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let t = GuiraffeTheme {
                scheme,
                ..Default::default()
            };
            let k = KitColors::from_theme(&t);
            assert_eq!(k.background, t.surfaces().body);
            assert!(k.foreground.contrast(k.background) >= 7.0);
            assert!(k.selection.contrast(k.foreground) >= 3.0, "{scheme:?}");
        }
    }

    #[test]
    fn chart_colors_are_distinct() {
        for primary in [ColorName::Blue, ColorName::Teal, ColorName::Orange] {
            let k = KitColors::from_theme(&GuiraffeTheme {
                primary,
                ..Default::default()
            });
            for (i, a) in k.charts.iter().enumerate() {
                for b in &k.charts[i + 1..] {
                    assert_ne!(a, b, "{primary:?}");
                }
            }
        }
    }

    #[test]
    fn kit_size_is_monotonic() {
        use gpui_kit::component::Size as K;
        assert_eq!(kit_size(Size::Xs), K::XSmall);
        assert_eq!(kit_size(Size::Md), K::Medium);
        assert_eq!(kit_size(Size::Xl), K::Large);
    }
}
