//! Semantic interaction-state tokens (spec/00 §12.1).
//!
//! Widgets ask the theme for the colors of a state (`theme.state(kind, variant, color)`) and
//! never apply raw opacity or fixed shades for hover, pressed, selected, disabled, dimmed or
//! focus. Everything resolves from scheme, primary color and [`Contrast`], so contrast floors
//! can be tested for every combination.

use crate::{
    color::Rgb,
    palette::ColorName,
    theme::{GuiraffeTheme, Scheme},
    variant::Variant,
};

/// Theme contrast setting. `High` also follows an OS high-contrast preference when the
/// platform exposes one (gpui-kit 0.9.1 does not; see spec/18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Contrast {
    #[default]
    Standard,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateKind {
    Rest,
    Hover,
    Pressed,
    Selected,
    Disabled,
    /// Secondary text on the body background.
    Dimmed,
    /// Focus-visible indicator; `border` is the ring color.
    FocusRing,
}

impl StateKind {
    pub const ALL: [StateKind; 7] = [
        StateKind::Rest,
        StateKind::Hover,
        StateKind::Pressed,
        StateKind::Selected,
        StateKind::Disabled,
        StateKind::Dimmed,
        StateKind::FocusRing,
    ];
}

/// Resolved colors of one state. `background` is what the control paints (already composited
/// over the body, so contrast can be measured directly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateColors {
    pub foreground: Rgb,
    pub background: Rgb,
    pub border: Rgb,
}

/// WCAG floors from §12.1.
pub const TEXT_MIN: f32 = 4.5;
pub const NON_TEXT_MIN: f32 = 3.0;
pub const DISABLED_MIN: f32 = 3.0;

impl GuiraffeTheme {
    /// Colors for `kind` on a control of `variant` in `color` (primary when `None`).
    pub fn state(
        &self,
        kind: StateKind,
        variant: Variant,
        color: Option<ColorName>,
    ) -> StateColors {
        let s = self.surfaces();
        let high = self.contrast == Contrast::High;
        let v = self.variant(color, variant);
        // A transparent-ish variant background reads as the body.
        let rest_bg = v.background.over(s.body);
        let hover_bg = v.hover.over(s.body);
        let boundary = if high { s.dimmed } else { s.border };
        let border = |b: Rgb| {
            if b == v.background {
                b
            } else if high {
                s.dimmed
            } else {
                b
            }
        };
        match kind {
            StateKind::Rest => StateColors {
                foreground: v.foreground,
                background: rest_bg,
                border: border(v.border.over(s.body)),
            },
            StateKind::Hover => StateColors {
                foreground: v.foreground,
                background: hover_bg,
                border: border(v.border.over(s.body)),
            },
            StateKind::Pressed => StateColors {
                foreground: v.foreground,
                // Pressed deepens the hover toward the foreground, per scheme.
                background: v.foreground.alpha(0.12).over(hover_bg),
                border: border(v.border.over(s.body)),
            },
            StateKind::Selected => {
                let f = self.variant(color, Variant::Filled);
                StateColors {
                    foreground: f.foreground,
                    background: f.background,
                    border: f.background,
                }
            }
            StateKind::Disabled => StateColors {
                foreground: if high { s.text } else { s.dimmed },
                background: s.track,
                border: boundary,
            },
            StateKind::Dimmed => StateColors {
                foreground: if high { s.text } else { s.dimmed },
                background: s.body,
                border: boundary,
            },
            StateKind::FocusRing => {
                let ring = color.map_or_else(
                    || self.primary_color(),
                    |c| {
                        c.shade(match self.scheme {
                            Scheme::Light => self.primary_shade_light,
                            Scheme::Dark => self.primary_shade_dark,
                        })
                    },
                );
                // Never let the ring vanish into the body.
                let ring = if ring.contrast(s.body) >= NON_TEXT_MIN {
                    ring
                } else {
                    s.text
                };
                StateColors {
                    foreground: s.text,
                    background: s.body,
                    border: ring,
                }
            }
        }
    }

    /// Overlay scrim: scheme-aware, composited over the body.
    pub fn scrim(&self) -> Rgb {
        match self.scheme {
            Scheme::Light => Rgb::BLACK.alpha(0.45),
            Scheme::Dark => Rgb::BLACK.alpha(0.65),
        }
    }
}

/// One contrast check: label, measured ratio, required floor.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastCheck {
    pub label: String,
    pub ratio: f32,
    pub floor: f32,
}

impl ContrastCheck {
    pub fn passes(&self) -> bool {
        self.ratio >= self.floor
    }
}

/// Every §12.1 floor for one theme: text in each state, disabled legibility, dimmed text,
/// focus ring and control boundaries (Default and Outline variants) against the body.
pub fn contrast_checks(t: &GuiraffeTheme) -> Vec<ContrastCheck> {
    let s = t.surfaces();
    let mut out = Vec::new();
    let mut push = |label: String, a: Rgb, b: Rgb, floor: f32| {
        out.push(ContrastCheck {
            label,
            ratio: a.contrast(b),
            floor,
        })
    };
    for variant in [
        Variant::Filled,
        Variant::Light,
        Variant::Outline,
        Variant::Subtle,
        Variant::Default,
    ] {
        for kind in [
            StateKind::Rest,
            StateKind::Hover,
            StateKind::Pressed,
            StateKind::Selected,
        ] {
            let c = t.state(kind, variant, None);
            push(
                format!("{variant:?}/{kind:?} text"),
                c.foreground,
                c.background,
                TEXT_MIN,
            );
        }
        let d = t.state(StateKind::Disabled, variant, None);
        push(
            format!("{variant:?}/Disabled text"),
            d.foreground,
            d.background,
            DISABLED_MIN,
        );
    }
    for variant in [Variant::Outline, Variant::Default] {
        let c = t.state(StateKind::Rest, variant, None);
        push(
            format!("{variant:?} boundary"),
            c.border,
            s.body,
            NON_TEXT_MIN,
        );
    }
    let d = t.state(StateKind::Dimmed, Variant::Default, None);
    push("Dimmed text".into(), d.foreground, d.background, TEXT_MIN);
    let f = t.state(StateKind::FocusRing, Variant::Default, None);
    push("Focus ring".into(), f.border, s.body, NON_TEXT_MIN);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARIES: [ColorName; 12] = [
        ColorName::Red,
        ColorName::Pink,
        ColorName::Grape,
        ColorName::Violet,
        ColorName::Indigo,
        ColorName::Blue,
        ColorName::Cyan,
        ColorName::Teal,
        ColorName::Green,
        ColorName::Lime,
        ColorName::Yellow,
        ColorName::Orange,
    ];

    fn failures(contrast: Contrast) -> Vec<String> {
        let mut out = Vec::new();
        for scheme in [Scheme::Light, Scheme::Dark] {
            for primary in PRIMARIES {
                let t = GuiraffeTheme {
                    scheme,
                    primary,
                    contrast,
                    ..Default::default()
                };
                for c in contrast_checks(&t).into_iter().filter(|c| !c.passes()) {
                    out.push(format!(
                        "{scheme:?}/{primary:?}/{contrast:?}: {} {:.2} < {}",
                        c.label, c.ratio, c.floor
                    ));
                }
            }
        }
        out
    }

    /// The §12.1 contrast report. Prints every failure. The current palette does not meet
    /// every floor yet (see spec/18 "Contrast report"), so instead of tuning colors silently
    /// this pins the failure count: fixing a failure or adding one both fail here, and the
    /// numbers must be updated deliberately.
    #[test]
    fn contrast_report_every_state_scheme_and_primary() {
        let std = failures(Contrast::Standard);
        let high = failures(Contrast::High);
        for f in std.iter().chain(&high) {
            println!("{f}");
        }
        assert_eq!(
            (std.len(), high.len()),
            KNOWN_FAILURES,
            "contrast failures changed; update spec/18 and KNOWN_FAILURES"
        );
    }

    /// (standard, high) failure counts at the time of the audit.
    const KNOWN_FAILURES: (usize, usize) = (302, 188);

    #[test]
    fn dimmed_and_focus_never_regress_in_high_contrast() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let t = GuiraffeTheme {
                scheme,
                contrast: Contrast::High,
                ..Default::default()
            };
            for c in contrast_checks(&t) {
                if c.label == "Dimmed text" || c.label == "Focus ring" {
                    assert!(c.passes(), "{scheme:?} {} {:.2}", c.label, c.ratio);
                }
            }
        }
    }

    #[test]
    fn scrim_is_scheme_aware() {
        let l = GuiraffeTheme::default();
        let d = GuiraffeTheme {
            scheme: Scheme::Dark,
            ..Default::default()
        };
        assert_ne!(l.scrim(), d.scrim());
    }
}
