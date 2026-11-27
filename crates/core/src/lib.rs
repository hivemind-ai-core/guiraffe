//! GUIraffe core: theme tokens (Mantine-style palette, sizes, variants), color math,
//! and the bridge that keeps GPUI Controls' theme in sync.
//!
//! Apply a style context to a Rust `ui!` subtree with `provide`.
//!
//! ```ignore
//! ui! {
//!     provide(StylePatch::new().density(Density::Compact),
//!         ui! { div { "Compact content" } })
//! }
//! ```

pub mod color;
pub mod context;
pub mod control;
pub mod focus_policy;
pub mod menu_section;
pub mod palette;
pub mod text_menu;
pub mod theme;
pub mod tokens;
pub mod variant;

pub mod kit_bridge;
pub mod state;
pub use color::Rgb;
pub use context::{
    Density, Emphasis, Provide, StyleContext, StylePatch, SurfaceKind, focus_scope, provide,
    tab_stop,
};
pub use control::{
    A11yDisabled, A11yDisabledExt, Press, PressColors, PressableExt, RING_OFFSET, RING_WIDTH,
    focus_ring, keyed_focus, release_focus_within, ring_visible,
};
pub use focus_policy::{
    FocusIndicator, FocusOpt, FocusPolicy, ResolvedFocus, app_focus_policy, set_app_focus_policy,
};
/// The drag axis lock, shared with GPUI-free crates (see `guiraffe-geometry`).
pub use guiraffe_geometry::{AxisLock, apply as apply_axis_lock};
pub use kit_bridge::{KitColors, kit_size};
pub use menu_section::{
    MenuSectionAlign, MenuSectionId, MenuSectionStyle, RegisterMenuSection, set_menu_section_styles,
};
pub use palette::{ColorName, Shade};
pub use state::{Contrast, DragPreviewColors, StateColors, StateKind, contrast_checks};
pub use text_menu::{TextMenu, TextMenuItem, TextMenuSpec};
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
pub mod keys;

pub mod protocol;
pub mod widget_catalog;
pub mod widget_spec;
pub mod widget_value;
