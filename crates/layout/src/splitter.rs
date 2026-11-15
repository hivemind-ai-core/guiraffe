//! Two panes with a divider between them.

use std::rc::Rc;

use gpui_kit::component::ResizableState;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, ParentElement as _, Pixels, RenderOnce, StyleRefinement,
    Styled as _, Window, div, px,
};

use crate::macros::impl_into_element;

/// The callback a splitter reports its new pane sizes to.
type ResizeCallback = Rc<dyn Fn(&[Pixels])>;

/// Which way a [`Splitter`] divides its panes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SplitterAxis {
    /// The panes sit side by side.
    #[default]
    Horizontal,
    /// The panes sit one above the other.
    Vertical,
}

/// The sizes of a splitter's panes, ready to be stored and restored.
///
/// The sizes are plain numbers so an application can keep them wherever it
/// keeps its settings.
///
/// ```
/// use guiraffe_layout::SplitterSizes;
/// use gpui_kit::px;
///
/// let sizes = SplitterSizes::from_pixels(&[px(200.), px(300.)]);
/// assert_eq!(sizes.get(0), Some(200.));
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SplitterSizes(Vec<f32>);

impl SplitterSizes {
    /// Reads the sizes of a splitter's panes.
    pub fn from_pixels(sizes: &[Pixels]) -> Self {
        Self(sizes.iter().map(|size| size.as_f32()).collect())
    }

    /// The sizes as pixels.
    pub fn to_pixels(&self) -> Vec<Pixels> {
        self.0.iter().map(|size| px(*size)).collect()
    }

    /// The size of one pane.
    pub fn get(&self, index: usize) -> Option<f32> {
        self.0.get(index).copied()
    }

    /// How many panes there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no panes at all.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The sizes as plain numbers.
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }
}

/// Two panes with a divider the user can drag.
///
/// The splitter keeps its state in a [`ResizableState`], which an application
/// can own so the sizes survive a restart: read them with
/// [`ResizableState::sizes`] and hand them back through
/// [`Splitter::with_state`].
///
/// The id has to stay the same across renders — it is what the panes are
/// remembered by.
///
/// ```
/// use guiraffe_layout::Splitter;
///
/// let splitter = Splitter::new("workspace").first("sidebar").second("content");
/// ```
pub struct Splitter {
    id: ElementId,
    axis: SplitterAxis,
    first: Option<AnyElement>,
    second: Option<AnyElement>,
    first_size: Option<Pixels>,
    min_size: Pixels,
    state: Option<Entity<ResizableState>>,
    on_resize: Option<ResizeCallback>,
    style: StyleRefinement,
}

impl Splitter {
    /// A splitter with two empty panes.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            axis: SplitterAxis::Horizontal,
            first: None,
            second: None,
            first_size: None,
            min_size: px(80.),
            state: None,
            on_resize: None,
            style: StyleRefinement::default(),
        }
    }

    /// Which way the panes are divided.
    pub fn axis(mut self, axis: SplitterAxis) -> Self {
        self.axis = axis;
        self
    }

    /// The first pane.
    pub fn first(mut self, pane: impl gpui_kit::IntoElement) -> Self {
        self.first = Some(pane.into_any_element());
        self
    }

    /// The second pane.
    pub fn second(mut self, pane: impl gpui_kit::IntoElement) -> Self {
        self.second = Some(pane.into_any_element());
        self
    }

    /// The size the first pane starts at.
    pub fn first_size(mut self, size: impl Into<Pixels>) -> Self {
        self.first_size = Some(size.into());
        self
    }

    /// The narrowest a pane may become.
    pub fn min_size(mut self, size: impl Into<Pixels>) -> Self {
        self.min_size = size.into();
        self
    }

    /// The state that remembers the pane sizes.
    pub fn with_state(mut self, state: &Entity<ResizableState>) -> Self {
        self.state = Some(state.clone());
        self
    }

    /// Called whenever the panes are resized.
    pub fn on_resize(mut self, on_resize: impl Fn(&[Pixels]) + 'static) -> Self {
        self.on_resize = Some(Rc::new(on_resize));
        self
    }
}

impl gpui_kit::Styled for Splitter {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Splitter {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut group = match self.axis {
            SplitterAxis::Horizontal => h_resizable(self.id.clone()),
            SplitterAxis::Vertical => v_resizable(self.id.clone()),
        };

        if let Some(state) = &self.state {
            group = group.with_state(state);
        }
        if let Some(on_resize) = &self.on_resize {
            let on_resize = on_resize.clone();
            group = group.on_resize(move |state, _, cx| on_resize(state.read(cx).sizes()));
        }

        let mut first = resizable_panel().size_range(self.min_size..Pixels::MAX);
        if let Some(size) = self.first_size {
            first = first.size(size);
        }
        if let Some(pane) = self.first {
            first = first.child(pane);
        }

        let mut second = resizable_panel().size_range(self.min_size..Pixels::MAX);
        if let Some(pane) = self.second {
            second = second.child(pane);
        }

        let element = div().size_full().child(group.child(first).child(second));
        gpui_kit::base::StyledExt::refine_style(element, &self.style)
    }
}

impl_into_element!(Splitter);

#[cfg(test)]
mod tests {
    use super::{Splitter, SplitterAxis, SplitterSizes};
    use gpui_kit::px;

    #[test]
    fn sizes_survive_a_round_trip_through_plain_numbers() {
        let sizes = SplitterSizes::from_pixels(&[px(200.), px(300.)]);
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes.get(0), Some(200.));
        assert_eq!(sizes.get(1), Some(300.));
        assert_eq!(sizes.get(2), None);
        assert_eq!(sizes.as_slice(), &[200., 300.]);
        assert_eq!(sizes.to_pixels(), vec![px(200.), px(300.)]);
    }

    #[test]
    fn empty_sizes_are_empty() {
        let sizes = SplitterSizes::default();
        assert!(sizes.is_empty());
        assert_eq!(sizes.len(), 0);
        assert!(sizes.to_pixels().is_empty());
    }

    #[test]
    fn a_new_splitter_is_a_horizontal_split_with_two_empty_panes() {
        let splitter = Splitter::new("workspace");
        assert_eq!(splitter.axis, SplitterAxis::Horizontal);
        assert!(splitter.first.is_none());
        assert!(splitter.second.is_none());
        assert_eq!(splitter.min_size, px(80.));
    }

    #[test]
    fn the_axis_can_be_changed() {
        assert_eq!(
            Splitter::new("workspace").axis(SplitterAxis::Vertical).axis,
            SplitterAxis::Vertical
        );
    }
}
