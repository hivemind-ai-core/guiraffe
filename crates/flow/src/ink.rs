//! Theme palette colours for edges, ports, cables and the grid (#160).
//!
//! Flow data names a colour by its palette name (`"teal"`, see
//! [`ColorName::name`](guiraffe_core::ColorName::name)). Each is shaded for the scheme so it
//! keeps 3:1 against the canvas (WCAG 1.4.11 non-text contrast), so typed ports and edges stay
//! distinct from one another and from the background in both schemes.

use guiraffe_core::{ColorName, GuiraffeTheme, Rgb, Scheme, variant::readable_hue};
use guiraffe_flow_core::Hue;

/// The palette colours, indexed as [`ColorName::ALL`], resolved for `theme`.
pub(crate) type Palette = [Rgb; ColorName::ALL.len()];

/// Contrast a coloured stroke keeps against the canvas.
const NON_TEXT: f32 = 3.0;

pub(crate) fn palette(theme: &GuiraffeTheme) -> Palette {
    let surfaces = theme.surfaces();
    let dark = theme.scheme == Scheme::Dark;
    let start = if dark { 4 } else { 6 };
    ColorName::ALL.map(|color| readable_hue(color, start, surfaces.body, NON_TEXT, dark))
}

/// The palette slot of a hue: `Hue` and `ColorName` list the palette in the same order.
pub(crate) fn slot(hue: Hue) -> usize {
    hue as usize
}

/// The colour of `hue` in `palette`.
pub(crate) fn resolve(palette: &Palette, hue: Option<Hue>) -> Option<Rgb> {
    hue.map(|hue| palette[slot(hue)])
}

/// The flow hue of a theme colour.
pub(crate) fn hue(color: ColorName) -> Hue {
    Hue::ALL[ColorName::ALL
        .iter()
        .position(|c| *c == color)
        .unwrap_or_default()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hues_are_the_theme_palette_in_its_order() {
        assert_eq!(Hue::ALL.len(), ColorName::ALL.len());
        for (index, (hue, color)) in Hue::ALL.iter().zip(ColorName::ALL).enumerate() {
            assert_eq!(hue.name(), color.name());
            assert_eq!(slot(*hue), index);
            assert_eq!(super::hue(color), *hue);
        }
        let palette = palette(&GuiraffeTheme::default());
        assert_eq!(resolve(&palette, None), None);
        assert_eq!(Hue::from_name("Teal"), Some(Hue::Teal));
    }

    #[test]
    fn every_colour_reads_against_the_canvas_in_both_schemes() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let theme = GuiraffeTheme {
                scheme,
                ..Default::default()
            };
            let body = theme.surfaces().body;
            for (color, ink) in ColorName::ALL.iter().zip(palette(&theme)) {
                assert!(
                    ink.contrast(body) >= NON_TEXT - 0.01,
                    "{color:?} in {scheme:?}"
                );
            }
        }
    }
}
