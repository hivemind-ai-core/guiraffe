//! A centered, width-limited container.

use crate::gpui::IntoElement;

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

use crate::tokens::Size;

/// Content in a column that is centered and never wider than its size says.
///
/// The width comes from the core size scale: a small container is 720 pixels
/// wide, a medium one 960.
///
/// ```
/// use guiraffe_layout::{Container, Size};
///
/// let container = Container::new()
///     .size(Size::Md)
///     .padding(Size::Md)
///     .child("content");
/// ```
#[derive(IntoElement)]
pub struct Container {
    size: Option<Size>,
    fluid: bool,
    padding: f32,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    /// A container with the default size and no padding.
    pub fn new() -> Self {
        Self {
            size: Some(Size::Md),
            fluid: false,
            padding: 0.,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// How wide the container may get.
    pub fn size(mut self, size: Size) -> Self {
        self.size = Some(size);
        self
    }

    /// Whether the container fills the width of its parent.
    pub fn fluid(mut self, fluid: bool) -> Self {
        self.fluid = fluid;
        self
    }

    /// The space inside the container.
    pub fn padding(mut self, padding: Size) -> Self {
        self.padding = padding.spacing();
        self
    }

    /// The space inside the container, in pixels.
    pub fn padding_px(mut self, padding: f32) -> Self {
        self.padding = padding;
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

impl gpui_kit::Styled for Container {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Container {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().mx_auto().w_full();
        element = match self.size.filter(|_| !self.fluid) {
            Some(size) => element.max_w(px(size.container_width())),
            None => element,
        };
        if self.padding > 0. {
            element = element.p(px(self.padding));
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
    use super::Container;
    use crate::tokens::Size;

    #[test]
    fn a_new_container_is_centered_without_padding() {
        let container = Container::new();
        assert_eq!(container.size, Some(Size::Md));
        assert!(!container.fluid);
        assert_eq!(container.padding, 0.);
    }

    #[test]
    fn the_container_widths_grow_with_the_scale() {
        let widths: Vec<f32> = Size::ALL
            .iter()
            .map(|size| size.container_width())
            .collect();
        assert!(widths.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_size_can_be_changed() {
        assert_eq!(Container::new().size(Size::Lg).size, Some(Size::Lg));
    }

    #[test]
    fn padding_can_be_set_by_token_or_by_pixels() {
        assert_eq!(
            Container::new().padding(Size::Lg).padding,
            Size::Lg.spacing()
        );
        assert_eq!(Container::new().padding_px(7.).padding, 7.);
    }
}
