//! A twelve-column grid and the columns that fill it.

use gpui_kit::{
    AnyElement, App, ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled as _, Window,
    div, px,
};

use crate::align::Justify;
use crate::macros::impl_into_element;
use crate::responsive::Responsive;
use crate::style::justify_content;
use crate::tokens::Size;
use crate::width::ContainerWidth;

/// The number of columns a grid has unless it is told otherwise.
pub const DEFAULT_COLUMNS: u16 = 12;

/// A grid of columns.
///
/// A grid has twelve columns by default. Each [`Col`] takes a span of those
/// columns and may leave an offset before it. Both can change with the width
/// the grid is laid out in.
///
/// ```
/// use guiraffe_layout::{Col, Grid, Size};
///
/// let grid = Grid::new()
///     .gutter(Size::Sm)
///     .child(Col::span(6).child("left"))
///     .child(Col::span(6).child("right"));
/// ```
pub struct Grid {
    columns: u16,
    gutter: f32,
    justify: Justify,
    grow: bool,
    container_width: ContainerWidth,
    children: Vec<Col>,
    style: StyleRefinement,
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Grid {
    /// A grid with twelve columns and the default gutter.
    pub fn new() -> Self {
        Self {
            columns: DEFAULT_COLUMNS,
            gutter: Size::Md.spacing(),
            justify: Justify::Start,
            grow: false,
            container_width: ContainerWidth::Viewport,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// How many columns the grid has.
    pub fn columns(mut self, columns: u16) -> Self {
        self.columns = columns.max(1);
        self
    }

    /// The space between columns and rows.
    pub fn gutter(mut self, gutter: Size) -> Self {
        self.gutter = gutter.spacing();
        self
    }

    /// The space between columns and rows, in pixels.
    pub fn gutter_px(mut self, gutter: f32) -> Self {
        self.gutter = gutter;
        self
    }

    /// Where the columns sit along the row.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    /// Whether the grid grows to fill the space its parent offers.
    pub fn grow(mut self, grow: bool) -> Self {
        self.grow = grow;
        self
    }

    /// The width the grid resolves its responsive columns against.
    pub fn container_width(mut self, container_width: ContainerWidth) -> Self {
        self.container_width = container_width;
        self
    }

    /// Adds a column.
    pub fn child(mut self, child: impl Into<Col>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Adds several columns.
    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Col>>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }
}

impl gpui_kit::Styled for Grid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Grid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        let width = self.container_width.resolve(window);
        let mut element = div().grid().grid_cols(self.columns).gap(px(self.gutter));
        element = justify_content(element, self.justify);
        if self.grow {
            element = element.flex_grow_1();
        }
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for column in self.children {
            element = element.child(column.render_at(width, window, cx));
        }
        element
    }
}

impl_into_element!(Grid);

/// One column of a [`Grid`].
///
/// A column spans a number of the grid's columns and may start after an
/// offset. Both numbers can be different per breakpoint.
///
/// ```
/// use guiraffe_layout::{Breakpoint, Col, Responsive};
///
/// let column = Col::span(Responsive::base(12).at(Breakpoint::Md, 6));
/// ```
pub struct Col {
    span: Responsive<u16>,
    offset: Responsive<u16>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for Col {
    fn default() -> Self {
        Self::new()
    }
}

impl Col {
    /// A column that spans the whole grid.
    pub fn new() -> Self {
        Self {
            span: Responsive::value(DEFAULT_COLUMNS),
            offset: Responsive::value(0),
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// A column that takes `span` of the grid's columns.
    ///
    /// ```
    /// use guiraffe_layout::Col;
    ///
    /// let column = Col::span(6);
    /// ```
    pub fn span(span: impl Into<Responsive<u16>>) -> Self {
        Self::new().with_span(span)
    }

    fn with_span(mut self, span: impl Into<Responsive<u16>>) -> Self {
        self.span = span.into();
        self
    }

    /// How many of the grid's columns are left empty before this one.
    pub fn offset(mut self, offset: impl Into<Responsive<u16>>) -> Self {
        self.offset = offset.into();
        self
    }

    /// Adds a child.
    pub fn child(mut self, child: impl gpui_kit::IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Adds several children.
    pub fn children(
        mut self,
        children: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
    ) -> Self {
        self.children
            .extend(children.into_iter().map(|child| child.into_any_element()));
        self
    }
}

impl From<AnyElement> for Col {
    fn from(element: AnyElement) -> Self {
        let mut column = Col::new();
        column.children.push(element);
        column
    }
}

impl gpui_kit::Styled for Col {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Col {
    /// The grid lines this column occupies at a given width.
    ///
    /// Lines are counted from one, the way CSS counts them: a column that
    /// spans six of twelve columns and starts after two empty ones runs from
    /// line three to line nine.
    pub fn placement(&self, width: f32) -> (u16, u16) {
        let span = self.span.resolve(width).max(1);
        let offset = self.offset.resolve(width);
        (offset + 1, offset + 1 + span)
    }

    fn render_at(self, width: Pixels, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let (start, end) = self.placement(width.as_f32());
        let mut element = div();
        if start > 1 {
            element = element.col_start(start as i16).col_end(end as i16);
        } else {
            element = element.col_span(end - 1);
        }
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for child in self.children {
            element = element.child(child);
        }
        element
    }
}

impl RenderOnce for Col {
    fn render(self, window: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        let width = ContainerWidth::Viewport.resolve(window);
        self.render_at(width, window, cx)
    }
}

impl_into_element!(Col);

#[cfg(test)]
mod tests {
    use super::{Col, DEFAULT_COLUMNS, Grid};
    use crate::align::Justify;
    use crate::responsive::{Breakpoint, Responsive};
    use crate::tokens::Size;
    use crate::width::ContainerWidth;

    #[test]
    fn a_new_grid_has_twelve_columns_and_the_default_gutter() {
        let grid = Grid::new();
        assert_eq!(grid.columns, DEFAULT_COLUMNS);
        assert_eq!(grid.gutter, Size::Md.spacing());
        assert_eq!(grid.justify, Justify::Start);
        assert_eq!(grid.container_width, ContainerWidth::Viewport);
        assert!(grid.children.is_empty());
    }

    #[test]
    fn a_grid_always_has_at_least_one_column() {
        assert_eq!(Grid::new().columns(0).columns, 1);
        assert_eq!(Grid::new().columns(6).columns, 6);
    }

    #[test]
    fn a_column_without_a_span_fills_the_grid() {
        let column = Col::new();
        assert_eq!(column.placement(1200.), (1, DEFAULT_COLUMNS + 1));
    }

    #[test]
    fn an_offset_pushes_the_column_to_the_right() {
        let column = Col::span(6).offset(2);
        assert_eq!(column.placement(1200.), (3, 9));
    }

    #[test]
    fn a_column_can_be_built_from_a_plain_span() {
        let column = Col::span(4);
        assert_eq!(column.placement(1200.), (1, 5));
    }

    #[test]
    fn a_span_never_collapses_to_nothing() {
        let column = Col::span(0);
        assert_eq!(column.placement(1200.), (1, 2));
    }

    #[test]
    fn a_column_can_change_span_per_breakpoint() {
        let column = Col::span(Responsive::base(12).at(Breakpoint::Md, 6));
        assert_eq!(column.placement(500.), (1, 13));
        assert_eq!(column.placement(900.), (1, 7));
    }

    #[test]
    fn a_column_can_change_offset_per_breakpoint() {
        let column = Col::new().offset(Responsive::base(0).at(Breakpoint::Lg, 3));
        assert_eq!(column.placement(500.), (1, 13));
        assert_eq!(column.placement(1200.), (4, 16));
    }
}
