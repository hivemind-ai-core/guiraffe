//! Theme lookups.
//!
//! Every colour, size and width a widget draws comes from here, so the
//! widgets themselves hold no constants of their own.

use gpui_kit::{App, Hsla, px};
use guiraffe_core::{GuiraffeTheme, Size};

/// The width of a hairline, in pixels.
///
/// The core theme does not name a border width yet, so the hairline keeps
/// its width here: one device pixel.
const HAIRLINE_WIDTH: f32 = 1.;

/// The hairline color of the active theme.
///
/// The core theme answers even when an application has not installed one, so
/// a widget that draws a rule can be shown before a theme exists.
pub(crate) fn hairline(cx: &App) -> Hsla {
    GuiraffeTheme::global(cx).surfaces().border.to_hsla()
}

/// The muted text color of the active theme.
pub(crate) fn muted_foreground(cx: &App) -> Hsla {
    GuiraffeTheme::global(cx).surfaces().dimmed.to_hsla()
}

/// The width of a hairline.
pub(crate) fn hairline_width() -> f32 {
    HAIRLINE_WIDTH
}

/// The size of the text at a size of the scale, in pixels.
///
/// The scale of the theme is applied, so the text follows the theme the
/// application installed.
pub(crate) fn font_size(cx: &App, size: Size) -> f32 {
    size.font_size() * GuiraffeTheme::global(cx).scale
}

/// The size of the text as a length.
pub(crate) fn font_size_px(cx: &App, size: Size) -> gpui_kit::Pixels {
    px(font_size(cx, size))
}

/// The narrowest a pane of a splitter may become, in pixels.
///
/// The core theme does not name a panel width yet, so the width comes from
/// the scale: two controls side by side.
pub(crate) fn panel_min_size() -> f32 {
    Size::Sm.control_height() * 2.
}

#[cfg(test)]
mod tests {
    use super::{font_size, hairline_width};
    use guiraffe_core::Size;

    #[test]
    fn a_hairline_is_one_pixel_wide() {
        assert_eq!(hairline_width(), 1.);
    }

    #[test]
    fn the_font_size_follows_the_scale_of_the_theme() {
        let cx = gpui_kit::TestAppContext::single();
        let size = cx.update(|cx| font_size(cx, Size::Xs));
        assert_eq!(size, Size::Xs.font_size());
    }
}
