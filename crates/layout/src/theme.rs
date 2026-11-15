//! Theme lookups.

use gpui_kit::{App, Hsla};

/// The hairline color of the active theme.
///
/// A neutral gray stands in when no theme has been installed, so a widget that
/// draws a rule never asks an application to have a theme before it can be
/// shown.
pub(crate) fn hairline(cx: &App) -> Hsla {
    cx.try_global::<gpui_kit::component::Theme>()
        .map(|theme| theme.border)
        .unwrap_or_else(|| gpui_kit::hsla(0., 0., 0.5, 0.25))
}

/// The muted text color of the active theme.
pub(crate) fn muted_foreground(cx: &App) -> Hsla {
    cx.try_global::<gpui_kit::component::Theme>()
        .map(|theme| theme.muted_foreground)
        .unwrap_or_else(|| gpui_kit::hsla(0., 0., 0.45, 0.9))
}

