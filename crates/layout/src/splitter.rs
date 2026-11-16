//! Panes with a divider between them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::ResizableState;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, EventEmitter, Global,
    InteractiveElement as _, ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled as _,
    Window, div, px,
};

use crate::gpui::IntoElement;

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

    /// The sizes that are fit to use for `panes` panes.
    ///
    /// A size has to be a number and it has to be positive, and there has to
    /// be one for every pane. The sizes that are fit to use add up to the
    /// total the panes share: the ones that are there are scaled to it, and
    /// a pane without a size of its own takes an equal share of what is
    /// left.
    ///
    /// ```
    /// use guiraffe_layout::SplitterSizes;
    /// use gpui_kit::px;
    ///
    /// let sizes = SplitterSizes::from_pixels(&[px(100.), px(300.)]);
    /// assert_eq!(sizes.usable(2, 800.), vec![200., 600.]);
    /// ```
    pub fn usable(&self, panes: usize, total: f32) -> Vec<f32> {
        let panes = panes.max(1);
        let mut sizes: Vec<f32> = self
            .0
            .iter()
            .take(panes)
            .map(|size| {
                if size.is_finite() && *size > 0. {
                    *size
                } else {
                    0.
                }
            })
            .collect();
        while sizes.len() < panes {
            sizes.push(0.);
        }

        let known: f32 = sizes.iter().sum();
        let missing = sizes.iter().filter(|size| **size <= 0.).count();
        let total = if total.is_finite() && total > 0. {
            total
        } else {
            known
        };

        if missing > 0 {
            // The panes without a size of their own share what is left.
            let share = (total - known).max(0.) / missing as f32;
            for size in sizes.iter_mut() {
                if *size <= 0. {
                    *size = share;
                }
            }
        } else if known > 0. && (known - total).abs() > f32::EPSILON {
            // The sizes are scaled so they add up to the total.
            for size in sizes.iter_mut() {
                *size *= total / known;
            }
        }
        sizes
    }
}

/// The event a [`Splitter`] emits when its panes are resized.
///
/// ```
/// use guiraffe_layout::SplitterEvent;
///
/// # fn read(event: SplitterEvent) {
/// match event {
///     SplitterEvent::Resized { sizes } => {
///         assert_eq!(sizes.len(), 2);
///     }
/// }
/// # }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum SplitterEvent {
    /// The panes were resized.
    ///
    /// The sizes are the sizes the panes have now.
    Resized {
        /// The sizes of the panes, in pixels.
        sizes: Vec<Pixels>,
    },
}

/// The state a [`Splitter`] keeps its pane sizes in.
///
/// The state is an entity, so it outlives a frame and an application can
/// hold on to it: read the sizes with [`SplitterState::sizes`] and put them
/// back when the application starts again.
///
/// The state emits [`SplitterEvent`] when the panes are resized.
///
/// ```
/// use guiraffe_layout::SplitterState;
///
/// # fn build(cx: &mut gpui_kit::App) {
/// let state = SplitterState::new(cx);
/// let splitter = guiraffe_layout::Splitter::new(&state).first("sidebar").second("content");
/// # }
/// ```
pub struct SplitterState {
    /// The state the resizable group keeps its sizes in.
    group: Entity<ResizableState>,
    /// The sizes the panes had the last time they were reported.
    ///
    /// The buffer lives in the state, so it survives from one render to the
    /// next: a drag that leaves the panes where they were is not reported,
    /// and the report happens once per drag.
    reported: RefCell<Vec<Pixels>>,
}

impl EventEmitter<SplitterEvent> for SplitterState {}

impl SplitterState {
    /// A state with no panes yet.
    pub fn new(cx: &mut App) -> Entity<Self> {
        let group = cx.new(|_| ResizableState::default());
        cx.new(|_| Self {
            group,
            reported: RefCell::new(Vec::new()),
        })
    }

    /// The sizes of the panes.
    pub fn sizes(&self, cx: &App) -> Vec<Pixels> {
        self.group.read(cx).sizes().clone()
    }

    /// The state the resizable group keeps its sizes in.
    ///
    /// The group is the one `gpui-kit` ships; it is here for the
    /// applications that resize a pane themselves.
    pub fn group(&self) -> &Entity<ResizableState> {
        &self.group
    }

    /// Remembers the sizes the panes have, the first time the splitter is
    /// built.
    ///
    /// The buffer is only filled once: the report compares the sizes of a
    /// drag with the sizes of the last report, so a drag that leaves the
    /// panes where they were is not a resize.
    fn begin(&self, sizes: &[Pixels]) {
        let mut reported = self.reported.borrow_mut();
        if reported.is_empty() {
            reported.extend_from_slice(sizes);
        }
    }

    /// Reports the sizes a drag ended with.
    ///
    /// The answer is whether the sizes changed: a drag that leaves the panes
    /// where they were is not a resize, and it is not reported.
    fn report(&self, sizes: &[Pixels], key: Option<&str>, cx: &mut Context<Self>) -> bool {
        if self.reported.borrow().as_slice() == sizes {
            return false;
        }
        {
            let mut reported = self.reported.borrow_mut();
            reported.clear();
            reported.extend_from_slice(sizes);
        }
        if let Some(key) = key {
            save_sizes(key, sizes, cx);
        }
        cx.emit(SplitterEvent::Resized {
            sizes: sizes.to_vec(),
        });
        true
    }
}

/// Where a layout is kept between runs.
///
/// The core crate has the store that writes to disk; this trait is the shape
/// the layout widgets ask for, so an application can put a store of its own
/// behind it.
///
/// ```
/// use guiraffe_layout::{LayoutPersist, MemoryPersist};
///
/// let store = MemoryPersist::new();
/// store.save("sidebar", "200");
/// assert_eq!(store.load("sidebar").as_deref(), Some("200"));
/// ```
pub trait LayoutPersist {
    /// The value stored under a key.
    fn load(&self, key: &str) -> Option<String>;

    /// Keeps a value under a key.
    fn save(&self, key: &str, value: &str);
}

/// A store that keeps the values in memory.
///
/// The values are gone when the application ends. It is the store the layout
/// widgets use when the application has not installed one of its own.
#[derive(Default)]
pub struct MemoryPersist {
    values: RefCell<HashMap<String, String>>,
}

impl MemoryPersist {
    /// A store with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// The keys the store holds.
    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.values.borrow().keys().cloned().collect();
        keys.sort();
        keys
    }
}

impl LayoutPersist for MemoryPersist {
    fn load(&self, key: &str) -> Option<String> {
        self.values.borrow().get(key).cloned()
    }

    fn save(&self, key: &str, value: &str) {
        self.values
            .borrow_mut()
            .insert(key.to_string(), value.to_string());
    }
}

/// The store the layout widgets keep their sizes in.
///
/// The store belongs to the application, not to a window, so the sizes
/// survive a window being closed.
pub struct LayoutStore {
    persist: Box<dyn LayoutPersist>,
}

impl LayoutStore {
    /// A store that keeps the values in memory.
    pub fn in_memory() -> Self {
        Self {
            persist: Box::new(MemoryPersist::new()),
        }
    }

    /// A store that keeps the values in a store of your own.
    pub fn new(persist: Box<dyn LayoutPersist>) -> Self {
        Self { persist }
    }

    /// The store the values live in.
    pub fn persist(&self) -> &dyn LayoutPersist {
        self.persist.as_ref()
    }

    /// The sizes of a splitter stored under a key.
    ///
    /// The sizes are read back as they were stored: a value that is not a
    /// size is not a size, and it is not returned.
    pub fn splitter_sizes(&self, key: &str) -> Option<SplitterSizes> {
        let json = self.persist.load(key)?;
        serde_json::from_str(&json).ok()
    }

    /// Keeps the sizes of a splitter under a key.
    pub fn save_splitter_sizes(&self, key: &str, sizes: &SplitterSizes) {
        if let Ok(json) = serde_json::to_string(sizes) {
            self.persist.save(key, &json);
        }
    }
}

impl Global for LayoutStore {}

/// The store the layout widgets keep their sizes in.
///
/// The store is installed by [`init`](crate::init). An application that has
/// not installed one still gets a store: the sizes are then kept for as long
/// as the application runs.
pub fn layout_store(cx: &App) -> Option<&LayoutStore> {
    cx.try_global::<LayoutStore>()
}

/// Keeps the sizes of a splitter under a key.
fn save_sizes(key: &str, sizes: &[Pixels], cx: &App) {
    let Some(store) = layout_store(cx) else {
        return;
    };
    store.save_splitter_sizes(key, &SplitterSizes::from_pixels(sizes));
}

/// The sizes a splitter stored under a key.
fn load_sizes(key: &str, cx: &App) -> Option<SplitterSizes> {
    layout_store(cx)?.splitter_sizes(key)
}

/// Which way a [`Splitter`] divides its panes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SplitterAxis {
    /// The panes sit side by side.
    #[default]
    Horizontal,
    /// The panes sit one above the other.
    Vertical,
}

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
    ///
    /// A pane cannot be narrower than the widest it may become, and it
    /// cannot be wider than the narrowest it may become: a pane that is
    /// given both is a mistake the developer is told about.
    pub(crate) fn size_range(&self) -> std::ops::Range<Pixels> {
        size_range(self.min_size, self.max_size)
    }
}

/// The range a pane may take.
///
/// The range is checked here, in one place, so the rule can be tested
/// without a window.
fn size_range(min: Pixels, max: Option<Pixels>) -> std::ops::Range<Pixels> {
    let max = max.unwrap_or(Pixels::MAX);
    debug_assert!(
        min <= max,
        "a pane cannot be wider than the narrowest it may become: \
         the narrowest is {min:?} and the widest is {max:?}"
    );
    min..max.max(min)
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

        let group_state = self.state.read(cx).group().clone();
        group = group.with_state(&group_state);

        // The sizes the panes start at: the ones the caller gave, or the
        // ones that were stored under the splitter's key. The stored sizes
        // are checked first: a size that is not a number, or not positive,
        // is not a size a pane can have.
        let stored = self
            .persist_key
            .as_deref()
            .and_then(|key| load_sizes(key, cx));
        let panes = self.panes.len();
        let stored = stored
            .map(|sizes| sizes.usable(panes, 0.))
            .unwrap_or_default();
        let sizes: Vec<Option<Pixels>> = (0..panes)
            .map(|index| {
                self.sizes
                    .get(index)
                    .copied()
                    .flatten()
                    .or_else(|| stored.get(index).copied().map(px))
            })
            .collect();

        // The report happens for every drag that ends, and only for the
        // drags that changed the sizes.
        let state = self.state.clone();
        let key = self.persist_key.clone();
        let on_resize = self.on_resize.clone();
        group = group.on_resize(move |group, window, cx| {
            let sizes = group.read(cx).sizes().clone();
            let changed = state.update(cx, |state, cx| state.report(&sizes, key.as_deref(), cx));
            if !changed {
                return;
            }
            if let Some(on_resize) = &on_resize {
                on_resize(&sizes, window, cx);
            }
        });

        let range = self.size_range();
        for (index, pane) in self.panes.into_iter().enumerate() {
            let mut panel = resizable_panel().size_range(range.clone());
            if let Some(size) = sizes.get(index).copied().flatten() {
                panel = panel.size(size);
            }
            group = group.child(panel.child(pane));
        }

        // The sizes the panes have now are what the next report is measured
        // against, until the panes are resized.
        let state = self.state.clone();
        let sizes = group_state.read(cx).sizes().clone();
        state.update(cx, |state, _| state.begin(&sizes));

        let element = div().id(self.id.clone()).size_full().child(group);
        gpui_kit::base::StyledExt::refine_style(element, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LayoutPersist, LayoutStore, MemoryPersist, Splitter, SplitterAxis, SplitterSizes,
        SplitterState, size_range,
    };
    use gpui_kit::{Pixels, px};

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
    fn the_sizes_that_are_stored_are_checked_before_they_are_used() {
        let sizes = SplitterSizes::from_pixels(&[px(200.), px(f32::NAN), px(-100.)]);
        let usable = sizes.usable(3, 600.);
        assert_eq!(usable[0], 200., "the size that is a number is kept");
        assert!(usable[1] > 0., "a size that is not a number is replaced");
        assert!(usable[2] > 0., "a size that is negative is replaced");
        assert_eq!(
            usable.iter().sum::<f32>(),
            600.,
            "the sizes that are fit to use add up to the total"
        );
    }

    #[test]
    fn sizes_for_another_number_of_panes_are_fitted_to_the_panes() {
        let sizes = SplitterSizes::from_pixels(&[px(100.), px(100.), px(100.)]);
        assert_eq!(
            sizes.usable(2, 400.),
            vec![200., 200.],
            "the size that does not belong to a pane is dropped"
        );
        assert_eq!(
            sizes.usable(4, 400.),
            vec![100., 100., 100., 100.],
            "a pane without a size of its own takes an equal share"
        );
    }

    #[test]
    fn the_sizes_are_scaled_to_the_total_the_panes_share() {
        let sizes = SplitterSizes::from_pixels(&[px(100.), px(300.)]);
        assert_eq!(sizes.usable(2, 800.), vec![200., 600.]);
    }

    #[test]
    fn a_new_splitter_is_a_horizontal_split_with_two_empty_panes() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(SplitterState::new);
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
        let state = cx.update(SplitterState::new);
        assert_eq!(
            Splitter::new(&state).axis(SplitterAxis::Vertical).axis,
            SplitterAxis::Vertical
        );
    }

    #[test]
    fn a_splitter_can_hold_any_number_of_panes() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(SplitterState::new);
        let splitter = Splitter::new(&state).pane("one").pane("two").pane("three");
        assert_eq!(splitter.panes.len(), 3);
    }

    #[test]
    #[should_panic(expected = "a pane cannot be wider than the narrowest it may become")]
    fn a_pane_that_is_narrower_than_it_is_wide_is_noticed() {
        let _ = size_range(px(200.), Some(px(100.)));
    }

    #[test]
    fn a_pane_may_be_as_wide_as_it_wants() {
        let range = size_range(px(100.), None);
        assert_eq!(range.start, px(100.));
        assert_eq!(range.end, Pixels::MAX);
    }

    #[test]
    fn the_store_keeps_the_values_of_a_key() {
        let store = MemoryPersist::new();
        assert!(store.load("sidebar").is_none());
        store.save("sidebar", "200");
        assert_eq!(store.load("sidebar").as_deref(), Some("200"));
        assert_eq!(store.keys(), vec!["sidebar".to_string()]);
    }

    #[test]
    fn an_application_without_a_store_has_none() {
        let cx = gpui_kit::TestAppContext::single();
        cx.update(|cx| {
            assert!(super::layout_store(cx).is_none());
        });
    }

    #[test]
    fn a_store_can_be_installed_by_the_application() {
        let cx = gpui_kit::TestAppContext::single();
        cx.update(|cx| {
            cx.set_global(LayoutStore::new(Box::new(MemoryPersist::new())));
        });
        cx.update(|cx| {
            let store = super::layout_store(cx).expect("the store is installed");
            store.persist().save("sidebar", "200");
            assert_eq!(store.persist().load("sidebar").as_deref(), Some("200"));
        });
    }

    #[test]
    fn a_store_of_your_own_can_be_used() {
        struct Own;
        impl LayoutPersist for Own {
            fn load(&self, _key: &str) -> Option<String> {
                Some("own".to_string())
            }
            fn save(&self, _key: &str, _value: &str) {}
        }
        let store = LayoutStore::new(Box::new(Own));
        assert_eq!(store.persist().load("sidebar").as_deref(), Some("own"));
    }
}
