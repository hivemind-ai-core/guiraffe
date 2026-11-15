//! A grid of equally sized cells.

use crate::gpui::IntoElement;

use gpui_kit::{
    AnyElement, App, ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled as _, Window,
    div, px,
};

use crate::tokens::Size;
use crate::width::ContainerWidth;

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
/// grid is laid out in.
///
/// ```
/// use guiraffe_layout::{SimpleGrid, Size};
///
/// let grid = SimpleGrid::new()
///     .cols(3)
///     .spacing(Size::Sm)
///     .child("one")
///     .child("two")
///     .child("three");
/// ```
#[derive(IntoElement)]
pub struct SimpleGrid {
    cols: u16,
    spacing: f32,
    min_child_width: Option<Pixels>,
    container_width: ContainerWidth,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for SimpleGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleGrid {
    /// A grid with two columns and the default spacing.
    pub fn new() -> Self {
        Self {
            cols: 2,
            spacing: Size::Md.spacing(),
            min_child_width: None,
            container_width: ContainerWidth::Viewport,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// How many columns the grid has.
    pub fn cols(mut self, cols: u16) -> Self {
        self.cols = cols.max(1);
        self
    }

    /// The space between the cells.
    pub fn spacing(mut self, spacing: Size) -> Self {
        self.spacing = spacing.spacing();
        self
    }

    /// The space between the cells, in pixels.
    pub fn spacing_px(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// The narrowest a cell may be; the column count follows the width.
    pub fn min_child_width(mut self, min_child_width: impl Into<Pixels>) -> Self {
        self.min_child_width = Some(min_child_width.into());
        self
    }

    /// The width the grid resolves its column count against.
    pub fn container_width(mut self, container_width: ContainerWidth) -> Self {
        self.container_width = container_width;
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
}

impl gpui_kit::Styled for SimpleGrid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SimpleGrid {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl gpui_kit::IntoElement {
        let width = self.container_width.resolve(window);
        let cols = match self.min_child_width {
            Some(min_child_width) => {
                columns_for_min_width(width.as_f32(), min_child_width.as_f32(), self.spacing)
            }
            None => self.cols,
        };
        let mut element = div().grid().grid_cols(cols).gap(px(self.spacing));
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
    use crate::tokens::Size;

    #[test]
    fn a_new_grid_has_two_columns_and_the_default_spacing() {
        let grid = SimpleGrid::new();
        assert_eq!(grid.cols, 2);
        assert_eq!(grid.spacing, Size::Md.spacing());
        assert_eq!(grid.min_child_width, None);
    }

    #[test]
    fn a_grid_always_has_at_least_one_column() {
        assert_eq!(SimpleGrid::new().cols(0).cols, 1);
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
