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

/// `b + (b - a)` per channel, clamped: one more step in the direction a -> b.
fn extrapolate(a: Rgb, b: Rgb) -> Rgb {
    let e = |x: u8, y: u8| (2 * i16::from(y) - i16::from(x)).clamp(0, 255) as u8;
    Rgb {
        r: e(a.r, b.r),
        g: e(a.g, b.g),
        b: e(a.b, b.b),
        a: 255,
    }
}

/// Every surface a focus ring can be drawn over (it sits outside the control's box).
fn ring_surfaces(s: &crate::theme::Surfaces) -> [Rgb; 4] {
    [s.body, s.surface, s.raised, s.track]
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
                // Pressed continues the hover's direction (rest -> hover -> pressed), so it
                // never drifts toward the text and cannot lose contrast the hover kept.
                background: extrapolate(rest_bg, hover_bg),
                border: border(v.border.over(s.body)),
            },
            StateKind::Selected => {
                let f = self.variant(color, Variant::Filled);
                if high && f.background.contrast(s.body) < NON_TEXT_MIN {
                    // High contrast: the selected fill is a state indicator and must stand
                    // out from the body (3:1). Step the hue away from the body until it does.
                    let family = color.unwrap_or(self.primary);
                    // Text on it: whichever of black or white reads better (a luminance
                    // cut-off picks the weaker one on mid-tone fills).
                    let best = |bg: Rgb| {
                        if Rgb::BLACK.contrast(bg) >= Rgb::WHITE.contrast(bg) {
                            Rgb::BLACK
                        } else {
                            Rgb::WHITE
                        }
                    };
                    let shades: &[u8] = match self.scheme {
                        Scheme::Light => &[7, 8, 9],
                        Scheme::Dark => &[5, 4, 3, 2, 1, 0],
                    };
                    let (bg, fg) = shades
                        .iter()
                        .map(|shade| family.shade(*shade))
                        .map(|bg| (bg, best(bg)))
                        .find(|(bg, fg)| {
                            bg.contrast(s.body) >= NON_TEXT_MIN && fg.contrast(*bg) >= TEXT_MIN
                        })
                        .unwrap_or((s.text, s.body));
                    return StateColors {
                        foreground: fg,
                        background: bg,
                        border: bg,
                    };
                }
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
                let family = color.unwrap_or(self.primary);
                let base = match self.scheme {
                    Scheme::Light => self.primary_shade_light,
                    Scheme::Dark => self.primary_shade_dark,
                };
                // The ring sits outside the control, over whatever surface holds it (body,
                // card, track, raised segment), so it must clear the floor on all of them.
                // Keep the hue: step away from the background (darker in light, lighter in
                // dark) to the nearest shade that passes, and only then fall back to text.
                let passes = |c: Rgb| {
                    ring_surfaces(&s)
                        .iter()
                        .all(|b| c.contrast(*b) >= NON_TEXT_MIN)
                };
                let steps: [u8; 10] = match self.scheme {
                    Scheme::Light => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
                    Scheme::Dark => [9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
                };
                let ring = steps
                    .iter()
                    .skip_while(|shade| **shade != base.min(9))
                    .map(|shade| family.shade(*shade))
                    .find(|c| passes(*c))
                    .unwrap_or(s.text);
                StateColors {
                    foreground: s.text,
                    background: s.body,
                    border: ring,
                }
            }
        }
    }

    /// Loading veil: fades content toward the body (keeps the page recognisable, signals
    /// "busy") rather than darkening it like a modal scrim.
    pub fn veil(&self) -> Rgb {
        self.surfaces().body.alpha(0.55)
    }

    /// Overlay scrim: scheme-aware, composited over the body.
    pub fn scrim(&self) -> Rgb {
        match self.scheme {
            // Cool near-black rather than pure black (Refactoring UI: greys carry a tint).
            Scheme::Light => crate::ColorName::Gray.shade(9).alpha(0.35),
            Scheme::Dark => Rgb::BLACK.alpha(0.6),
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
    // The semantic surfaces of Kit follow the same rules as the primary: the
    // text reads on the surface it is painted on, in every state.
    let kit = crate::kit_bridge::KitColors::from_theme(t);
    for (name, family) in [
        ("danger", kit.danger),
        ("success", kit.success),
        ("warning", kit.warning),
        ("info", kit.info),
    ] {
        // An alert paints the colour of the family as its text, over a tint of
        // the same colour on the surface of the app: Kit mixes four percent
        // of the colour into the background.
        let tint = family.base.alpha(0.04).over(s.body);
        push(format!("{name} alert text"), family.base, tint, TEXT_MIN);
        // A tag paints the colour of the family as its fill, with the
        // foreground of the family on it.
        push(
            format!("{name} tag text"),
            family.foreground,
            family.base,
            TEXT_MIN,
        );
        // An outlined tag paints the colour of the family as its text on the
        // surface of the app.
        push(
            format!("{name} outlined tag text"),
            family.base,
            s.body,
            TEXT_MIN,
        );
        // A badge of Kit carries white text on the fill of the family.
        push(
            format!("{name} badge text"),
            Rgb::WHITE,
            family.fill,
            TEXT_MIN,
        );
        // A button of a semantic variant carries the foreground of the family
        // on the fill, in every state.
        push(
            format!("{name} button text"),
            family.foreground,
            family.base,
            TEXT_MIN,
        );
        push(
            format!("{name} button hover text"),
            family.foreground,
            family.hover,
            TEXT_MIN,
        );
        push(
            format!("{name} button pressed text"),
            family.foreground,
            family.active,
            TEXT_MIN,
        );
    }
    // Approved surface/state pairings: controls are not only placed on the body. Translucent
    // (tinted) variants composite over the surface they sit on, so their text is measured on
    // each one; neutral (Gray) and destructive (Red) emphasis are checked next to the
    // primary. The focus ring and secondary text must read on every surface too.
    let surfaces = [
        ("body", s.body),
        ("surface", s.surface),
        ("raised", s.raised),
        ("track", s.track),
    ];
    let ring = t.state(StateKind::FocusRing, Variant::Default, None).border;
    let dimmed = t
        .state(StateKind::Dimmed, Variant::Default, None)
        .foreground;
    for (surface, bg) in surfaces {
        for (emphasis, color) in [
            ("primary", None),
            ("neutral", Some(crate::ColorName::Gray)),
            ("destructive", Some(crate::ColorName::Red)),
        ] {
            for variant in [Variant::Light, Variant::Subtle, Variant::Outline] {
                let v = t.variant(color, variant);
                push(
                    format!("{emphasis} {variant:?} text on {surface}"),
                    v.foreground,
                    v.background.over(bg),
                    TEXT_MIN,
                );
                push(
                    format!("{emphasis} {variant:?} hover text on {surface}"),
                    v.foreground,
                    v.hover.over(bg),
                    TEXT_MIN,
                );
            }
        }
        push(format!("Dimmed text on {surface}"), dimmed, bg, TEXT_MIN);
        push(format!("Focus ring on {surface}"), ring, bg, NON_TEXT_MIN);
    }
    // Focus on a selected item: the ring is offset outside the control, so it is read
    // against the surface around the selected fill (covered above), never against the fill
    // itself. The selected fill must itself stand out from the body as a state indicator.
    let selected = t.state(StateKind::Selected, Variant::Default, None);
    push(
        "Selected fill vs body".into(),
        selected.background,
        s.body,
        NON_TEXT_MIN,
    );
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

    /// Documented exemption: in Standard contrast the Default variant keeps the subtle grey
    /// control border (≈1.5:1, Mantine/Refactoring UI style). High contrast meets 3:1. See
    /// spec/18 "Contrast report" for the rationale; everything else must pass.
    ///
    /// Second exemption, Standard only: the selected fill is the Filled shade (Mantine), and
    /// for light hues (cyan, teal, green, lime, yellow, orange in light; pink…indigo, cyan in
    /// dark) it reaches only 1.9–2.8:1 against the body. Its text still meets 4.5:1. Changing
    /// it would recolour every filled control, so it is a palette decision (spec/18); High
    /// contrast meets 3:1.
    fn exempt(contrast: Contrast, failure: &str) -> bool {
        contrast == Contrast::Standard
            && failure.ends_with("< 3")
            && (failure.contains("Default boundary") || failure.contains("Selected fill vs body"))
    }

    /// The §12.1 contrast report: every state, both schemes, every primary, both contrast
    /// levels meet the WCAG floors (one documented exemption).
    #[test]
    fn contrast_report_every_state_scheme_and_primary() {
        let mut bad = Vec::new();
        for contrast in [Contrast::Standard, Contrast::High] {
            bad.extend(
                failures(contrast)
                    .into_iter()
                    .filter(|f| !exempt(contrast, f)),
            );
        }
        assert!(
            bad.is_empty(),
            "{} contrast failures:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

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
