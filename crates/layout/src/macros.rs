//! Internal macros.

/// Implements [`gpui_kit::IntoElement`] for a stateless component.
///
/// `gpui-kit` re-exports GPUI, so the derive macro that ships with GPUI cannot
/// be used from a crate that only depends on `gpui-kit`: it expands to paths
/// that start at `gpui::`. This writes the same implementation by hand.
macro_rules! impl_into_element {
    ($ty:ty) => {
        impl gpui_kit::IntoElement for $ty {
            type Element = gpui_kit::ViewElement<$ty>;

            fn into_element(self) -> Self::Element {
                gpui_kit::ViewElement::new(self)
            }

            fn into_any_element(self) -> gpui_kit::AnyElement {
                gpui_kit::Element::into_any(self.into_element())
            }
        }
    };
}

pub(crate) use impl_into_element;
