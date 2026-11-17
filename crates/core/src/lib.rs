//! GUIraffe core: theme tokens (Mantine-style palette, sizes, variants), color math,
//! and the bridge that keeps GPUI Controls' theme in sync.

pub mod color;
pub mod context;
pub mod palette;
pub mod theme;
pub mod tokens;
pub mod variant;

pub mod kit_bridge;
pub mod state;
pub use color::Rgb;
pub use context::{Density, Emphasis, Provide, StyleContext, StylePatch, SurfaceKind, provide};
pub use kit_bridge::{KitColors, kit_size};
pub use palette::{ColorName, Shade};
pub use state::{Contrast, StateColors, StateKind, contrast_checks};
pub use theme::{GuiraffeTheme, Recipes, Scheme, Surfaces, apply, set_scheme};
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
