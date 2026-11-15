//! Content centered in both directions.

use gpui_kit::{
    AnyElement, App, ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled as _, Window,
    div,
};

use crate::macros::impl_into_element;

/// Content centered on both axes.
///
/// ```
/// use guiraffe_layout::Center;
///
/// let center = Center::new().child("middle");
/// ```
pub struct Center {
    min_height: Option<Pixels>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Default for Center {
    fn default() -> Self {
        Self::new()
    }
}

impl Center {
    /// A center with no minimum height.
    pub fn new() -> Self {
        Self {
            min_height: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// The height the center grows to at least.
    pub fn min_height(mut self, min_height: impl Into<Pixels>) -> Self {
        self.min_height = Some(min_height.into());
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

impl gpui_kit::Styled for Center {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Center {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().flex().items_center().justify_center();
        if let Some(min_height) = self.min_height {
            element = element.min_h(min_height);
        }
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for child in self.children {
            element = element.child(child);
        }
        element
    }
}

impl_into_element!(Center);

#[cfg(test)]
mod tests {
    use super::Center;
    use gpui_kit::px;

    #[test]
    fn a_new_center_has_no_minimum_height() {
        assert_eq!(Center::new().min_height, None);
    }

    #[test]
    fn the_minimum_height_is_remembered() {
        assert_eq!(
            Center::new().min_height(px(120.)).min_height,
            Some(px(120.))
        );
    }
}
