//! A box that keeps a fixed shape.

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div,
};

use crate::macros::impl_into_element;

/// Content in a box whose height follows from its width.
///
/// The ratio is width over height: `16.0 / 9.0` is a wide box, `1.0` a square.
///
/// ```
/// use guiraffe_layout::AspectRatio;
///
/// let ratio = AspectRatio::new(16.0 / 9.0).child("video");
/// ```
pub struct AspectRatio {
    ratio: f32,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for AspectRatio {
    fn default() -> Self {
        Self::new(1.)
    }
}

impl AspectRatio {
    /// A box with the given width-over-height ratio.
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio: ratio.max(f32::MIN_POSITIVE),
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// The width-over-height ratio of the box.
    pub fn ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio.max(f32::MIN_POSITIVE);
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

impl gpui_kit::Styled for AspectRatio {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AspectRatio {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut overlay = div().absolute().inset_0();
        for child in self.children {
            overlay = overlay.child(child);
        }

        let mut element = div().aspect_ratio(self.ratio).child(overlay);
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        element
    }
}

impl_into_element!(AspectRatio);

#[cfg(test)]
mod tests {
    use super::AspectRatio;

    #[test]
    fn the_ratio_is_kept() {
        assert_eq!(AspectRatio::new(16. / 9.).ratio, 16. / 9.);
        assert_eq!(AspectRatio::new(1.).ratio(2.).ratio, 2.);
    }

    #[test]
    fn a_ratio_never_collapses_to_zero() {
        assert!(AspectRatio::new(0.).ratio > 0.);
        assert!(AspectRatio::new(-4.).ratio > 0.);
        assert!(AspectRatio::new(1.).ratio(-1.).ratio > 0.);
    }

    #[test]
    fn the_default_ratio_is_a_square() {
        assert_eq!(AspectRatio::default().ratio, 1.);
    }
}
