//! Empty space that pushes its neighbours apart.

use gpui_kit::{App, RenderOnce, StyleRefinement, Styled as _, Window, div, px};

use crate::macros::impl_into_element;
use crate::tokens::Size;

/// A gap with nothing in it.
///
/// ```
/// use guiraffe_layout::{Size, Space};
///
/// let space = Space::h(Size::Md);
/// ```
pub struct Space {
    width: Option<f32>,
    height: Option<f32>,
    style: StyleRefinement,
}

impl Default for Space {
    fn default() -> Self {
        Self::new()
    }
}

impl Space {
    /// Space with neither a width nor a height.
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            style: StyleRefinement::default(),
        }
    }

    /// Space that is tall but not wide.
    pub fn h(size: Size) -> Self {
        Self::new().height(size)
    }

    /// Space that is wide but not tall.
    pub fn w(size: Size) -> Self {
        Self::new().width(size)
    }

    /// The height of the space.
    pub fn height(mut self, size: Size) -> Self {
        self.height = Some(size.spacing());
        self
    }

    /// The height of the space, in pixels.
    pub fn height_px(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// The width of the space.
    pub fn width(mut self, size: Size) -> Self {
        self.width = Some(size.spacing());
        self
    }

    /// The width of the space, in pixels.
    pub fn width_px(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl gpui_kit::Styled for Space {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Space {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().flex_none();
        if let Some(height) = self.height {
            element = element.h(px(height));
        }
        if let Some(width) = self.width {
            element = element.w(px(width));
        }
        gpui_kit::base::StyledExt::refine_style(element, &self.style)
    }
}

impl_into_element!(Space);

#[cfg(test)]
mod tests {
    use super::Space;
    use crate::tokens::Size;

    #[test]
    fn a_new_space_has_no_size_at_all() {
        let space = Space::new();
        assert_eq!(space.width, None);
        assert_eq!(space.height, None);
    }

    #[test]
    fn vertical_space_is_tall_only() {
        let space = Space::h(Size::Md);
        assert_eq!(space.height, Some(Size::Md.spacing()));
        assert_eq!(space.width, None);
    }

    #[test]
    fn horizontal_space_is_wide_only() {
        let space = Space::w(Size::Lg);
        assert_eq!(space.width, Some(Size::Lg.spacing()));
        assert_eq!(space.height, None);
    }

    #[test]
    fn a_space_can_be_both_tall_and_wide() {
        let space = Space::new().height(Size::Sm).width(Size::Xs);
        assert_eq!(space.height, Some(Size::Sm.spacing()));
        assert_eq!(space.width, Some(Size::Xs.spacing()));
    }
}
