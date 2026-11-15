//! Shared styling helpers.

use gpui_kit::Styled;

use crate::align::{Align, Justify};

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
