//! A vertical run of children.

use crate::gpui::IntoElement;

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

use crate::align::{Align, Justify};
use crate::style::{align_items, justify_content};
use crate::tokens::Size;

/// Children stacked on top of each other.
///
/// ```
/// use guiraffe_layout::{Size, Stack};
///
/// let stack = Stack::new().gap(Size::Md).child("first").child("second");
/// ```
#[derive(IntoElement)]
pub struct Stack {
    gap: f32,
    align: Align,
    justify: Justify,
    grow: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Stack {
    /// A stack with the default gap and alignment.
    pub fn new() -> Self {
        Self {
            gap: Size::Md.spacing(),
            align: Align::Stretch,
            justify: Justify::Start,
            grow: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// The space between children.
    pub fn gap(mut self, gap: Size) -> Self {
        self.gap = gap.spacing();
        self
    }

    /// The space between children, in pixels.
    pub fn gap_px(mut self, gap: f32) -> Self {
        self.gap = crate::style::gap_value(gap);
        self
    }

    /// Where children sit across the row.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Where children sit down the column.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    /// Whether the stack grows to fill the space its parent offers.
    pub fn grow(mut self, grow: bool) -> Self {
        self.grow = grow;
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

impl gpui_kit::Styled for Stack {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Stack {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().flex().flex_col().gap(px(self.gap));
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

#[cfg(test)]
mod tests {
    use super::Stack;
    use crate::align::{Align, Justify};
    use crate::tokens::Size;

    #[test]
    fn a_new_stack_uses_the_default_scale() {
        let stack = Stack::new();
        assert_eq!(stack.gap, Size::Md.spacing());
        assert_eq!(stack.align, Align::Stretch);
        assert_eq!(stack.justify, Justify::Start);
        assert!(!stack.grow);
        assert!(stack.children.is_empty());
    }

    #[test]
    fn the_gap_can_be_set_by_token_or_by_pixels() {
        assert_eq!(Stack::new().gap(Size::Lg).gap, Size::Lg.spacing());
        assert_eq!(Stack::new().gap_px(3.).gap, 3.);
    }

    #[test]
    fn alignment_and_growth_are_recorded() {
        let stack = Stack::new()
            .align(Align::Center)
            .justify(Justify::Between)
            .grow(true);
        assert_eq!(stack.align, Align::Center);
        assert_eq!(stack.justify, Justify::Between);
        assert!(stack.grow);
    }
}
