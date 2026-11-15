//! A flex row or column.

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

use crate::align::{Align, Justify};
use crate::macros::impl_into_element;
use crate::style::{align_items, justify_content};
use crate::tokens::Size;

/// Which way a [`Flex`] lays its children out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    /// Left to right.
    #[default]
    Row,
    /// Right to left.
    RowReverse,
    /// Top to bottom.
    Column,
    /// Bottom to top.
    ColumnReverse,
}

/// A row or a column of children, with wrapping under your control.
///
/// [`crate::Group`] is the everyday row; `Flex` is the row or column when the
/// direction itself is the thing being decided.
///
/// ```
/// use guiraffe_layout::{Flex, FlexDirection, Size};
///
/// let flex = Flex::new()
///     .direction(FlexDirection::Column)
///     .gap(Size::Sm)
///     .child("first")
///     .child("second");
/// ```
pub struct Flex {
    direction: FlexDirection,
    gap: f32,
    align: Align,
    justify: Justify,
    grow: bool,
    wrap: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for Flex {
    fn default() -> Self {
        Self::new()
    }
}

impl Flex {
    /// A horizontal flex row with the default gap.
    pub fn new() -> Self {
        Self {
            direction: FlexDirection::Row,
            gap: Size::Md.spacing(),
            align: Align::Start,
            justify: Justify::Start,
            grow: false,
            wrap: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Which way the children are laid out.
    pub fn direction(mut self, direction: FlexDirection) -> Self {
        self.direction = direction;
        self
    }

    /// The space between children.
    pub fn gap(mut self, gap: Size) -> Self {
        self.gap = gap.spacing();
        self
    }

    /// The space between children, in pixels.
    pub fn gap_px(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Where children sit across the main axis.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Where children sit along the main axis.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    /// Whether the flex box grows to fill the space its parent offers.
    pub fn grow(mut self, grow: bool) -> Self {
        self.grow = grow;
        self
    }

    /// Whether children that do not fit move to the next line.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
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

impl gpui_kit::Styled for Flex {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Flex {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().flex().gap(px(self.gap));
        element = match self.direction {
            FlexDirection::Row => element.flex_row(),
            FlexDirection::RowReverse => element.flex_row_reverse(),
            FlexDirection::Column => element.flex_col(),
            FlexDirection::ColumnReverse => element.flex_col_reverse(),
        };
        if self.wrap {
            element = element.flex_wrap();
        } else {
            element = element.flex_nowrap();
        }
        element = align_items(element, self.align);
        element = justify_content(element, self.justify);
        if self.grow {
            element = element.flex_grow_1();
        }
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for child in self.children {
            element = element.child(child);
        }
        element
    }
}

impl_into_element!(Flex);

#[cfg(test)]
mod tests {
    use super::{Flex, FlexDirection};
    use crate::align::{Align, Justify};
    use crate::tokens::Size;

    #[test]
    fn a_new_flex_is_a_row_that_does_not_wrap() {
        let flex = Flex::new();
        assert_eq!(flex.direction, FlexDirection::Row);
        assert_eq!(flex.gap, Size::Md.spacing());
        assert_eq!(flex.align, Align::Start);
        assert_eq!(flex.justify, Justify::Start);
        assert!(!flex.wrap);
        assert!(!flex.grow);
    }

    #[test]
    fn the_direction_can_be_any_of_the_four() {
        assert_eq!(
            Flex::new().direction(FlexDirection::Column).direction,
            FlexDirection::Column
        );
        assert_eq!(
            Flex::new().direction(FlexDirection::RowReverse).direction,
            FlexDirection::RowReverse
        );
        assert_eq!(
            Flex::new()
                .direction(FlexDirection::ColumnReverse)
                .direction,
            FlexDirection::ColumnReverse
        );
    }

    #[test]
    fn wrapping_and_growth_are_recorded() {
        let flex = Flex::new().wrap(true).grow(true);
        assert!(flex.wrap);
        assert!(flex.grow);
    }
}
