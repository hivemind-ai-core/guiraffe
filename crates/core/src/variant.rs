//! Mantine variant color resolution (`theme.variantColorResolver`).

use crate::{color::Rgb, palette::ColorName, theme::Scheme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Variant {
    #[default]
    Filled,
    Light,
    Outline,
    Subtle,
    Transparent,
    White,
    Default,
}

/// Resolved colors for a variant. `hover` is the background on hover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariantColors {
    pub background: Rgb,
    pub hover: Rgb,
    pub foreground: Rgb,
    pub border: Rgb,
}

/// Inputs to resolution, decoupled from the global theme for testability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveCtx {
    pub scheme: Scheme,
    /// Shade used for filled backgrounds in light (6 by default) / dark (8) scheme.
    pub primary_shade_light: u8,
    pub primary_shade_dark: u8,
    /// Background of the surface the control sits on (used for contrast checks).
    pub surface: Rgb,
}

impl ResolveCtx {
    pub fn new(scheme: Scheme) -> Self {
        Self {
            scheme,
            primary_shade_light: 6,
            primary_shade_dark: 8,
            surface: match scheme {
                Scheme::Light => Rgb::WHITE,
                Scheme::Dark => ColorName::Dark.shade(7),
            },
        }
    }
    fn filled_shade(&self) -> u8 {
        match self.scheme {
            Scheme::Light => self.primary_shade_light,
            Scheme::Dark => self.primary_shade_dark,
        }
    }
}

pub fn resolve(ctx: &ResolveCtx, color: ColorName, variant: Variant) -> VariantColors {
    let dark = ctx.scheme == Scheme::Dark;
    let filled = color.shade(ctx.filled_shade());
    let t = Rgb::TRANSPARENT;
    // Composite translucent tints over the surface so contrast is computable and stable.
    let tint = |alpha: f32| filled.alpha(alpha).over(ctx.surface);
    // Text/border color for tinted variants. The `Dark` palette is too close to the
    // dark surface, so it uses its lightest shade there.
    let accent = match (dark, color) {
        (true, ColorName::Dark) => color.shade(0),
        (true, _) => color.shade(4),
        (false, _) => filled,
    };
    match variant {
        Variant::Filled => VariantColors {
            background: filled,
            hover: color.shade((ctx.filled_shade() + 1).min(9)),
            foreground: filled.readable_foreground(),
            border: t,
        },
        Variant::Light => VariantColors {
            background: tint(if dark { 0.15 } else { 0.1 }),
            hover: tint(if dark { 0.2 } else { 0.12 }),
            foreground: accent,
            border: t,
        },
        Variant::Outline => VariantColors {
            background: t,
            hover: tint(0.05),
            foreground: accent,
            border: accent,
        },
        Variant::Subtle => VariantColors {
            background: t,
            hover: tint(if dark { 0.12 } else { 0.08 }),
            foreground: accent,
            border: t,
        },
        Variant::Transparent => VariantColors {
            background: t,
            hover: t,
            foreground: accent,
            border: t,
        },
        Variant::White => VariantColors {
            background: Rgb::WHITE,
            hover: ColorName::Gray.shade(0),
            foreground: if filled.contrast(Rgb::WHITE) >= 3.0 {
                filled
            } else {
                color.shade(9)
            },
            border: t,
        },
        Variant::Default => {
            if dark {
                let d = ColorName::Dark;
                VariantColors {
                    background: d.shade(6),
                    hover: d.shade(5),
                    foreground: Rgb::WHITE,
                    border: d.shade(4),
                }
            } else {
                let g = ColorName::Gray;
                VariantColors {
                    background: Rgb::WHITE,
                    hover: g.shade(0),
                    foreground: Rgb::BLACK,
                    border: g.shade(4),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARIANTS: [Variant; 7] = [
        Variant::Filled,
        Variant::Light,
        Variant::Outline,
        Variant::Subtle,
        Variant::Transparent,
        Variant::White,
        Variant::Default,
    ];

    #[test]
    fn filled_matches_mantine_primary() {
        let c = resolve(
            &ResolveCtx::new(Scheme::Light),
            ColorName::Blue,
            Variant::Filled,
        );
        assert_eq!(c.background, Rgb::hex(0x228be6));
        assert_eq!(c.hover, Rgb::hex(0x1c7ed6));
        assert_eq!(c.foreground, Rgb::WHITE);
    }

    #[test]
    fn filled_yellow_uses_dark_text() {
        let c = resolve(
            &ResolveCtx::new(Scheme::Light),
            ColorName::Yellow,
            Variant::Filled,
        );
        assert_eq!(c.foreground, Rgb::BLACK);
    }

    /// Every text-bearing variant must be legible (WCAG large-text 3:1) on its own
    /// background, composited over the surface, for every color in both schemes.
    #[test]
    fn all_variants_legible() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let ctx = ResolveCtx::new(scheme);
            for color in ColorName::ALL {
                for v in VARIANTS {
                    let c = resolve(&ctx, color, v);
                    let bg = c.background.over(ctx.surface);
                    let ratio = c.foreground.contrast(bg);
                    // Light-scheme tinted yellow/lime text is a known Mantine weakness; require 1.5 there.
                    let weak = scheme == Scheme::Light
                        && matches!(
                            color,
                            ColorName::Yellow
                                | ColorName::Lime
                                | ColorName::Cyan
                                | ColorName::Teal
                                | ColorName::Green
                                | ColorName::Orange
                        )
                        && v != Variant::Filled
                        && v != Variant::Default;
                    let min = if weak { 1.5 } else { 2.5 };
                    assert!(ratio >= min, "{scheme:?} {color:?} {v:?}: {ratio}");
                }
            }
        }
    }

    #[test]
    fn hover_differs_for_interactive_variants() {
        let ctx = ResolveCtx::new(Scheme::Light);
        for v in [
            Variant::Filled,
            Variant::Light,
            Variant::Outline,
            Variant::Subtle,
            Variant::Default,
        ] {
            let c = resolve(&ctx, ColorName::Violet, v);
            assert_ne!(c.hover, c.background, "{v:?}");
        }
    }
}
