//! A box that keeps a fixed shape.

use crate::gpui::IntoElement;

use gpui_kit::{
    AnyElement, App, ParentElement as _, RenderOnce, StyleRefinement, Styled as _, Window, div,
};

/// Content in a box whose height follows from its width.
///
/// The ratio is width over height: `16.0 / 9.0` is a wide box, `1.0` a square.
///
/// The ratio is limited to the range a box can hold: a ratio that is not a
/// number falls back to a square, and a ratio outside `0.01..=100` is trimmed
/// to it.
///
/// ```
/// use guiraffe_layout::AspectRatio;
///
/// let ratio = AspectRatio::new(16.0 / 9.0).child("video");
/// ```
#[derive(IntoElement)]
pub struct AspectRatio {
    ratio: f32,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

/// The narrowest ratio a box may have.
const MIN_RATIO: f32 = 0.01;

/// The widest ratio a box may have.
const MAX_RATIO: f32 = 100.;

/// The ratio a box gets when it is told something that is not a number.
const FALLBACK_RATIO: f32 = 1.;

/// Keeps a ratio inside the range a box can hold.
fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_nan() {
        return FALLBACK_RATIO;
    }
    ratio.clamp(MIN_RATIO, MAX_RATIO)
}

/// The ratio a box gets, and a word for the developer when it is not one.
///
/// A ratio that is not a number, or one outside the range a box can hold, is
/// a mistake: the box keeps a ratio that works, and the developer is told
/// about it.
fn checked_ratio(ratio: f32) -> f32 {
    debug_assert!(
        !ratio.is_nan() && ratio.is_finite(),
        "a ratio that is not a number is not a ratio: {ratio}"
    );
    debug_assert!(
        (MIN_RATIO..=MAX_RATIO).contains(&ratio),
        "a ratio outside {MIN_RATIO}..={MAX_RATIO} is not a ratio a box can hold: {ratio}"
    );
    clamp_ratio(ratio)
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
            ratio: checked_ratio(ratio),
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// The width-over-height ratio of the box.
    pub fn ratio(mut self, ratio: f32) -> Self {
        self.ratio = checked_ratio(ratio);
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
        let mut overlay = div().absolute().inset_0().overflow_hidden();
        for child in self.children {
            overlay = overlay.child(child);
        }

        let mut element = div()
            .relative()
            .aspect_ratio(self.ratio)
            .overflow_hidden()
            .child(overlay);
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        element
    }
}

#[cfg(test)]
mod tests {
    use super::{AspectRatio, clamp_ratio};

    #[test]
    fn the_ratio_is_kept() {
        assert_eq!(AspectRatio::new(16. / 9.).ratio, 16. / 9.);
        assert_eq!(AspectRatio::new(1.).ratio(2.).ratio, 2.);
    }

    #[test]
    fn a_ratio_never_collapses_to_zero() {
        assert!(clamp_ratio(0.) > 0.);
        assert!(clamp_ratio(-4.) > 0.);
        assert!(clamp_ratio(-1.) > 0.);
    }

    #[test]
    fn a_ratio_stays_inside_the_range_a_box_can_hold() {
        assert_eq!(clamp_ratio(0.), 0.01, "the narrowest ratio");
        assert_eq!(clamp_ratio(-4.), 0.01);
        assert_eq!(clamp_ratio(1000.), 100., "the widest ratio");
    }

    #[test]
    fn a_ratio_that_is_not_a_number_falls_back_to_a_square() {
        assert_eq!(clamp_ratio(f32::NAN), 1.);
        assert_eq!(clamp_ratio(f32::INFINITY), 100.);
        assert_eq!(clamp_ratio(f32::NEG_INFINITY), 0.01);
    }

    #[test]
    #[should_panic(expected = "a ratio outside")]
    fn a_ratio_outside_the_range_a_box_can_hold_is_noticed() {
        let _ = AspectRatio::new(1000.);
    }

    #[test]
    #[should_panic(expected = "a ratio that is not a number")]
    fn a_ratio_that_is_not_a_number_is_noticed() {
        let _ = AspectRatio::new(f32::NAN);
    }

    #[test]
    fn the_default_ratio_is_a_square() {
        assert_eq!(AspectRatio::default().ratio, 1.);
    }
}
