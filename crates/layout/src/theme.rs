//! Theme lookups.

use gpui_kit::{App, Hsla};
use guiraffe_core::GuiraffeTheme;

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
