//! The width a widget resolves its responsive props against.

use gpui_kit::{App, AppContext as _, Entity, Pixels, Window, px};

use crate::responsive::Breakpoint;

/// The width a widget measures itself against.
///
/// The viewport is the width of the window. A fixed width is a width the
/// caller already knows. A measured width is the width of the box the widget
/// is laid out in: the widget measures itself while it renders and keeps the
/// measurement in a [`ContainerWidthState`].
///
/// A measured width is what a widget inside a pane or a column needs: the
/// pane is narrower than the window, and the props should follow the pane.
///
/// ```
/// use guiraffe_layout::{ContainerWidth, ContainerWidthState};
///
/// # fn build(cx: &mut gpui_kit::App) {
/// let width = ContainerWidthState::new(cx);
/// let grid = guiraffe_layout::Grid::new().container_width(ContainerWidth::Measured(width));
/// # }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum ContainerWidth {
    /// The width of the window.
    Viewport,
    /// A width the caller already knows.
    Fixed(Pixels),
    /// The width of the box the widget is laid out in.
    Measured(Entity<ContainerWidthState>),
}

impl ContainerWidth {
    /// The width of the window.
    pub fn viewport() -> Self {
        ContainerWidth::Viewport
    }

    /// A width the caller already knows.
    pub fn fixed(width: impl Into<Pixels>) -> Self {
        ContainerWidth::Fixed(width.into())
    }

    /// The width of the box the widget is laid out in.
    pub fn measured(state: &Entity<ContainerWidthState>) -> Self {
        ContainerWidth::Measured(state.clone())
    }

    /// The width the widget resolves against.
    pub fn resolve(&self, window: &Window, cx: &App) -> Pixels {
        match self {
            ContainerWidth::Viewport => window.viewport_size().width,
            ContainerWidth::Fixed(width) => *width,
            ContainerWidth::Measured(state) => px(state.read(cx).width()),
        }
    }

    /// The entity the measurement is kept in, if the width is measured.
    pub fn measurement(&self) -> Option<&Entity<ContainerWidthState>> {
        match self {
            ContainerWidth::Measured(state) => Some(state),
            _ => None,
        }
    }
}

/// The measured width of the box a widget is laid out in.
///
/// The measurement is kept in a small entity: it holds a width and the band
/// the width falls in, and it only says something when the band changes. A
/// resize that stays inside one band therefore costs nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContainerWidthState {
    width: f32,
    band: Breakpoint,
}

impl ContainerWidthState {
    /// A measurement that has not happened yet.
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self::default())
    }

    /// The measured width, in pixels.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The band the measured width falls in.
    pub fn band(&self) -> Breakpoint {
        self.band
    }

    /// Remembers a width, and says something when the band changes.
    fn measure(&mut self, width: f32, cx: &mut gpui_kit::Context<Self>) {
        let band = Breakpoint::from_width(width);
        if band == self.band {
            return;
        }
        self.band = band;
        self.width = width;
        cx.notify();
    }
}

/// Measures the box the widget is laid out in.
///
/// The measurement is written into the state, and the state only says
/// something when the band changes.
pub(crate) fn measure(
    state: &Entity<ContainerWidthState>,
    bounds: gpui_kit::Bounds<Pixels>,
    cx: &mut App,
) {
    let width = bounds.size.width.as_f32();
    state.update(cx, |state, cx| state.measure(width, cx));
}

#[cfg(test)]
mod tests {
    use super::{ContainerWidth, ContainerWidthState};
    use crate::responsive::Breakpoint;
    use gpui_kit::px;

    #[test]
    fn a_measurement_starts_out_empty() {
        let state = ContainerWidthState::default();
        assert_eq!(state.width(), 0.);
        assert_eq!(state.band(), Breakpoint::Base);
    }

    #[test]
    fn a_fixed_width_is_the_width_it_is() {
        assert_eq!(
            ContainerWidth::Fixed(px(240.)),
            ContainerWidth::Fixed(px(240.))
        );
        assert_ne!(
            ContainerWidth::Fixed(px(240.)),
            ContainerWidth::Fixed(px(120.))
        );
    }

    #[test]
    fn the_viewport_is_not_a_measurement() {
        assert!(ContainerWidth::Viewport.measurement().is_none());
        assert!(ContainerWidth::Fixed(px(10.)).measurement().is_none());
    }
}
