//! Shared styling helpers.

use gpui_kit::Styled;

use crate::align::{Align, Justify};

/// Keeps a gap a distance.
///
/// A gap is never negative and never a number that is not a number: a
/// developer who asks for one is told about it, and the layout still gets a
/// gap it can use.
pub(crate) fn gap_value(gap: f32) -> f32 {
    debug_assert!(
        gap.is_finite() && gap >= 0.,
        "a gap is a distance: it cannot be negative or NaN"
    );
    if gap.is_finite() { gap.max(0.) } else { 0. }
}

/// Places children across the cross axis.
pub(crate) fn align_items<S: Styled>(element: S, align: Align) -> S {
    match align {
        Align::Start => element.items_start(),
        Align::Center => element.items_center(),
        Align::End => element.items_end(),
        Align::Stretch => element.items_stretch(),
        Align::Baseline => element.items_baseline(),
    }
}

/// Places children along the main axis.
pub(crate) fn justify_content<S: Styled>(element: S, justify: Justify) -> S {
    match justify {
        Justify::Start => element.justify_start(),
        Justify::Center => element.justify_center(),
        Justify::End => element.justify_end(),
        Justify::Between => element.justify_between(),
        Justify::Around => element.justify_around(),
        Justify::Evenly => element.justify_evenly(),
    }
}

#[cfg(test)]
mod tests {
    use super::gap_value;

    #[test]
    fn a_gap_keeps_its_value() {
        assert_eq!(gap_value(0.), 0.);
        assert_eq!(gap_value(16.), 16.);
    }

    #[test]
    #[should_panic(expected = "a gap is a distance")]
    fn a_negative_gap_is_noticed_in_a_debug_build() {
        let _ = gap_value(-4.);
    }

    #[test]
    #[should_panic(expected = "a gap is a distance")]
    fn a_gap_that_is_not_a_number_is_noticed_in_a_debug_build() {
        let _ = gap_value(f32::NAN);
    }
}
