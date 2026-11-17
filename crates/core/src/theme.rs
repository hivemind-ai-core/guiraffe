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
}

/// Style recipes per layout kind (spec/14). Containers apply these as [`crate::StylePatch`]es;
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
    /// Background for highlighted/marked text.
    pub mark: Rgb,
    /// Multiplier for shadow opacity (dark surfaces need stronger shadows to read).
    pub shadow_strength: f32,
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

    pub fn surfaces(&self) -> Surfaces {
        let (g, d) = (ColorName::Gray, ColorName::Dark);
        match self.scheme {
            Scheme::Light => Surfaces {
                body: Rgb::WHITE,
                surface: g.shade(0),
                text: Rgb::BLACK,
                dimmed: g.shade(6),
                border: g.shade(3),
                hover: g.shade(1),
                track: g.shade(1),
                raised: Rgb::WHITE,
                mark: ColorName::Yellow.shade(2),
                shadow_strength: 1.0,
            },
            Scheme::Dark => Surfaces {
                body: d.shade(7),
                surface: d.shade(6),
                text: d.shade(0),
                dimmed: d.shade(2),
                border: d.shade(4),
                hover: d.shade(5),
                track: d.shade(6),
                raised: d.shade(5),
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
            let (p, ph, pf) = (
                k.primary.to_hsla(),
                k.primary_hover.to_hsla(),
                k.primary_foreground.to_hsla(),
            );
            t.primary = p;
            t.primary_hover = ph;
            t.primary_active = ph;
            t.primary_foreground = pf;
            t.button_primary = p;
            t.button_primary_hover = ph;
            t.button_primary_active = ph;
            t.button_primary_foreground = pf;
            t.sidebar_primary = p;
            t.sidebar_primary_foreground = pf;
            t.progress_bar = p;
            t.slider_bar = p;
            t.slider_thumb = p;
            t.switch_thumb = k.background.to_hsla();
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
            [t.chart_1, t.chart_2, t.chart_3, t.chart_4, t.chart_5] = k.charts.map(Rgb::to_hsla);
            t.radius = px(k.radius);
        });
    }
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
        }
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
