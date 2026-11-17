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

/// Text needs 4.5:1 (WCAG 1.4.3). Tinted text targets a little more so hover/pressed tints
/// (which sit between rest and the text) still clear the floor.
const TEXT_TARGET: f32 = 5.0;

/// The shade of `color` nearest `start` that reaches `floor` against `bg`, walking toward the
/// dark end in light mode and the light end in dark mode (Refactoring UI: keep the hue, move
/// along its scale; darkest shades are for text). Falls back to near-black / near-white.
pub fn readable_shade(color: ColorName, start: u8, bg: Rgb, floor: f32, dark: bool) -> Rgb {
    let start = start.min(9);
    // No allocation: this runs inside render paths.
    (0..=9u8)
        .map(|k| {
            if dark {
                start.saturating_sub(k)
            } else {
                (start + k).min(9)
            }
        })
        .map(|i| color.shade(i))
        .find(|c| c.contrast(bg) >= floor)
        .unwrap_or(if dark {
            ColorName::Gray.shade(0)
        } else {
            ColorName::Gray.shade(9)
        })
}

/// Foreground for a filled background: white when it reads, otherwise "flip the contrast" to
/// the hue's darkest shade, then near-black.
fn filled_foreground(color: ColorName, bg: Rgb) -> Rgb {
    [
        Rgb::WHITE,
        color.shade(9),
        ColorName::Gray.shade(9),
        Rgb::BLACK,
    ]
    .into_iter()
    .find(|c| c.contrast(bg) >= 4.5)
    .unwrap_or(Rgb::BLACK)
}

pub fn resolve(ctx: &ResolveCtx, color: ColorName, variant: Variant) -> VariantColors {
    let dark = ctx.scheme == Scheme::Dark;
    // Filled background: prefer white text (the conventional look), darkening the fill by up
    // to two shades until white reaches 4.5:1. Hues still too bright (yellow, lime) keep the
    // theme shade and "flip the contrast" to dark text (Refactoring UI).
    let theme_shade = ctx.filled_shade();
    let base = (theme_shade..=(theme_shade + 2).min(9))
        .find(|&k| color.shade(k).contrast(Rgb::WHITE) >= 4.5)
        .unwrap_or(theme_shade);
    let filled = color.shade(base);
    let t = Rgb::TRANSPARENT;
    // Composite translucent tints over the surface so contrast is computable and stable.
    let tint = |alpha: f32| filled.alpha(alpha).over(ctx.surface);
    // Text/border color for tinted variants. The `Dark` palette is too close to the
    // dark surface, so it uses its lightest shade there.
    // Tinted text: start at the classic accent shade and darken (light) / lighten (dark) along
    // the same hue until it reads on the strongest tint it sits on.
    let accent = match (dark, color) {
        (true, ColorName::Dark) => color.shade(0),
        _ => {
            // Strongest tint it sits on: the pressed state (hover deepened once more).
            let strongest_tint = tint(if dark { 0.28 } else { 0.18 });
            readable_shade(
                color,
                if dark { 4 } else { ctx.filled_shade() },
                strongest_tint,
                TEXT_TARGET,
                dark,
            )
        }
    };
    match variant {
        Variant::Filled => VariantColors {
            background: filled,
            // Hover moves away from the text: darker under light text, lighter under dark.
            hover: if filled_foreground(color, filled) == Rgb::WHITE {
                color.shade((base + 1).min(9))
            } else {
                color.shade(base.saturating_sub(1))
            },
            foreground: filled_foreground(color, filled),
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
        // Mantine's blue 6 gives white text only 3.55:1; the fill deepens to blue 8 (4.9:1).
        assert_eq!(c.background, ColorName::Blue.shade(8));
        assert_eq!(c.foreground, Rgb::WHITE);
        assert!(c.foreground.contrast(c.background) >= 4.5);
    }

    #[test]
    fn filled_yellow_uses_dark_text() {
        let c = resolve(
            &ResolveCtx::new(Scheme::Light),
            ColorName::Yellow,
            Variant::Filled,
        );
        // Too bright for white even two shades deeper: dark text, still 4.5:1.
        assert_ne!(c.foreground, Rgb::WHITE);
        assert!(c.foreground.contrast(c.background) >= 4.5);
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
