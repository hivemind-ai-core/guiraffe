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
    pub primary_active: Rgb,
    pub primary_foreground: Rgb,
    pub selection: Rgb,
    pub background: Rgb,
    pub foreground: Rgb,
    pub border: Rgb,
    pub muted: Rgb,
    pub muted_foreground: Rgb,
    pub popover: Rgb,
    pub accent: Rgb,
    /// The semantic families Kit paints its alerts and badges with. They follow
    /// the same rules as the primary: a contrast-corrected filled background
    /// and a foreground that reads on it.
    pub danger: SemanticColors,
    pub success: SemanticColors,
    pub warning: SemanticColors,
    pub info: SemanticColors,
    pub charts: [Rgb; 5],
    pub radius: f32,
}

/// One semantic colour family of Kit: the fill, its hover and pressed
/// shades, and the text that reads on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SemanticColors {
    pub base: Rgb,
    pub hover: Rgb,
    pub active: Rgb,
    pub foreground: Rgb,
}

impl SemanticColors {
    /// The family of a colour, with the same contrast rules as the primary:
    /// the filled variant of the colour (darkened until white text reads on
    /// it) and the foreground that goes with it.
    fn of(t: &GuiraffeTheme, color: ColorName) -> Self {
        let filled = t.variant(Some(color), Variant::Filled);
        Self {
            base: filled.background,
            hover: t
                .state(crate::StateKind::Hover, Variant::Filled, Some(color))
                .background,
            active: t
                .state(crate::StateKind::Pressed, Variant::Filled, Some(color))
                .background,
            foreground: filled.foreground,
        }
    }
}

impl KitColors {
    pub fn from_theme(t: &GuiraffeTheme) -> Self {
        let s = t.surfaces();
        // The contrast-corrected filled background (may be deeper than the raw primary shade).
        let primary = t.variant(None, Variant::Filled).background;
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
            primary_hover: t
                .state(crate::StateKind::Hover, Variant::Filled, None)
                .background,
            primary_active: t
                .state(crate::StateKind::Pressed, Variant::Filled, None)
                .background,
            primary_foreground: t
                .state(crate::StateKind::Rest, Variant::Filled, None)
                .foreground,
            selection: primary.alpha(0.3).over(s.body),
            background: s.body,
            foreground: s.text,
            border: s.border,
            muted: s.hover,
            muted_foreground: s.dimmed,
            popover: s.raised,
            accent: s.hover,
            danger: SemanticColors::of(t, ColorName::Red),
            success: SemanticColors::of(t, ColorName::Green),
            warning: SemanticColors::of(t, ColorName::Yellow),
            info: SemanticColors::of(t, ColorName::Blue),
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
        // Kit's primary is GUIraffe's contrast-corrected filled background, not the raw shade.
        assert_eq!(k.primary, t.variant(None, Variant::Filled).background);
        assert_eq!(k.charts[1], k.primary);
        assert!(k.primary_foreground.contrast(k.primary) >= 4.5);
        // Pressed is distinct from hover and rest, and keeps the text readable.
        assert_ne!(k.primary_active, k.primary_hover);
        assert_ne!(k.primary_active, k.primary);
        assert!(k.primary_foreground.contrast(k.primary_active) >= 4.5);
        assert_ne!(k.selection, k.primary, "selection is a tint");
    }

    #[test]
    fn semantic_families_follow_the_contrast_rules_of_the_primary() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let t = GuiraffeTheme {
                scheme,
                ..Default::default()
            };
            let k = KitColors::from_theme(&t);
            for (name, family) in [
                ("danger", k.danger),
                ("success", k.success),
                ("warning", k.warning),
                ("info", k.info),
            ] {
                // The fill is the contrast-corrected one, so the text reads on
                // it in every state of the family, as the primary does.
                assert!(
                    family.foreground.contrast(family.base) >= 4.5,
                    "{scheme:?} {name} text"
                );
                assert!(
                    family.foreground.contrast(family.hover) >= 4.5,
                    "{scheme:?} {name} hover text"
                );
                assert!(
                    family.foreground.contrast(family.active) >= 4.5,
                    "{scheme:?} {name} pressed text"
                );
                assert_ne!(family.hover, family.base, "{scheme:?} {name} hover");
                assert_ne!(family.active, family.hover, "{scheme:?} {name} pressed");
            }
        }
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
