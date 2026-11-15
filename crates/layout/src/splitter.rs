//! Panes with a divider between them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::{ResizablePanelEvent, ResizableState};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, Global, InteractiveElement as _, ParentElement as _,
    Pixels, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

use crate::gpui::IntoElement;

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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// The sizes of the splitters of an application.
///
/// A splitter that is given a [`persist_key`](Splitter::persist_key) stores
/// its sizes here and picks them up again the next time it is built. The
/// store belongs to the application, not to a window, so the sizes survive a
/// window being closed.
#[derive(Default)]
pub struct SplitterStore {
    sizes: Mutex<HashMap<String, SplitterSizes>>,
}

impl SplitterStore {
    /// The sizes stored under a key.
    pub fn get(&self, key: &str) -> Option<SplitterSizes> {
        let sizes = self.sizes.lock().ok()?;
        sizes.get(key).cloned()
    }

    /// Stores the sizes under a key.
    pub fn set(&self, key: &str, sizes: SplitterSizes) {
        if let Ok(mut stored) = self.sizes.lock() {
            stored.insert(key.to_string(), sizes);
        }
    }

    /// The keys the store holds.
    pub fn keys(&self) -> Vec<String> {
        let Ok(sizes) = self.sizes.lock() else {
            return Vec::new();
        };
        let mut keys: Vec<String> = sizes.keys().cloned().collect();
        keys.sort();
        keys
    }
}

impl Global for SplitterStore {}

/// The store the splitter sizes live in.
///
/// The store is installed by [`init`](crate::init). An application that has
/// not installed one still gets a store: the sizes are then kept for as long
/// as the application runs.
pub fn splitter_store(cx: &App) -> &SplitterStore {
    cx.try_global::<SplitterStore>().unwrap_or(&EMPTY_STORE)
}

/// The store used when the application has not installed one.
static EMPTY_STORE: LazyLock<SplitterStore> = LazyLock::new(SplitterStore::default);

/// Panes with a divider the user can drag.
///
/// The splitter is a control: the pane sizes live in a [`SplitterState`] the
/// application owns, so they survive a restart. Read them with
/// [`SplitterState::sizes`], turn them into plain numbers with
/// [`SplitterSizes`], and hand the state back when the application starts.
///
/// A splitter can hold any number of panes. Give it a
/// [`persist_key`](Splitter::persist_key) and the sizes are kept between
/// runs.
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
    panes: Vec<AnyElement>,
    sizes: Vec<Option<Pixels>>,
    min_size: Pixels,
    max_size: Option<Pixels>,
    on_resize: Option<ResizeCallback>,
    persist_key: Option<String>,
    /// The sizes as they were last reported.
    ///
    /// The buffer is reused, so a drag does not allocate a vector per event.
    reported: Rc<RefCell<Vec<Pixels>>>,
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
            panes: Vec::new(),
            sizes: Vec::new(),
            min_size: px(crate::theme::panel_min_size()),
            max_size: None,
            on_resize: None,
            persist_key: None,
            reported: Rc::new(RefCell::new(Vec::new())),
            style: StyleRefinement::default(),
        }
    }

    /// Which way the panes are divided.
    pub fn axis(mut self, axis: SplitterAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Adds a pane.
    pub fn pane(mut self, pane: impl gpui_kit::IntoElement) -> Self {
        self.panes.push(pane.into_any_element());
        self
    }

    /// Adds several panes.
    pub fn panes(mut self, panes: impl IntoIterator<Item = impl gpui_kit::IntoElement>) -> Self {
        self.panes
            .extend(panes.into_iter().map(|pane| pane.into_any_element()));
        self
    }

    /// The first pane.
    pub fn first(mut self, pane: impl gpui_kit::IntoElement) -> Self {
        self.set_pane(0, pane.into_any_element());
        self
    }

    /// The second pane.
    pub fn second(mut self, pane: impl gpui_kit::IntoElement) -> Self {
        self.set_pane(1, pane.into_any_element());
        self
    }

    fn set_pane(&mut self, index: usize, pane: AnyElement) {
        while self.panes.len() < index {
            self.panes.push(div().into_any_element());
        }
        if self.panes.len() == index {
            self.panes.push(pane);
        } else {
            self.panes[index] = pane;
        }
    }

    /// The size the first pane starts at.
    pub fn first_size(self, size: impl Into<Pixels>) -> Self {
        self.pane_size(0, size)
    }

    /// The size a pane starts at.
    pub fn pane_size(mut self, index: usize, size: impl Into<Pixels>) -> Self {
        if self.sizes.len() <= index {
            self.sizes.resize(index + 1, None);
        }
        self.sizes[index] = Some(size.into());
        self
    }

    /// The narrowest a pane may become.
    pub fn min_size(mut self, size: impl Into<Pixels>) -> Self {
        self.min_size = size.into();
        self
    }

    /// The widest a pane may become.
    pub fn max_size(mut self, size: impl Into<Pixels>) -> Self {
        self.max_size = Some(size.into());
        self
    }

    /// The key the sizes of this splitter are kept under.
    ///
    /// A splitter with a key takes the sizes it had the last time it was
    /// built, and stores them again when the user drags a divider.
    pub fn persist_key(mut self, key: impl Into<String>) -> Self {
        self.persist_key = Some(key.into());
        self
    }

    /// Called whenever the panes are resized.
    ///
    /// The callback is not called when a drag leaves the sizes where they
    /// were.
    pub fn on_resize(
        mut self,
        on_resize: impl Fn(&[Pixels], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_resize = Some(Rc::new(on_resize));
        self
    }

    /// The range a pane may take.
    fn size_range(&self) -> std::ops::Range<Pixels> {
        let min = self.min_size;
        let max = self.max_size.unwrap_or(Pixels::MAX);
        debug_assert!(min <= max, "a pane cannot be narrower than it is wide");
        min..max.max(min)
    }
}

impl gpui_kit::Styled for Splitter {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Splitter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        let mut group = match self.axis {
            SplitterAxis::Horizontal => h_resizable(self.id.clone()),
            SplitterAxis::Vertical => v_resizable(self.id.clone()),
        };

        group = group.with_state(&self.state);

        // The sizes the panes start at: the ones the caller gave, or the ones
        // that were stored under the splitter's key.
        let stored = self
            .persist_key
            .as_deref()
            .and_then(|key| splitter_store(cx).get(key));
        let sizes: Vec<Option<Pixels>> = (0..self.panes.len())
            .map(|index| {
                self.sizes
                    .get(index)
                    .copied()
                    .flatten()
                    .or_else(|| stored.as_ref().and_then(|sizes| sizes.get(index)).map(px))
            })
            .collect();

        if let Some(on_resize) = &self.on_resize {
            let on_resize = on_resize.clone();
            let reported = self.reported.clone();
            let key = self.persist_key.clone();
            group = group.on_resize(move |state, window, cx| {
                let mut buffer = reported.borrow_mut();
                if state.read(cx).sizes().as_slice() == buffer.as_slice() {
                    // The drag left the sizes where they were: nothing to report.
                    return;
                }
                buffer.clear();
                buffer.extend_from_slice(state.read(cx).sizes());
                drop(buffer);

                let sizes = reported.borrow();
                if let Some(key) = &key {
                    splitter_store(cx).set(key, SplitterSizes::from_pixels(&sizes));
                }
                on_resize(&sizes, window, cx);
            });
        }

        let range = self.size_range();
        for (index, pane) in self.panes.into_iter().enumerate() {
            let mut panel = resizable_panel().size_range(range.clone());
            if let Some(size) = sizes.get(index).copied().flatten() {
                panel = panel.size(size);
            }
            group = group.child(panel.child(pane));
        }

        // The sizes the splitter had before the drag: they are what a drag
        // that changes nothing is measured against.
        let reported = self.reported.clone();
        let state = self.state.clone();
        let element = div()
            .id(self.id.clone())
            .size_full()
            .on_mouse_down(gpui_kit::MouseButton::Left, move |_, _, cx| {
                let mut buffer = reported.borrow_mut();
                buffer.clear();
                buffer.extend_from_slice(state.read(cx).sizes());
            })
            .child(group);
        gpui_kit::base::StyledExt::refine_style(element, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{Splitter, SplitterAxis, SplitterSizes, SplitterStore, splitter_store};
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
    fn the_sizes_can_be_stored_as_json() {
        let sizes = SplitterSizes::from_pixels(&[px(200.), px(300.)]);
        let json = serde_json::to_string(&sizes).expect("the sizes serialize");
        let read: SplitterSizes = serde_json::from_str(&json).expect("the sizes read back");
        assert_eq!(read, sizes);
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
        assert!(splitter.panes.is_empty());
        assert!(splitter.min_size > px(0.));
        assert_eq!(
            splitter.min_size,
            px(crate::theme::panel_min_size()),
            "the narrowest a pane may become comes from the theme"
        );
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

    #[test]
    fn a_splitter_can_hold_any_number_of_panes() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(|cx| cx.new(|_| super::SplitterState::default()));
        let splitter = Splitter::new(&state).pane("one").pane("two").pane("three");
        assert_eq!(splitter.panes.len(), 3);
    }

    #[test]
    #[should_panic(expected = "a pane cannot be narrower than it is wide")]
    fn a_pane_that_is_narrower_than_it_is_wide_is_noticed() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(|cx| cx.new(|_| super::SplitterState::default()));
        let splitter = Splitter::new(&state).min_size(px(200.)).max_size(px(100.));
        let _ = splitter.size_range();
    }

    #[test]
    fn the_store_keeps_the_sizes_of_a_key() {
        let store = SplitterStore::default();
        assert!(store.get("sidebar").is_none());
        store.set("sidebar", SplitterSizes::from_pixels(&[px(120.)]));
        assert_eq!(
            store.get("sidebar").map(|sizes| sizes.get(0)),
            Some(Some(120.))
        );
        assert_eq!(store.keys(), vec!["sidebar".to_string()]);
    }

    #[test]
    fn an_application_without_a_store_still_has_one() {
        let cx = gpui_kit::TestAppContext::single();
        cx.update(|cx| {
            let store = splitter_store(cx);
            assert!(store.get("sidebar").is_none());
        });
    }
}
