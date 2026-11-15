//! A rule between two blocks, optionally with a label in it.

use crate::gpui::IntoElement;

use gpui_kit::{
    App, Hsla, ParentElement as _, RenderOnce, SharedString, StyleRefinement, Styled as _, Window,
    div, px,
};

use crate::tokens::Size;

/// Which way a [`Divider`] runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DividerOrientation {
    /// Left to right.
    #[default]
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// Where the label of a divider sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LabelPosition {
    /// At the start of the rule.
    Start,
    /// In the middle of the rule.
    #[default]
    Center,
    /// At the end of the rule.
    End,
}

/// A hairline between two blocks of content.
///
/// A divider may carry a label, which sits in a gap in the rule.
///
/// ```
/// use guiraffe_layout::Divider;
///
/// let divider = Divider::new().label("Section");
/// ```
#[derive(IntoElement)]
pub struct Divider {
    orientation: DividerOrientation,
    label: Option<SharedString>,
    label_position: LabelPosition,
    color: Option<Hsla>,
    thickness: f32,
    style: StyleRefinement,
}

impl Default for Divider {
    fn default() -> Self {
        Self::new()
    }
}

impl Divider {
    /// A horizontal rule.
    pub fn new() -> Self {
        Self {
            orientation: DividerOrientation::Horizontal,
            label: None,
            label_position: LabelPosition::Center,
            color: None,
            thickness: 1.,
            style: StyleRefinement::default(),
        }
    }

    /// A vertical rule.
    pub fn vertical() -> Self {
        Self::new().orientation(DividerOrientation::Vertical)
    }

    /// Which way the rule runs.
    pub fn orientation(mut self, orientation: DividerOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// The label that sits in the rule.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Where the label sits.
    pub fn label_position(mut self, label_position: LabelPosition) -> Self {
        self.label_position = label_position;
        self
    }

    /// The color of the rule.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// The thickness of the rule, in pixels.
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }
}

impl gpui_kit::Styled for Divider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Divider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        let color = self.color.unwrap_or_else(|| crate::theme::hairline(cx));
        let label = self.label.clone();
        let thickness = self.thickness;

        let mut element = match self.orientation {
            DividerOrientation::Horizontal => div().flex().flex_row().items_center(),
            DividerOrientation::Vertical => div().flex().flex_col().items_center(),
        };

        let line = |color: Hsla| match self.orientation {
            DividerOrientation::Horizontal => div().h(px(thickness)).flex_1().bg(color),
            DividerOrientation::Vertical => div().w(px(thickness)).flex_1().bg(color),
        };

        let label_element = label.map(|label| {
            div()
                .px(px(Size::Xs.spacing()))
                .text_xs()
                .text_color(crate::theme::muted_foreground(cx))
                .child(label)
        });

        match (self.label_position, label_element) {
            (LabelPosition::Center, Some(label)) => {
                element = element.child(line(color)).child(label).child(line(color));
            }
            (LabelPosition::Start, Some(label)) => {
                element = element.child(label).child(line(color));
            }
            (LabelPosition::End, Some(label)) => {
                element = element.child(line(color)).child(label);
            }
            (_, None) => {
                element = element.child(line(color));
            }
        }

        gpui_kit::base::StyledExt::refine_style(element, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{Divider, DividerOrientation, LabelPosition};

    #[test]
    fn a_new_divider_is_a_horizontal_rule_without_a_label() {
        let divider = Divider::new();
        assert_eq!(divider.orientation, DividerOrientation::Horizontal);
        assert_eq!(divider.label_position, LabelPosition::Center);
        assert_eq!(divider.label, None);
        assert_eq!(divider.thickness, 1.);
    }

    #[test]
    fn a_divider_can_run_vertically() {
        assert_eq!(
            Divider::vertical().orientation,
            DividerOrientation::Vertical
        );
    }

    #[test]
    fn the_label_and_its_position_are_remembered() {
        let divider = Divider::new()
            .label("Section")
            .label_position(LabelPosition::End);
        assert_eq!(divider.label.as_deref(), Some("Section"));
        assert_eq!(divider.label_position, LabelPosition::End);
    }
}
