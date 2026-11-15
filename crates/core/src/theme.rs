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
        }
    }
}

impl Global for GuiraffeTheme {}

/// Semantic surface colors, derived from scheme (Mantine `--mantine-color-body` etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surfaces {
    pub body: Rgb,
    pub surface: Rgb,
    pub text: Rgb,
    pub dimmed: Rgb,
    pub border: Rgb,
    pub hover: Rgb,
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
            },
            Scheme::Dark => Surfaces {
                body: d.shade(7),
                surface: d.shade(6),
                text: d.shade(0),
                dimmed: d.shade(2),
                border: d.shade(4),
                hover: d.shade(5),
            },
        }
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
        let primary = theme.primary_color();
        let hover = theme.variant(None, Variant::Filled).hover;
        let radius = theme.default_radius.radius();
        Theme::update(cx, |t| {
            t.primary = primary.to_hsla();
            t.primary_hover = hover.to_hsla();
            t.primary_active = hover.to_hsla();
            t.primary_foreground = primary.readable_foreground().to_hsla();
            t.ring = primary.to_hsla();
            t.radius = px(radius);
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
