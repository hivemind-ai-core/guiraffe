//! A centered, width-limited container.

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

use crate::macros::impl_into_element;
use crate::tokens::Size;

/// How wide a container may get.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ContainerSize {
    /// 320px.
    Xs,
    /// 480px.
    Sm,
    /// 720px.
    #[default]
    Md,
    /// 960px.
    Lg,
    /// 1200px.
    Xl,
    /// As wide as the parent allows.
    Full,
}

impl ContainerSize {
    /// The width this size stands for, in pixels.
    pub fn max_width(self) -> Option<f32> {
        match self {
            ContainerSize::Xs => Some(320.),
            ContainerSize::Sm => Some(480.),
            ContainerSize::Md => Some(720.),
            ContainerSize::Lg => Some(960.),
            ContainerSize::Xl => Some(1200.),
            ContainerSize::Full => None,
        }
    }
}

/// Content in a column that is centered and never wider than its size says.
///
/// ```
/// use guiraffe_layout::{Container, ContainerSize, Size};
///
/// let container = Container::new()
///     .size(ContainerSize::Md)
///     .padding(Size::Md)
///     .child("content");
/// ```
pub struct Container {
    size: ContainerSize,
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
            size: ContainerSize::Md,
            fluid: false,
            padding: 0.,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// How wide the container may get.
    pub fn size(mut self, size: ContainerSize) -> Self {
        self.size = size;
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
        let mut element = div().mx_auto();
        element = if self.fluid {
            element.w_full()
        } else {
            match self.size.max_width() {
                Some(width) => element.w(px(width)),
                None => element.w_full(),
            }
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

impl_into_element!(Container);

#[cfg(test)]
mod tests {
    use super::{Container, ContainerSize};
    use crate::tokens::Size;

    #[test]
    fn the_container_sizes_grow() {
        let widths: Vec<f32> = [
            ContainerSize::Xs,
            ContainerSize::Sm,
            ContainerSize::Md,
            ContainerSize::Lg,
            ContainerSize::Xl,
        ]
        .iter()
        .map(|size| size.max_width().unwrap_or_default())
        .collect();
        assert_eq!(widths, vec![320., 480., 720., 960., 1200.]);
        assert!(widths.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_full_size_has_no_width_of_its_own() {
        assert_eq!(ContainerSize::Full.max_width(), None);
    }

    #[test]
    fn a_new_container_is_centered_without_padding() {
        let container = Container::new();
        assert_eq!(container.size, ContainerSize::Md);
        assert!(!container.fluid);
        assert_eq!(container.padding, 0.);
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
