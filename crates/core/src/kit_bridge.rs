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

/// One semantic colour family of Kit: the colour of the surface, a shade of it
/// that carries white text, its hover and pressed shades, and the text that
/// reads on it.
///
/// Kit paints with one colour per family in several places: the text of an
/// alert on its tint, the fill of a tag, the text of an outlined tag, the
/// fill of a badge and the fill of a button. The colour therefore has to read
/// as text on a light tint *and* carry text on it, so it is a shade of the hue
/// that reaches the text floor against the surface of the app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SemanticColors {
    /// The colour of the family: the text of an alert on its tint, the fill of
    /// a tag, the text of an outlined tag, the fill of a button.
    pub base: Rgb,
    /// A shade of the family that carries white text: the fill of a badge.
    ///
    /// A colour cannot read on a dark surface and carry white text at the
    /// same time, so the badge takes a shade of its own that is dark in both
    /// schemes.
    pub fill: Rgb,
    /// The hover shade of a fill.
    pub hover: Rgb,
    /// The pressed shade of a fill.
    pub active: Rgb,
    /// The text on a fill.
    pub foreground: Rgb,
}

impl SemanticColors {
    /// The family of a colour, with the same rules as the text of the theme:
    /// the shade of the hue that reads on the surface of the app (see
    /// [`crate::variant::readable_shade`]), and the text that reads on it.
    fn of(t: &GuiraffeTheme, color: ColorName) -> Self {
        let surfaces = t.surfaces();
        let dark = t.scheme == Scheme::Dark;
        let start = t.resolve_ctx().filled_shade();
        // The colour of the family reads on the surface of the app, so it
        // reads on the tint an alert draws over it as well.
        let base = crate::variant::readable_shade(
            color,
            start,
            surfaces.body,
            crate::variant::TEXT_TARGET,
            dark,
        );
        // The hover and pressed shades move away from the surface, so the text
        // on them reads at least as well as on the fill.
        let foreground = crate::variant::filled_foreground(color, base);
        let away = if dark { Rgb::WHITE } else { Rgb::BLACK };
        let hover = shift_toward(base, away, 0.06);
        let active = shift_toward(base, away, 0.12);
        let hover = if foreground.contrast(hover) >= 4.5 {
            hover
        } else {
            base
        };
        let active = if foreground.contrast(active) >= 4.5 {
            active
        } else {
            base
        };
        // The badge of Kit carries white text, so it takes a shade that is
        // dark in both schemes.
        let fill = crate::variant::readable_shade(color, start, Rgb::WHITE, 4.5, false);
        Self {
            base,
            fill,
            hover,
            active,
            foreground,
        }
    }
}

/// Move a colour a little toward another one, so a hover state is a shade of
/// the same hue.
fn shift_toward(color: Rgb, toward: Rgb, amount: f32) -> Rgb {
    let amount = amount.clamp(0., 1.);
    let mix = |a: u8, b: u8| {
        (a as f32 + (b as f32 - a as f32) * amount)
            .round()
            .clamp(0., 255.) as u8
    };
    Rgb {
        r: mix(color.r, toward.r),
        g: mix(color.g, toward.g),
        b: mix(color.b, toward.b),
        a: color.a,
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
    fn semantic_families_read_on_every_surface_kit_paints_them_on() {
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
                let body = t.surfaces().body;
                // An alert paints the colour of the family as its text, over a
                // tint of the same colour: Kit mixes four percent of it into
                // the background of the app.
                let tint = family.base.alpha(0.04).over(body);
                assert!(
                    family.base.contrast(tint) >= 4.5,
                    "{scheme:?} {name} alert text"
                );
                // A tag paints the colour of the family as its fill, with the
                // foreground of the family on it.
                assert!(
                    family.foreground.contrast(family.base) >= 4.5,
                    "{scheme:?} {name} tag text"
                );
                // An outlined tag paints the colour of the family as its text
                // on the surface of the app.
                assert!(
                    family.base.contrast(body) >= 4.5,
                    "{scheme:?} {name} outlined tag text"
                );
                // A badge of Kit carries white text on the fill of the family.
                assert!(
                    Rgb::WHITE.contrast(family.fill) >= 4.5,
                    "{scheme:?} {name} badge text"
                );
                // A button of a semantic variant carries the foreground of the
                // family on the fill, in every state.
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
