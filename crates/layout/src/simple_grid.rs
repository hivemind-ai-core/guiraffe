//! A grid of equally sized cells.

use crate::gpui::IntoElement;
use gpui_kit::base::ElementExt as _;

use gpui_kit::{
    AnyElement, App, ElementId, ParentElement as _, Pixels, RenderOnce, StyleRefinement,
    Styled as _, Window, div, px,
};

use crate::responsive::Responsive;
use crate::tokens::Size;
use crate::width::{ContainerWidth, ContainerWidthState, measure, measured_width};

impl From<Size> for Responsive<f32> {
    fn from(size: Size) -> Self {
        Responsive::value(size.spacing())
    }
}

/// How many cells fit across a width when every cell has a minimum width.
///
/// The spacing is counted once per cell and once between the cells, the way a
/// row of columns measures itself.
pub fn columns_for_min_width(width: f32, min_child_width: f32, spacing: f32) -> u16 {
    if min_child_width <= 0. || width <= 0. {
        return 1;
    }
    let spacing = spacing.max(0.);
    let cell = min_child_width + spacing;
    let count = ((width + spacing) / cell).floor();
    count.max(1.).min(f32::from(u16::MAX)) as u16
}

/// A grid whose cells are all the same size.
///
/// A simple grid has a fixed number of columns, or a minimum width for its
/// cells: with a minimum width the number of columns follows the width the
/// grid is laid out in. The columns and the spacing can change with the width
/// as well.
///
/// The grid follows the box it is laid out in: it measures that box and
/// resolves its columns against it, so a grid inside a narrow pane answers
/// to the pane and not to the window. Name a width of your own with
/// [`container_width`](SimpleGrid::container_width) to resolve against
/// something else.
///
/// ```
/// use guiraffe_layout::{Breakpoint, Responsive, SimpleGrid, Size};
///
/// let grid = SimpleGrid::new()
///     .cols(Responsive::base(2).at(Breakpoint::Md, 3))
///     .spacing(Size::Sm)
///     .child("one")
///     .child("two")
///     .child("three");
/// ```
#[derive(IntoElement)]
pub struct SimpleGrid {
    cols: Responsive<u16>,
    spacing: Responsive<f32>,
    vertical_spacing: Option<Responsive<f32>>,
    min_child_width: Option<Pixels>,
    /// The width the grid resolves against, or nothing when the grid
    /// measures the box it is laid out in.
    container_width: Option<ContainerWidth>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
    /// The key the measurement of the grid is kept under.
    ///
    /// The measurement is keyed by the place the grid was built: two grids
    /// built in the same place share their measurement, because they are in
    /// the same container and have the same width.
    pub(crate) key: ElementId,
}

impl Default for SimpleGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleGrid {
    /// A grid with two columns and the default spacing.
    #[track_caller]
    pub fn new() -> Self {
        let location = std::panic::Location::caller();
        Self {
            cols: Responsive::value(2),
            spacing: Responsive::value(Size::Md.spacing()),
            vertical_spacing: None,
            min_child_width: None,
            container_width: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
            key: ElementId::CodeLocation(*location),
        }
    }

    /// How many columns the grid has.
    pub fn cols(mut self, cols: impl Into<Responsive<u16>>) -> Self {
        self.cols = cols.into();
        self
    }

    /// The space between the cells.
    pub fn spacing(mut self, spacing: impl Into<Responsive<f32>>) -> Self {
        self.spacing = spacing.into().map(crate::style::gap_value);
        self
    }

    /// The space between the rows of cells.
    pub fn vertical_spacing(mut self, spacing: impl Into<Responsive<f32>>) -> Self {
        self.vertical_spacing = Some(spacing.into().map(crate::style::gap_value));
        self
    }

    /// The narrowest a cell may be; the column count follows the width.
    pub fn min_child_width(mut self, min_child_width: impl Into<Pixels>) -> Self {
        self.min_child_width = Some(min_child_width.into());
        self
    }

    /// The width the grid resolves its column count against.
    ///
    /// The grid measures the box it is laid out in by default. Name a width
    /// of your own to resolve against something else: the width of the
    /// window, a width you know, or a box you measure yourself.
    pub fn container_width(mut self, container_width: ContainerWidth) -> Self {
        self.container_width = Some(container_width);
        self
    }

    /// Adds a cell.
    pub fn child(mut self, child: impl gpui_kit::IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Adds several cells.
    pub fn children(
        mut self,
        children: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
    ) -> Self {
        self.children
            .extend(children.into_iter().map(|child| child.into_any_element()));
        self
    }

    /// How many columns the grid has at a width.
    pub fn cols_at(&self, width: f32) -> u16 {
        self.cols.resolve(width).max(1)
    }

    /// The space between the cells at a width.
    pub fn spacing_at(&self, width: f32) -> f32 {
        self.spacing.resolve(width).max(0.)
    }

    /// The space between the rows of cells at a width.
    pub fn vertical_spacing_at(&self, width: f32) -> Option<f32> {
        self.vertical_spacing
            .as_ref()
            .map(|spacing| spacing.resolve(width).max(0.))
    }
}

impl gpui_kit::Styled for SimpleGrid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SimpleGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        // The grid keeps the measurement of its own container in a state of
        // its own, so it can answer to the box it is laid out in.
        let own =
            window.use_keyed_state(self.key.clone(), cx, |_, _| ContainerWidthState::default());
        let owner = own.entity_id().as_u64();

        let (width, measurement) = match &self.container_width {
            Some(ContainerWidth::Measured(state)) => (
                measured_width(state, window.viewport_size().width, cx),
                Some(state.clone()),
            ),
            Some(width) => (width.resolve(window, cx), None),
            None => (
                measured_width(&own, window.viewport_size().width, cx),
                Some(own.clone()),
            ),
        };
        let width = width.as_f32();
        let spacing = self.spacing_at(width);
        let cols = match self.min_child_width {
            Some(min_child_width) => {
                columns_for_min_width(width, min_child_width.as_f32(), spacing)
            }
            None => self.cols_at(width),
        };
        let mut element = div().grid().grid_cols(cols).gap(px(spacing));
        if let Some(vertical_spacing) = self.vertical_spacing_at(width) {
            element = element.gap_y(px(vertical_spacing));
        }
        if let Some(state) = measurement {
            let state = state.clone();
            element = element.w_full().on_prepaint(move |bounds, window, cx| {
                measure(&state, owner, bounds, window, cx);
            });
        }
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for child in self.children {
            element = element.child(child);
        }
        element
    }
}

#[cfg(test)]
mod tests {
    use super::{SimpleGrid, columns_for_min_width};
    use crate::responsive::{Breakpoint, Responsive};
    use crate::tokens::Size;

    #[test]
    fn a_new_grid_has_two_columns_and_the_default_spacing() {
        let grid = SimpleGrid::new();
        assert_eq!(grid.cols_at(1200.), 2);
        assert_eq!(grid.spacing_at(1200.), Size::Md.spacing());
        assert_eq!(grid.min_child_width, None);
    }

    #[test]
    fn a_grid_always_has_at_least_one_column() {
        assert_eq!(SimpleGrid::new().cols(0).cols_at(1200.), 1);
    }

    #[test]
    fn the_columns_can_change_with_the_width() {
        let grid = SimpleGrid::new().cols(Responsive::base(2).at(Breakpoint::Md, 4));
        assert_eq!(grid.cols_at(500.), 2);
        assert_eq!(grid.cols_at(1000.), 4);
    }

    #[test]
    fn the_spacing_can_change_with_the_width() {
        let grid = SimpleGrid::new().spacing(Responsive::base(4.).at(Breakpoint::Md, 12.));
        assert_eq!(grid.spacing_at(500.), 4.);
        assert_eq!(grid.spacing_at(1000.), 12.);
    }

    #[test]
    fn the_vertical_spacing_can_change_with_the_width() {
        let grid = SimpleGrid::new().vertical_spacing(Responsive::base(4.).at(Breakpoint::Md, 12.));
        assert_eq!(grid.vertical_spacing_at(500.), Some(4.));
        assert_eq!(grid.vertical_spacing_at(1000.), Some(12.));
        assert_eq!(SimpleGrid::new().vertical_spacing_at(500.), None);
    }

    #[test]
    fn a_minimum_width_decides_the_column_count() {
        assert_eq!(columns_for_min_width(1000., 200., 0.), 5);
        assert_eq!(columns_for_min_width(1000., 200., 20.), 4);
        assert_eq!(columns_for_min_width(250., 200., 0.), 1);
        assert_eq!(columns_for_min_width(150., 200., 0.), 1);
    }

    #[test]
    fn a_minimum_width_never_produces_no_columns() {
        assert_eq!(columns_for_min_width(0., 200., 16.), 1);
        assert_eq!(columns_for_min_width(1000., 0., 16.), 1);
        assert_eq!(columns_for_min_width(-100., 200., 16.), 1);
    }

    #[test]
    fn a_minimum_width_that_fits_once_still_gets_one_column() {
        assert_eq!(columns_for_min_width(199., 200., 0.), 1);
        assert_eq!(columns_for_min_width(200., 200., 0.), 1);
    }
}
