//! GUIraffe core: theme tokens (Mantine-style palette, sizes, variants), color math,
//! and the bridge that keeps GPUI Controls' theme in sync.

pub mod color;
pub mod palette;
pub mod theme;
pub mod tokens;
pub mod variant;

pub use color::Rgb;
pub use palette::{ColorName, Shade};
pub use theme::{GuiraffeTheme, Scheme, Surfaces, apply, set_scheme};
pub use tokens::{ShadowLayer, Size, shadow};
pub use variant::{Variant, VariantColors};

/// Initialize Controls (if not done) and install the default GUIraffe theme.
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<gpui_kit::component::theme::Theme>() {
        gpui_kit::component::init(cx);
    }
    apply(GuiraffeTheme::default(), cx);
}

#[cfg(test)]
mod gpui_tests;
