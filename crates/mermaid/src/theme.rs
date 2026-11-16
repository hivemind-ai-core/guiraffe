//! Mapping GUIraffe theme tokens onto the Mermaid renderer's theme.
//!
//! `mermaid-rs-renderer` takes colors as CSS strings, so the mapping is pure:
//! GUIraffe `Rgb` tokens in, a themed `mermaid_rs_renderer::Theme` out. The
//! renderer's own `modern()`/`dark()` themes supply the parts GUIraffe has no
//! token for (pie palettes, git colors), so a diagram is never half-themed.

use guiraffe_core::{ColorName, Rgb, Scheme, Surfaces};
use mermaid_rs_renderer::Theme;

/// Base font size for diagram text, in points.
pub const DIAGRAM_FONT_SIZE: f32 = 14.0;

fn hex(color: Rgb) -> String {
    format!("#{:06x}", color.to_u32())
}

/// The renderer theme for a GUIraffe scheme and its semantic surfaces.
///
/// Node fills use the lightest surface step so diagrams sit on the page
/// rather than on their own background, and edges use the dimmed text color
/// so they read as structure, not emphasis.
pub fn theme_for(scheme: Scheme, surfaces: Surfaces, primary: ColorName) -> Theme {
    let mut theme = match scheme {
        Scheme::Light => Theme::modern(),
        Scheme::Dark => Theme::dark(),
    };
    let accent = primary.shade(match scheme {
        Scheme::Light => 6,
        Scheme::Dark => 4,
    });

    theme.font_size = DIAGRAM_FONT_SIZE;
    theme.background = hex(surfaces.body);
    theme.text_color = hex(surfaces.text);
    theme.line_color = hex(surfaces.dimmed);
    theme.primary_text_color = hex(surfaces.text);
    theme.primary_color = hex(surfaces.surface);
    theme.primary_border_color = hex(surfaces.border);
    theme.secondary_color = hex(surfaces.hover);
    theme.tertiary_color = hex(surfaces.body);
    theme.edge_label_background = hex(surfaces.body);
    theme.cluster_background = hex(surfaces.surface);
    theme.cluster_border = hex(surfaces.border);

    theme.sequence_actor_fill = hex(surfaces.surface);
    theme.sequence_actor_border = hex(surfaces.border);
    theme.sequence_actor_line = hex(surfaces.dimmed);
    theme.sequence_note_fill = hex(accent.mix(surfaces.surface, 0.85));
    theme.sequence_note_border = hex(accent);
    theme.sequence_activation_fill = hex(surfaces.hover);
    theme.sequence_activation_border = hex(surfaces.border);

    theme.pie_title_text_color = hex(surfaces.text);
    theme.pie_section_text_color = hex(surfaces.text);
    theme.pie_legend_text_color = hex(surfaces.dimmed);
    theme.pie_outer_stroke_color = hex(surfaces.border);
    theme
}

/// Background the raster is composited against, matching the theme's page.
pub fn background_for(scheme: Scheme, surfaces: Surfaces) -> Rgb {
    let _ = scheme;
    surfaces.body
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_core::GuiraffeTheme;

    fn surfaces(scheme: Scheme) -> Surfaces {
        GuiraffeTheme {
            scheme,
            ..Default::default()
        }
        .surfaces()
    }

    #[test]
    fn light_and_dark_pages_follow_the_scheme() {
        let light = theme_for(Scheme::Light, surfaces(Scheme::Light), ColorName::Blue);
        let dark = theme_for(Scheme::Dark, surfaces(Scheme::Dark), ColorName::Blue);
        assert_eq!(light.background, "#ffffff");
        assert_ne!(dark.background, light.background);
        assert_ne!(dark.text_color, light.text_color);
    }

    #[test]
    fn text_contrasts_with_the_page_in_both_schemes() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let s = surfaces(scheme);
            let theme = theme_for(scheme, s, ColorName::Blue);
            assert_eq!(theme.text_color, hex(s.text));
            assert!(s.text.contrast(s.body) > 7.0, "{scheme:?}");
        }
    }

    #[test]
    fn accent_reaches_sequence_notes() {
        let s = surfaces(Scheme::Light);
        let blue = theme_for(Scheme::Light, s, ColorName::Blue);
        let red = theme_for(Scheme::Light, s, ColorName::Red);
        assert_ne!(blue.sequence_note_border, red.sequence_note_border);
        assert_eq!(blue.sequence_note_border, hex(ColorName::Blue.shade(6)));
    }

    #[test]
    fn every_mapped_field_is_a_css_hex_color() {
        let theme = theme_for(Scheme::Dark, surfaces(Scheme::Dark), ColorName::Teal);
        for value in [
            &theme.background,
            &theme.text_color,
            &theme.line_color,
            &theme.primary_color,
            &theme.primary_border_color,
            &theme.cluster_background,
            &theme.cluster_border,
            &theme.sequence_note_fill,
            &theme.pie_title_text_color,
        ] {
            assert!(
                value.len() == 7
                    && value.starts_with('#')
                    && value[1..].chars().all(|c| c.is_ascii_hexdigit()),
                "{value}"
            );
        }
        assert_eq!(theme.font_size, DIAGRAM_FONT_SIZE);
    }

    #[test]
    fn background_for_matches_the_theme_page() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let s = surfaces(scheme);
            assert_eq!(background_for(scheme, s), s.body);
        }
    }
}
