//! The width a widget resolves its responsive props against.

use gpui_kit::{App, AppContext as _, Entity, Pixels, Window, px};

use crate::responsive::Breakpoint;

/// The width a widget resolves its responsive props against.
///
/// The viewport is the width of the window. A fixed width is a width the
/// caller already knows. A measured width is the width of a box the caller
/// measures and keeps in a [`ContainerWidthState`].
///
/// A widget that measures its own container does not name a width here: it
/// measures the box it is laid out in, which is what a grid inside a pane
/// needs.
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
    /// The width of a box the caller measures.
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

    /// The width of a box the caller measures.
    pub fn measured(state: &Entity<ContainerWidthState>) -> Self {
        ContainerWidth::Measured(state.clone())
    }

    /// The width the widget resolves against.
    pub fn resolve(&self, window: &Window, cx: &App) -> Pixels {
        match self {
            ContainerWidth::Viewport => window.viewport_size().width,
            ContainerWidth::Fixed(width) => *width,
            ContainerWidth::Measured(state) => {
                measured_width(state, window.viewport_size().width, cx)
            }
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

/// The width a measurement holds, or the width of the window before the
/// measurement has happened.
///
/// A measurement only lands after the box has been painted, so the width of
/// the window is the starting point: a widget answers to something sensible
/// from the first frame, and the measurement takes over on the next one.
pub(crate) fn measured_width(
    state: &Entity<ContainerWidthState>,
    viewport: Pixels,
    cx: &App,
) -> Pixels {
    let state = state.read(cx);
    if state.is_measured() {
        px(state.width())
    } else {
        viewport
    }
}

/// The measured width of a box.
///
/// The measurement keeps the exact width it was given, so a widget that
/// needs the width in pixels has it, and it only says something when the
/// *band* the width falls in changes: a resize that stays inside one band
/// therefore costs nothing.
///
/// A measurement belongs to one widget. The first widget that measures owns
/// it, and a measurement from another widget is refused: two widgets of
/// different widths sharing one measurement would otherwise flip the band
/// every frame, and the view would render again and again.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContainerWidthState {
    width: f32,
    band: Breakpoint,
    owner: Option<u64>,
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

    /// Whether the box has been measured.
    pub fn is_measured(&self) -> bool {
        self.owner.is_some()
    }

    /// Takes a measurement, if the owner may take it.
    ///
    /// The exact width is always kept, and the band only changes when the
    /// width crosses into another one. The answer is whether the band
    /// changed.
    fn take(&mut self, width: f32, owner: u64) -> bool {
        if let Some(current) = self.owner {
            if current != owner {
                // Another widget measures this box: the measurement is
                // refused, so the band does not flip back and forth.
                return false;
            }
        }
        self.owner = Some(owner);
        self.width = width;
        let band = Breakpoint::from_width(width);
        if band == self.band {
            return false;
        }
        self.band = band;
        true
    }

    /// Remembers a width, and says something when the band changes.
    ///
    /// The answer is whether the band changed: the caller asks for a frame
    /// when it did.
    fn measure(&mut self, width: f32, owner: u64) -> bool {
        if let Some(current) = self.owner {
            debug_assert!(
                current == owner,
                "one measurement belongs to one widget: two widgets of different widths \
                 would flip the band every frame"
            );
        }
        self.take(width, owner)
    }
}

/// Measures the box a widget is laid out in.
///
/// The measurement is written into the state, and the state only says
/// something when the band changes.
pub(crate) fn measure(
    state: &Entity<ContainerWidthState>,
    owner: u64,
    bounds: gpui_kit::Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let width = bounds.size.width.as_f32();
    let changed = state.update(cx, |state, _| state.measure(width, owner));
    if changed {
        // A notify raised during a draw only marks the view as dirty; it
        // does not ask for a frame. The frame is asked for here, so the
        // widget answers to the new width on the next one.
        let state = state.clone();
        window.defer(cx, move |_, cx| {
            state.update(cx, |_, cx| cx.notify());
        });
    }
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
        assert!(!state.is_measured());
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

    #[test]
    fn a_measurement_that_has_not_happened_yet_is_the_width_of_the_window() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(ContainerWidthState::new);
        let width = cx.update(|cx| super::measured_width(&state, px(800.), cx));
        assert_eq!(width, px(800.), "the window's width is the starting point");
        cx.update(|cx| {
            state.update(cx, |state, _| {
                state.take(400., 1);
            });
        });
        let width = cx.update(|cx| super::measured_width(&state, px(800.), cx));
        assert_eq!(width, px(400.), "the measurement takes over");
    }

    #[test]
    fn a_measurement_keeps_the_exact_width_it_was_given() {
        let mut state = ContainerWidthState::default();
        assert!(!state.take(400., 1), "the band does not change");
        assert_eq!(state.width(), 400., "the exact width is kept");
        assert_eq!(state.band(), Breakpoint::Base);
        assert!(state.is_measured());
    }

    #[test]
    fn a_measurement_only_says_something_when_the_band_changes() {
        let mut state = ContainerWidthState::default();
        assert!(
            !state.take(400., 1),
            "the first measurement stays in the band it starts in"
        );
        assert!(
            !state.take(500., 1),
            "another width in the same band says nothing"
        );
        assert!(state.take(600., 1), "another band says something");
        assert_eq!(state.band(), Breakpoint::Xs);
        assert_eq!(state.width(), 600., "the exact width is kept");
    }

    #[test]
    fn a_measurement_belongs_to_one_widget() {
        let mut state = ContainerWidthState::default();
        assert!(state.take(600., 1), "the first widget owns the measurement");
        assert!(
            !state.take(1000., 2),
            "the measurement of another widget is refused"
        );
        assert_eq!(
            state.band(),
            Breakpoint::Xs,
            "the band does not flip to the other widget's width"
        );
        assert_eq!(state.width(), 600., "the owner's width is kept");
    }

    #[test]
    fn two_widgets_of_different_widths_cannot_flip_the_band_forever() {
        let mut state = ContainerWidthState::default();
        assert!(state.take(600., 1));
        for _ in 0..10 {
            assert!(
                !state.take(1000., 2),
                "the other widget never gets the band"
            );
            assert_eq!(state.band(), Breakpoint::Xs);
        }
    }

    #[test]
    #[should_panic(expected = "one measurement belongs to one widget")]
    fn a_measurement_from_another_widget_is_noticed() {
        let mut state = ContainerWidthState::default();
        state.measure(400., 1);
        state.measure(1000., 2);
    }
}
