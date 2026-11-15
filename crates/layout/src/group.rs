//! A horizontal run of children.

use crate::gpui::IntoElement;

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

use crate::align::{Align, Justify};
use crate::style::{align_items, justify_content};
use crate::tokens::Size;

/// Children laid out in a row.
///
/// Unlike [`crate::Stack`], a group wraps: children that do not fit move to
/// the next line.
///
/// ```
/// use guiraffe_layout::{Group, Size};
///
/// let group = Group::new().gap(Size::Sm).child("first").child("second");
/// ```
#[derive(IntoElement)]
pub struct Group {
    gap: f32,
    align: Align,
    justify: Justify,
    grow: bool,
    wrap: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for Group {
    fn default() -> Self {
        Self::new()
    }
}

impl Group {
    /// A group with the default gap and alignment.
    pub fn new() -> Self {
        Self {
            gap: Size::Md.spacing(),
            align: Align::Center,
            justify: Justify::Start,
            grow: false,
            wrap: true,
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
        self.gap = gap;
        self
    }

    /// Where children sit across the row.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Where children sit along the row.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    /// Whether the group grows to fill the space its parent offers.
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

impl gpui_kit::Styled for Group {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Group {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().flex().flex_row().gap(px(self.gap));
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

#[cfg(test)]
mod tests {
    use super::Group;
    use crate::align::{Align, Justify};
    use crate::tokens::Size;

    #[test]
    fn a_new_group_wraps_and_centers_its_children() {
        let group = Group::new();
        assert_eq!(group.gap, Size::Md.spacing());
        assert_eq!(group.align, Align::Center);
        assert_eq!(group.justify, Justify::Start);
        assert!(group.wrap);
        assert!(!group.grow);
    }

    #[test]
    fn wrapping_can_be_turned_off() {
        assert!(!Group::new().wrap(false).wrap);
        assert!(Group::new().wrap(true).wrap);
    }

    #[test]
    fn the_gap_can_be_set_by_token_or_by_pixels() {
        assert_eq!(Group::new().gap(Size::Xs).gap, Size::Xs.spacing());
        assert_eq!(Group::new().gap_px(2.).gap, 2.);
    }
}
