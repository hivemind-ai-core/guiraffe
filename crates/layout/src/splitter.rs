//! Two panes with a divider between them.

use std::rc::Rc;

use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::{ResizablePanelEvent, ResizableState};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, ParentElement as _, Pixels, RenderOnce, StyleRefinement,
    Styled as _, Window, div, px,
};

use crate::gpui::IntoElement;
use crate::tokens::Size;

/// The state a [`Splitter`] keeps its pane sizes in.
///
/// The state is an entity, so it outlives a frame and an application can hold
/// on to it: read the sizes with [`SplitterState::sizes`] and put them back
/// when the application starts again.
pub type SplitterState = ResizableState;

/// The event a [`Splitter`] emits when its panes are resized.
pub type SplitterEvent = ResizablePanelEvent;

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
/// The splitter is a control: the pane sizes live in a [`SplitterState`] the
/// application owns, so they survive a restart. Read them with
/// [`SplitterState::sizes`], turn them into plain numbers with
/// [`SplitterSizes`], and hand the state back when the application starts.
///
/// ```
/// use guiraffe_layout::Splitter;
///
/// # fn build(state: &gpui_kit::Entity<guiraffe_layout::SplitterState>) {
/// let splitter = Splitter::new(state).first("sidebar").second("content");
/// # }
/// ```
#[derive(IntoElement)]
pub struct Splitter {
    id: ElementId,
    state: Entity<SplitterState>,
    axis: SplitterAxis,
    first: Option<AnyElement>,
    second: Option<AnyElement>,
    first_size: Option<Pixels>,
    min_size: Pixels,
    on_resize: Option<ResizeCallback>,
    style: StyleRefinement,
}

/// The callback a splitter reports its new pane sizes to.
type ResizeCallback = Rc<dyn Fn(&[Pixels], &mut Window, &mut App)>;

impl Splitter {
    /// A splitter whose pane sizes live in `state`.
    pub fn new(state: &Entity<SplitterState>) -> Self {
        Self {
            id: ElementId::View(state.entity_id()),
            state: state.clone(),
            axis: SplitterAxis::Horizontal,
            first: None,
            second: None,
            first_size: None,
            min_size: px(Size::Sm.control_height() * 2.),
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

    /// Called whenever the panes are resized.
    pub fn on_resize(
        mut self,
        on_resize: impl Fn(&[Pixels], &mut Window, &mut App) + 'static,
    ) -> Self {
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

        group = group.with_state(&self.state);
        if let Some(on_resize) = &self.on_resize {
            let on_resize = on_resize.clone();
            group = group.on_resize(move |state, window, cx| {
                let sizes = state.read(cx).sizes().to_vec();
                on_resize(&sizes, window, cx);
            });
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

#[cfg(test)]
mod tests {
    use super::{Splitter, SplitterAxis, SplitterSizes};
    use gpui_kit::{AppContext as _, px};

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
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(|cx| cx.new(|_| super::SplitterState::default()));
        let splitter = Splitter::new(&state);
        assert_eq!(splitter.axis, SplitterAxis::Horizontal);
        assert!(splitter.first.is_none());
        assert!(splitter.second.is_none());
        assert!(splitter.min_size > px(0.));
    }

    #[test]
    fn the_axis_can_be_changed() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(|cx| cx.new(|_| super::SplitterState::default()));
        assert_eq!(
            Splitter::new(&state).axis(SplitterAxis::Vertical).axis,
            SplitterAxis::Vertical
        );
    }
}
