//! A twelve-column grid and the columns that fill it.

use crate::gpui::IntoElement;
use gpui_kit::base::ElementExt as _;

use gpui_kit::{
    AnyElement, App, ElementId, ParentElement as _, RenderOnce, StyleRefinement, Styled as _,
    Window, div, px,
};

use crate::align::Justify;
use crate::responsive::Responsive;
use crate::style::justify_content;
use crate::tokens::Size;
use crate::width::{ContainerWidth, ContainerWidthState, measure, measured_width};

/// The number of columns a grid has unless it is told otherwise.
pub const DEFAULT_COLUMNS: u16 = 12;

/// How many of the grid's columns a [`Col`] takes.
///
/// A column that takes the whole grid is the default: it fills the row it is
/// in, whatever the grid's column count is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    /// The whole width of the grid.
    Full,
    /// A number of the grid's columns.
    Columns(u16),
}

impl From<u16> for Span {
    fn from(columns: u16) -> Self {
        Span::Columns(columns)
    }
}

/// Where a column sits in the grid.
///
/// Lines are counted from one, the way CSS counts them: a column that spans
/// six of twelve columns and starts after two empty ones runs from line three
/// to line nine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlacement {
    /// The row the column is in, counted from one.
    pub row: u16,
    /// The first grid line the column covers.
    pub start: u16,
    /// The line after the last one the column covers.
    pub end: u16,
}

/// Keeps a line number inside the range GPUI can be given.
fn to_i16(line: u16) -> i16 {
    i16::try_from(line).unwrap_or(i16::MAX)
}

/// A grid of columns.
///
/// A grid has twelve columns by default. Each [`Col`] takes a span of those
/// columns and may leave an offset before it. Both can change with the width
/// the grid is laid out in.
///
/// The grid follows the box it is laid out in: it measures that box and
/// resolves its columns against it, so a grid inside a narrow pane answers
/// to the pane and not to the window. Name a width of your own with
/// [`container_width`](Grid::container_width) to resolve against something
/// else.
///
/// A column that does not fit in the row it is placed in moves down to the
/// next one, the way a grid of columns wraps.
///
/// ```
/// use guiraffe_layout::{Col, Grid, Size};
///
/// let grid = Grid::new()
///     .gutter(Size::Sm)
///     .child(Col::span(6).child("left"))
///     .child(Col::span(6).child("right"));
/// ```
#[derive(IntoElement)]
pub struct Grid {
    columns: u16,
    gutter: f32,
    justify: Justify,
    grow: bool,
    /// The width the grid resolves against, or nothing when the grid
    /// measures the box it is laid out in.
    container_width: Option<ContainerWidth>,
    children: Vec<Col>,
    style: StyleRefinement,
    /// The key the measurement of the grid is kept under.
    ///
    /// The measurement is keyed by the place the grid was built: two grids
    /// built in the same place share their measurement, because they are in
    /// the same container and have the same width.
    key: ElementId,
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Grid {
    /// A grid with twelve columns and the default gutter.
    #[track_caller]
    pub fn new() -> Self {
        let location = std::panic::Location::caller();
        Self {
            columns: DEFAULT_COLUMNS,
            gutter: Size::Md.spacing(),
            justify: Justify::Start,
            grow: false,
            container_width: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
            key: ElementId::CodeLocation(*location),
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
        self.gutter = crate::style::gap_value(gutter);
        self
    }

    /// Where the columns sit along the row.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    /// Whether the columns in the last row grow to fill all the room the row
    /// has, the way Mantine's `grow` does: every column of the last row
    /// grows by the same amount until the row is full.
    pub fn grow(mut self, grow: bool) -> Self {
        self.grow = grow;
        self
    }

    /// The width the grid resolves its responsive columns against.
    ///
    /// The grid measures the box it is laid out in by default. Name a width
    /// of your own to resolve against something else: the width of the
    /// window, a width you know, or a box you measure yourself.
    pub fn container_width(mut self, container_width: ContainerWidth) -> Self {
        self.container_width = Some(container_width);
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

    /// Where the grid's columns sit at a width.
    ///
    /// The columns fill the grid one after another. A column that does not
    /// fit in the row it would land in moves down to the next row, and the
    /// grid never grows past the columns it has.
    ///
    /// An offset is the room a column leaves empty *after* the column before
    /// it, the way Mantine counts it: the offset of the first column of a row
    /// counts from the edge of the grid, and the offset of any other column
    /// counts from where the column before it ends. A column that does not
    /// fit in the room its offset leaves moves down to the next row, where
    /// the offset is trimmed to what that row can afford.
    pub fn layout(&self, width: f32) -> Vec<GridPlacement> {
        let columns = u32::from(self.columns.max(1));
        let mut placements = Vec::with_capacity(self.children.len());
        let mut row = 1u32;
        let mut cursor = 0u32;

        for column in &self.children {
            let span = u32::from(column.span_at(self.columns, width));
            let offset = u32::from(column.offset_at(self.columns, width));

            let (start, end) = if cursor + offset + span > columns {
                // The column does not fit in what is left of the row: it
                // moves down, and the offset it asked for is trimmed to what
                // the new row can afford.
                row += 1;
                cursor = 0;
                let offset = offset.min(columns.saturating_sub(span));
                (offset + 1, offset + 1 + span)
            } else {
                let start = cursor + offset + 1;
                (start, start + span)
            };

            cursor = cursor.max(end - 1);
            placements.push(GridPlacement {
                row: u16::try_from(row).unwrap_or(u16::MAX),
                start: u16::try_from(start).unwrap_or(u16::MAX),
                end: u16::try_from(end).unwrap_or(u16::MAX),
            });
        }

        placements
    }
}

impl gpui_kit::Styled for Grid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Grid {
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
        let placements = self.layout(width);

        let mut rows: Vec<Vec<(GridPlacement, Col)>> = Vec::new();
        for (column, placement) in self.children.into_iter().zip(placements) {
            match rows.last_mut() {
                Some(row)
                    if row
                        .first()
                        .is_some_and(|(first, _)| first.row == placement.row) =>
                {
                    row.push((placement, column));
                }
                _ => rows.push(vec![(placement, column)]),
            }
        }

        let last_row = rows.len().saturating_sub(1);
        let mut grid = div().flex().flex_col().gap(px(self.gutter));
        if measurement.is_some() {
            // A grid that measures its container fills it, so the measurement
            // is the width of the container and not the width of the columns.
            grid = grid.relative().w_full();
        }

        let columns = f32::from(self.columns.max(1));
        for (index, row) in rows.into_iter().enumerate() {
            if self.grow && index == last_row {
                // The last row of a growing grid is a flex row: every column
                // starts at its span and grows by the same share of what is
                // left, so the row is full.
                let mut line = div().flex().flex_row().w_full().gap(px(self.gutter));
                line = justify_content(line, self.justify);
                let mut previous_end = 1u16;
                for (placement, column) in row {
                    let span = f32::from(placement.end.saturating_sub(placement.start));
                    let offset = f32::from(placement.start.saturating_sub(previous_end));
                    previous_end = placement.end;
                    line = line.child(column.render_growing(span / columns, offset / columns));
                }
                grid = grid.child(line);
                continue;
            }
            let mut line = div().grid().grid_cols(self.columns).gap(px(self.gutter));
            line = justify_content(line, self.justify);
            for (placement, column) in row {
                line = line.child(column.render_at(placement, window, cx));
            }
            grid = grid.child(line);
        }
        if let Some(state) = measurement {
            let state = state.clone();
            grid = grid.on_prepaint(move |bounds, window, cx| {
                measure(&state, owner, bounds, window, cx);
            });
        }

        gpui_kit::base::StyledExt::refine_style(grid, &self.style)
    }
}

/// One column of a [`Grid`].
///
/// A column spans a number of the grid's columns and may start after an
/// offset. Both numbers can be different per breakpoint. A column without a
/// span fills the grid.
///
/// ```
/// use guiraffe_layout::{Breakpoint, Col, Responsive, Span};
///
/// let column = Col::span(Responsive::base(12).at(Breakpoint::Md, 6));
/// let full = Col::span(Span::Full);
/// ```
pub struct Col {
    span: Responsive<Span>,
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
    /// A column that fills the grid.
    pub fn new() -> Self {
        Self {
            span: Responsive::value(Span::Full),
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
    pub fn span(span: impl Into<Responsive<Span>>) -> Self {
        Self::new().with_span(span)
    }

    fn with_span(mut self, span: impl Into<Responsive<Span>>) -> Self {
        self.span = span.into();
        self
    }

    /// How many of the grid's columns are left empty after the column before
    /// this one.
    ///
    /// The offset counts from where the column before it ends, the way
    /// Mantine counts it: the first column of a row leaves its offset empty
    /// from the edge of the grid.
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

impl From<u16> for Responsive<Span> {
    fn from(columns: u16) -> Self {
        Responsive::value(Span::Columns(columns))
    }
}

impl From<Responsive<u16>> for Responsive<Span> {
    fn from(span: Responsive<u16>) -> Self {
        span.map(Span::Columns)
    }
}

impl gpui_kit::Styled for Col {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Col {
    /// How many of the grid's columns this column takes at a width.
    ///
    /// A column that fills the grid takes all of them; a column that names a
    /// number takes that many, never more than the grid has.
    pub fn span_at(&self, columns: u16, width: f32) -> u16 {
        let columns = columns.max(1);
        match self.span.resolve(width) {
            Span::Full => columns,
            Span::Columns(span) => span.clamp(1, columns),
        }
    }

    /// How many columns are left empty before this one at a width.
    pub fn offset_at(&self, columns: u16, width: f32) -> u16 {
        let columns = columns.max(1);
        self.offset.resolve(width).min(columns.saturating_sub(1))
    }

    fn render_at(
        self,
        placement: GridPlacement,
        _: &mut Window,
        _: &mut App,
    ) -> impl gpui_kit::IntoElement {
        // Every row of the grid is a grid of its own, so a column always
        // sits in the first row of the one it belongs to.
        let mut element = div()
            .col_start(to_i16(placement.start))
            .col_end(to_i16(placement.end))
            .row_start(1)
            .row_end(2);
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for child in self.children {
            element = element.child(child);
        }
        element
    }
}

impl Col {
    /// The column as a growing cell of a flex row: it starts at `basis` of
    /// the row and leaves `offset` of the row empty before it.
    fn render_growing(self, basis: f32, offset: f32) -> impl gpui_kit::IntoElement {
        let mut element = div()
            .flex_basis(gpui_kit::relative(basis))
            .flex_grow_1()
            .flex_shrink_1()
            .min_w_0();
        if offset > 0. {
            element = element.ml(gpui_kit::relative(offset));
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
        let width = ContainerWidth::Viewport.resolve(window, cx).as_f32();
        let placement = GridPlacement {
            row: 1,
            start: 1,
            end: self.span_at(DEFAULT_COLUMNS, width) + 1,
        };
        self.render_at(placement, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{Col, DEFAULT_COLUMNS, Grid, GridPlacement, Span};
    use crate::align::Justify;
    use crate::responsive::{Breakpoint, Responsive};
    use crate::tokens::Size;

    fn placement(row: u16, start: u16, end: u16) -> GridPlacement {
        GridPlacement { row, start, end }
    }

    #[test]
    fn a_new_grid_has_twelve_columns_and_the_default_gutter() {
        let grid = Grid::new();
        assert_eq!(grid.columns, DEFAULT_COLUMNS);
        assert_eq!(grid.gutter, Size::Md.spacing());
        assert_eq!(grid.justify, Justify::Start);
        assert_eq!(
            grid.container_width, None,
            "the grid measures the box it is laid out in"
        );
        assert!(grid.children.is_empty());
    }

    #[test]
    fn a_grid_always_has_at_least_one_column() {
        assert_eq!(Grid::new().columns(0).columns, 1);
        assert_eq!(Grid::new().columns(6).columns, 6);
    }

    #[test]
    fn a_column_without_a_span_fills_the_grid() {
        let grid = Grid::new().columns(6).child(Col::new());
        assert_eq!(grid.layout(1200.), vec![placement(1, 1, 7)]);
    }

    #[test]
    fn a_column_without_a_span_fills_a_wider_grid_too() {
        let grid = Grid::new().columns(24).child(Col::new());
        assert_eq!(grid.layout(1200.), vec![placement(1, 1, 25)]);
    }

    #[test]
    fn an_offset_pushes_the_column_to_the_right() {
        let grid = Grid::new().child(Col::span(6).offset(2));
        assert_eq!(grid.layout(1200.), vec![placement(1, 3, 9)]);
    }

    #[test]
    fn an_offset_counts_from_where_the_column_before_it_ends() {
        let grid = Grid::new()
            .child(Col::span(4))
            .child(Col::span(4).offset(2));
        assert_eq!(
            grid.layout(1200.),
            vec![placement(1, 1, 5), placement(1, 7, 11)],
            "the offset is the room after the column before it, so the two do not overlap"
        );
    }

    #[test]
    fn an_offset_after_a_column_that_fills_the_row_moves_the_column_down() {
        let grid = Grid::new()
            .child(Col::span(8))
            .child(Col::span(4).offset(2));
        assert_eq!(
            grid.layout(1200.),
            vec![placement(1, 1, 9), placement(2, 3, 7)],
            "the column does not fit in what is left of the row"
        );
    }

    #[test]
    fn an_offset_that_does_not_fit_the_row_is_trimmed() {
        let grid = Grid::new().child(Col::span(2).offset(20));
        assert_eq!(
            grid.layout(1200.),
            vec![placement(2, 11, 13)],
            "the offset is trimmed to what the row can afford, and the column stays inside the grid"
        );
    }

    #[test]
    fn a_column_can_be_built_from_a_plain_span() {
        let grid = Grid::new().child(Col::span(4));
        assert_eq!(grid.layout(1200.), vec![placement(1, 1, 5)]);
    }

    #[test]
    fn a_span_never_collapses_to_nothing() {
        let grid = Grid::new().child(Col::span(0));
        assert_eq!(grid.layout(1200.), vec![placement(1, 1, 2)]);
    }

    #[test]
    fn the_columns_fill_the_grid_one_after_the_other() {
        let grid = Grid::new().child(Col::span(6)).child(Col::span(6));
        assert_eq!(
            grid.layout(1200.),
            vec![placement(1, 1, 7), placement(1, 7, 13)]
        );
    }

    #[test]
    fn a_column_that_does_not_fit_moves_to_the_next_row() {
        let grid = Grid::new().child(Col::span(8)).child(Col::span(8));
        assert_eq!(
            grid.layout(1200.),
            vec![placement(1, 1, 9), placement(2, 1, 9)]
        );
    }

    #[test]
    fn a_column_never_reaches_past_the_last_column_of_the_grid() {
        let grid = Grid::new().columns(4).child(Col::span(12));
        assert_eq!(grid.layout(1200.), vec![placement(1, 1, 5)]);
    }

    #[test]
    fn an_offset_that_does_not_fit_moves_the_column_to_the_next_row() {
        let grid = Grid::new()
            .child(Col::span(4))
            .child(Col::span(4).offset(10));
        assert_eq!(
            grid.layout(1200.),
            vec![placement(1, 1, 5), placement(2, 9, 13)],
            "the offset is trimmed so the column still fits"
        );
    }

    #[test]
    fn a_column_can_change_span_per_breakpoint() {
        let grid = Grid::new().child(Col::span(Responsive::base(12).at(Breakpoint::Md, 6)));
        assert_eq!(grid.layout(500.), vec![placement(1, 1, 13)]);
        assert_eq!(grid.layout(1000.), vec![placement(1, 1, 7)]);
    }

    #[test]
    fn a_column_can_change_offset_per_breakpoint() {
        let grid = Grid::new().child(Col::new().offset(Responsive::base(0).at(Breakpoint::Lg, 3)));
        assert_eq!(grid.layout(500.), vec![placement(1, 1, 13)]);
        assert_eq!(
            grid.layout(1300.),
            vec![placement(2, 1, 13)],
            "the offset no longer fits, so the column moves down"
        );
    }

    #[test]
    fn huge_numbers_do_not_overflow() {
        let grid = Grid::new().child(Col::span(u16::MAX).offset(u16::MAX));
        let layout = grid.layout(1200.);
        assert_eq!(layout.len(), 1);
        assert!(layout[0].start <= grid.columns);
        assert!(layout[0].end <= grid.columns + 1);
    }

    #[test]
    fn a_full_span_can_be_set_explicitly() {
        let grid = Grid::new().columns(6).child(Col::span(Span::Full));
        assert_eq!(grid.layout(1200.), vec![placement(1, 1, 7)]);
    }
}
