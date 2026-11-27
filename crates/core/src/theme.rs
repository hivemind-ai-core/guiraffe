//! The GUIraffe theme global and its bridge into GPUI Controls' theme.

use gpui_kit::{App, Global, Hsla, px};

use crate::{
    color::Rgb,
    palette::ColorName,
    tokens::Size,
    variant::{self, ResolveCtx, Variant, VariantColors},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Scheme {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuiraffeTheme {
    pub scheme: Scheme,
    pub primary: ColorName,
    pub primary_shade_light: u8,
    pub primary_shade_dark: u8,
    pub default_radius: Size,
    /// Global font scale multiplier applied to `Size::font_size`.
    pub scale: f32,
    /// Layout recipes: what each kind of container provides to the widgets inside it.
    pub recipes: Recipes,
    /// Contrast setting (API conventions §12.1).
    pub contrast: crate::state::Contrast,
    /// The neutral surface ladder of the light scheme.
    pub surfaces_light: SurfaceShades,
    /// The neutral surface ladder of the dark scheme.
    pub surfaces_dark: SurfaceShades,
}

/// One scheme's neutral surface ladder. Adjacent levels (body and surface, surface and
/// raised, surface and sunken, body and sunken) differ by at least 1.08:1, so panels, cards
/// and wells separate without borders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceShades {
    /// The window behind everything.
    pub body: Rgb,
    /// Panels and cards on the body.
    pub surface: Rgb,
    /// Popovers and the active element on a track.
    pub raised: Rgb,
    /// Recessed areas: wells, recessed fields, meter tracks, plot backgrounds.
    pub sunken: Rgb,
    /// Dividers and outlines.
    pub border: Rgb,
}

impl SurfaceShades {
    /// Light default: white body, gray 1 panels, white raised, gray 3 wells.
    pub const LIGHT: Self = Self {
        body: Rgb::WHITE,
        surface: Rgb::hex(0xf1f3f5),
        raised: Rgb::WHITE,
        sunken: Rgb::hex(0xdee2e6),
        border: Rgb::hex(0xdee2e6),
    };
    /// Dark default: dark 7 body, dark 6 panels, dark 5 raised, dark 9 wells.
    pub const DARK: Self = Self {
        body: Rgb::hex(0x242424),
        surface: Rgb::hex(0x2e2e2e),
        raised: Rgb::hex(0x3b3b3b),
        sunken: Rgb::hex(0x141414),
        border: Rgb::hex(0x424242),
    };
    /// The adjacent pairs that must separate, with their names (for tests and audits).
    pub fn adjacent(&self) -> [(&'static str, Rgb, Rgb); 4] {
        [
            ("body/surface", self.body, self.surface),
            ("surface/raised", self.surface, self.raised),
            ("surface/sunken", self.surface, self.sunken),
            ("body/sunken", self.body, self.sunken),
        ]
    }
}

/// Minimum contrast between adjacent surface levels.
pub const SURFACE_STEP: f32 = 1.08;

/// Style recipes per layout kind. Containers apply these as [`crate::StylePatch`]es;
/// restyle an app globally by editing recipes rather than call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Recipes {
    pub page: crate::StylePatch,
    pub toolbar: crate::StylePatch,
    pub form: crate::StylePatch,
    pub card: crate::StylePatch,
    pub sidebar: crate::StylePatch,
    pub table: crate::StylePatch,
}

impl Recipes {
    pub const DEFAULT: Recipes = {
        use crate::{Density, Emphasis, StylePatch, SurfaceKind};
        Recipes {
            page: StylePatch::new().density(Density::Comfortable),
            toolbar: StylePatch::new()
                .control_size(Size::Xs)
                .emphasis(Emphasis::Quiet),
            form: StylePatch::new()
                .control_size(Size::Sm)
                .emphasis(Emphasis::Default),
            card: StylePatch::new().surface(SurfaceKind::Raised),
            sidebar: StylePatch::new()
                .surface(SurfaceKind::Sunken)
                .control_size(Size::Sm)
                .emphasis(Emphasis::Quiet),
            table: StylePatch::new()
                .density(Density::Compact)
                .emphasis(Emphasis::Quiet),
        }
    };
}

impl Default for Recipes {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Default for GuiraffeTheme {
    fn default() -> Self {
        Self {
            scheme: Scheme::Light,
            primary: ColorName::Blue,
            primary_shade_light: 6,
            primary_shade_dark: 8,
            default_radius: Size::Sm,
            scale: 1.0,
            recipes: Recipes::DEFAULT,
            contrast: crate::state::Contrast::Standard,
            surfaces_light: SurfaceShades::LIGHT,
            surfaces_dark: SurfaceShades::DARK,
        }
    }
}

impl Global for GuiraffeTheme {}

/// Semantic surface colors, derived from scheme (Mantine `--mantine-color-body` etc.).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surfaces {
    pub body: Rgb,
    pub surface: Rgb,
    pub text: Rgb,
    pub dimmed: Rgb,
    pub border: Rgb,
    pub hover: Rgb,
    /// Recessed track behind segmented controls, progress rings, pills.
    pub track: Rgb,
    /// Raised element on a track (active segment).
    pub raised: Rgb,
    /// Recessed areas: wells, recessed fields, meter tracks, plot backgrounds.
    pub sunken: Rgb,
    /// Background for highlighted/marked text.
    pub mark: Rgb,
    /// Multiplier for shadow opacity (dark surfaces need stronger shadows to read).
    pub shadow_strength: f32,
}

impl Surfaces {
    /// The colour a region of `kind` paints.
    pub fn of(&self, kind: crate::context::SurfaceKind) -> Rgb {
        match kind {
            crate::context::SurfaceKind::Body => self.body,
            crate::context::SurfaceKind::Raised => self.raised,
            crate::context::SurfaceKind::Sunken => self.sunken,
        }
    }
}

/// The shade of `color` nearest to `preferred` that clears `min` contrast on `background`,
/// searching away from the background's lightness first. Falls back to the family's
/// highest-contrast shade when none clears `min`.
pub fn readable_shade(color: ColorName, preferred: u8, background: Rgb, min: f32) -> Rgb {
    let shades = color.shades();
    let preferred = usize::from(preferred.min(9));
    // On a light background darker (higher) shades gain contrast; on a dark one, lighter.
    let darker_first = background.luminance() > 0.18;
    let mut best = shades[preferred];
    for step in 0..10usize {
        let ahead = if darker_first {
            preferred.checked_add(step)
        } else {
            preferred.checked_sub(step)
        };
        let behind = if darker_first {
            preferred.checked_sub(step)
        } else {
            preferred.checked_add(step)
        };
        for candidate in [ahead, behind].into_iter().flatten().filter(|i| *i < 10) {
            let rgb = shades[candidate];
            if rgb.contrast(background) >= min {
                return rgb;
            }
            if rgb.contrast(background) > best.contrast(background) {
                best = rgb;
            }
        }
    }
    best
}

impl GuiraffeTheme {
    pub fn global(cx: &App) -> &Self {
        cx.try_global::<Self>().unwrap_or(&DEFAULT)
    }

    pub fn resolve_ctx(&self) -> ResolveCtx {
        ResolveCtx {
            primary_shade_light: self.primary_shade_light,
            primary_shade_dark: self.primary_shade_dark,
            ..ResolveCtx::new(self.scheme)
        }
    }

    pub fn variant(&self, color: Option<ColorName>, v: Variant) -> VariantColors {
        variant::resolve(&self.resolve_ctx(), color.unwrap_or(self.primary), v)
    }

    pub fn primary_color(&self) -> Rgb {
        let shade = match self.scheme {
            Scheme::Light => self.primary_shade_light,
            Scheme::Dark => self.primary_shade_dark,
        };
        self.primary.shade(shade)
    }

    /// The highlight of the active item in a menu, dropdown or select list: the item under
    /// the pointer and the item the arrow keys moved to are one highlight. Returns the fill and
    /// the text on it. The fill is the primary colour at the shade nearest the theme's own
    /// that clears 3:1 against the menu surface, so it reads as a highlight in both schemes.
    pub fn menu_highlight(&self) -> (Rgb, Rgb) {
        let menu = self.surfaces().raised;
        let base = match self.scheme {
            Scheme::Light => self.primary_shade_light,
            Scheme::Dark => self.primary_shade_dark,
        };
        // Search away from the menu surface: darker on light, lighter on dark. A primary
        // with no shade that clears it (yellow on white) falls back to gray.
        let away = |color: ColorName| -> Vec<Rgb> {
            match self.scheme {
                Scheme::Light => (base..=9).map(|s| color.shade(s)).collect(),
                Scheme::Dark => (0..=base.min(9)).rev().map(|s| color.shade(s)).collect(),
            }
        };
        let (color, fill) = [self.primary, ColorName::Gray]
            .into_iter()
            .find_map(|color| {
                away(color)
                    .into_iter()
                    .find(|fill| fill.contrast(menu) >= crate::state::NON_TEXT_MIN)
                    .map(|fill| (color, fill))
            })
            .unwrap_or((self.primary, self.primary.shade(base)));
        (fill, crate::variant::filled_foreground(color, fill))
    }

    /// The scheme's surface ladder.
    pub fn surface_shades(&self) -> &SurfaceShades {
        match self.scheme {
            Scheme::Light => &self.surfaces_light,
            Scheme::Dark => &self.surfaces_dark,
        }
    }

    pub fn surfaces(&self) -> Surfaces {
        let (g, d) = (ColorName::Gray, ColorName::Dark);
        let l = self.surface_shades();
        match self.scheme {
            Scheme::Light => Surfaces {
                body: l.body,
                surface: l.surface,
                text: Rgb::BLACK,
                // Gray 6 is only 3.3:1 on white; halfway to gray 7 clears 4.5:1.
                dimmed: g.shade(7).alpha(0.5).over(g.shade(6)),
                border: l.border,
                hover: g.shade(1),
                track: g.shade(1),
                raised: l.raised,
                sunken: l.sunken,
                mark: ColorName::Yellow.shade(2),
                shadow_strength: 1.0,
            },
            Scheme::Dark => Surfaces {
                body: l.body,
                surface: l.surface,
                text: d.shade(0),
                // Dark 2 is 4.0:1 on the dark body; dark 1 clears 4.5:1.
                dimmed: d.shade(1),
                border: l.border,
                hover: d.shade(5),
                track: d.shade(6),
                raised: l.raised,
                sunken: l.sunken,
                mark: ColorName::Yellow.shade(8).alpha(0.35).over(d.shade(7)),
                shadow_strength: 4.0,
            },
        }
    }

    /// Theme-resolved box shadow layers for a shadow token.
    pub fn shadow(&self, size: Size) -> [crate::tokens::ShadowLayer; 2] {
        let k = self.surfaces().shadow_strength;
        crate::tokens::shadow(size).map(|l| crate::tokens::ShadowLayer {
            alpha: (l.alpha * k).min(1.0),
            ..l
        })
    }

    pub fn hsla(c: Rgb) -> Hsla {
        c.to_hsla()
    }
}

static DEFAULT: GuiraffeTheme = GuiraffeTheme {
    scheme: Scheme::Light,
    primary: ColorName::Blue,
    primary_shade_light: 6,
    primary_shade_dark: 8,
    default_radius: Size::Sm,
    scale: 1.0,
    recipes: Recipes::DEFAULT,
    contrast: crate::state::Contrast::Standard,
    surfaces_light: SurfaceShades::LIGHT,
    surfaces_dark: SurfaceShades::DARK,
};

/// Install `theme` and push its primary color, radius and scheme into Controls' theme so
/// stock gpui-component widgets match GUIraffe widgets.
pub fn apply(theme: GuiraffeTheme, cx: &mut App) {
    use gpui_kit::component::theme::{Theme, ThemeMode};
    let mode = match theme.scheme {
        Scheme::Light => ThemeMode::Light,
        Scheme::Dark => ThemeMode::Dark,
    };
    if cx.has_global::<Theme>() {
        if Theme::global(cx).mode != mode {
            Theme::change(mode, None, cx);
        }
        let k = crate::kit_bridge::KitColors::from_theme(&theme);
        Theme::update(cx, |t| {
            let (p, ph, pa, pf) = (
                k.primary.to_hsla(),
                k.primary_hover.to_hsla(),
                k.primary_active.to_hsla(),
                k.primary_foreground.to_hsla(),
            );
            t.primary = p;
            t.primary_hover = ph;
            t.primary_active = pa;
            t.primary_foreground = pf;
            t.button_primary = p;
            t.button_primary_hover = ph;
            t.button_primary_active = pa;
            t.button_primary_foreground = pf;
            t.sidebar_primary = p;
            t.sidebar_primary_foreground = pf;
            t.progress_bar = p;
            t.slider_bar = p;
            t.slider_thumb = p;
            t.ring = p;
            t.caret = p;
            t.link = p;
            t.link_hover = ph;
            t.link_active = ph;
            t.list_active = k.selection.to_hsla();
            t.list_active_border = p;
            t.selection = k.selection.to_hsla();
            t.background = k.background.to_hsla();
            t.foreground = k.foreground.to_hsla();
            t.border = k.border.to_hsla();
            t.input = k.border.to_hsla();
            t.muted = k.muted.to_hsla();
            t.muted_foreground = k.muted_foreground.to_hsla();
            t.popover = k.popover.to_hsla();
            t.popover_foreground = k.foreground.to_hsla();
            t.accent = k.accent.to_hsla();
            t.accent_foreground = k.foreground.to_hsla();
            // The semantic families follow the same rules as the primary. One
            // family paints the text of an alert on its tint, the fill of a
            // tag, the text of an outlined tag, the fill of a button and the
            // fill of a badge.
            let (danger, success, warning, info) = (k.danger, k.success, k.warning, k.info);
            (
                t.danger,
                t.danger_hover,
                t.danger_active,
                t.danger_foreground,
            ) = (
                danger.base.to_hsla(),
                danger.hover.to_hsla(),
                danger.active.to_hsla(),
                danger.foreground.to_hsla(),
            );
            (
                t.success,
                t.success_hover,
                t.success_active,
                t.success_foreground,
            ) = (
                success.base.to_hsla(),
                success.hover.to_hsla(),
                success.active.to_hsla(),
                success.foreground.to_hsla(),
            );
            (
                t.warning,
                t.warning_hover,
                t.warning_active,
                t.warning_foreground,
            ) = (
                warning.base.to_hsla(),
                warning.hover.to_hsla(),
                warning.active.to_hsla(),
                warning.foreground.to_hsla(),
            );
            (t.info, t.info_hover, t.info_active, t.info_foreground) = (
                info.base.to_hsla(),
                info.hover.to_hsla(),
                info.active.to_hsla(),
                info.foreground.to_hsla(),
            );
            // The buttons of the semantic variants take the same family.
            (
                t.button_danger,
                t.button_danger_hover,
                t.button_danger_active,
                t.button_danger_foreground,
            ) = (
                danger.base.to_hsla(),
                danger.hover.to_hsla(),
                danger.active.to_hsla(),
                danger.foreground.to_hsla(),
            );
            (
                t.button_success,
                t.button_success_hover,
                t.button_success_active,
                t.button_success_foreground,
            ) = (
                success.base.to_hsla(),
                success.hover.to_hsla(),
                success.active.to_hsla(),
                success.foreground.to_hsla(),
            );
            (
                t.button_warning,
                t.button_warning_hover,
                t.button_warning_active,
                t.button_warning_foreground,
            ) = (
                warning.base.to_hsla(),
                warning.hover.to_hsla(),
                warning.active.to_hsla(),
                warning.foreground.to_hsla(),
            );
            (
                t.button_info,
                t.button_info_hover,
                t.button_info_active,
                t.button_info_foreground,
            ) = (
                info.base.to_hsla(),
                info.hover.to_hsla(),
                info.active.to_hsla(),
                info.foreground.to_hsla(),
            );
            // The badge of Kit paints itself with the danger colour and
            // carries white text, so it takes the shade of the family that is
            // dark in both schemes.
            t.red = danger.fill.to_hsla();
            [t.chart_1, t.chart_2, t.chart_3, t.chart_4, t.chart_5] = k.charts.map(Rgb::to_hsla);
            t.radius = px(k.radius);
        });
    }
    // One highlight for the active item of every menu, dropdown and select list.
    let (fill, text) = theme.menu_highlight();
    cx.set_global(gpui_kit::component::menu::MenuHighlight {
        background: fill.to_hsla(),
        foreground: text.to_hsla(),
    });
    cx.set_global(theme);
    cx.refresh_windows();
}

/// Change only the scheme, keeping everything else.
pub fn set_scheme(scheme: Scheme, cx: &mut App) {
    let mut t = GuiraffeTheme::global(cx).clone();
    t.scheme = scheme;
    apply(t, cx);
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn surfaces_are_legible_and_distinct_in_both_schemes() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let t = GuiraffeTheme {
                scheme,
                ..Default::default()
            };
            let s = t.surfaces();
            assert!(s.text.contrast(s.body) >= 7.0, "{scheme:?} body text AAA");
            assert!(s.text.contrast(s.mark) >= 4.5, "{scheme:?} marked text AA");
            assert!(s.dimmed.contrast(s.body) >= 3.0, "{scheme:?} dimmed text");
            assert_ne!(s.track, s.body, "{scheme:?} track visible");
            assert_ne!(s.raised, s.track, "{scheme:?} raised visible on track");
            for (pair, a, b) in t.surface_shades().adjacent() {
                assert!(
                    a.contrast(b) >= SURFACE_STEP,
                    "{scheme:?} {pair}: {:.3}:1 < {SURFACE_STEP}:1",
                    a.contrast(b)
                );
            }
            assert_eq!(s.of(crate::context::SurfaceKind::Sunken), s.sunken);
            assert!(
                s.text.contrast(s.sunken) >= 4.5,
                "{scheme:?} text on a well"
            );
        }
    }

    #[test]
    fn readable_shade_walks_to_the_nearest_passing_shade() {
        let white = Rgb::WHITE;
        // Orange 6 fails 3:1 on white; the nearest darker passing shade is chosen.
        let pick = readable_shade(ColorName::Orange, 6, white, 3.0);
        assert!(pick.contrast(white) >= 3.0);
        assert_ne!(pick, ColorName::Orange.shade(6));
        // A passing preferred shade is kept.
        assert_eq!(
            readable_shade(ColorName::Blue, 8, white, 3.0),
            ColorName::Blue.shade(8)
        );
        // On a dark background it walks towards lighter shades.
        let dark = ColorName::Dark.shade(8);
        assert!(readable_shade(ColorName::Violet, 8, dark, 3.0).contrast(dark) >= 3.0);
        // Impossible targets return the best available, never panic.
        let best = readable_shade(ColorName::Gray, 0, white, 21.0);
        assert!(best.contrast(white) > 5.0);
    }

    #[test]
    fn dark_shadows_are_stronger() {
        let l = GuiraffeTheme::default().shadow(Size::Md);
        let d = GuiraffeTheme {
            scheme: Scheme::Dark,
            ..Default::default()
        }
        .shadow(Size::Md);
        assert!(d[1].alpha > l[1].alpha);
    }
}
