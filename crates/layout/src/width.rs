//! Where a widget reads the width it lays out against.

use gpui_kit::{Pixels, Window};

/// The width a widget measures itself against when it resolves responsive props.
///
/// GPUI 0.7 has no per-element measurement callback, so a widget that cannot
/// be told its own width reads the window instead. Callers that know the
/// width — a container with a fixed size, a panel in a split view — can hand
/// it over with [`ContainerWidth::Fixed`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ContainerWidth {
    /// The window's viewport width.
    #[default]
    Viewport,
    /// A width the caller already knows.
    Fixed(Pixels),
}

impl ContainerWidth {
    /// The width this setting stands for.
    pub fn resolve(self, window: &Window) -> Pixels {
        match self {
            ContainerWidth::Viewport => window.viewport_size().width,
            ContainerWidth::Fixed(width) => width,
        }
    }

    /// A fixed width, in pixels.
    pub fn fixed(width: impl Into<Pixels>) -> Self {
        ContainerWidth::Fixed(width.into())
    }
}
