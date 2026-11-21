//! GPUI elements for opt-in layout animation (spec/29): [`AnimatedSize`], [`AnimatedList`]
//! and [`Reveal`].
//!
//! Each element keeps its state in GPUI element state (keyed by its id path) and requests an
//! animation frame for the view that renders it only while something moves. Disabled, or under
//! reduced motion, each one is a plain pass-through layout that touches no state.

use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement, Pixels, Style, Styled, Window, div,
    px, relative, size,
};

use crate::{
    layout::{FlipList, SIZE_EPSILON, SizeTrack, keys_hash},
    timing::{MotionPrefs, Timing},
};

/// Which extents an [`AnimatedSize`] interpolates. The other axis fills the parent (100 %).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeAxis {
    /// Height animates; width fills the parent. Lists, groups, accordions.
    #[default]
    Vertical,
    /// Width animates; height fills the parent. Side panels.
    Horizontal,
    /// Both animate, measured at max-content.
    Both,
}

/// Interpolate a container's size from its child's old laid-out size to the new one.
///
/// ```ignore
/// // In the render of the (small) view that owns `items`:
/// animate_size("todo", Stack::new().children(rows)).enabled(self.animate)
/// ```
pub struct AnimatedSize {
    id: ElementId,
    child: AnyElement,
    axis: SizeAxis,
    enabled: Option<bool>,
    timing: Option<Timing>,
    clip: bool,
}

/// Shorthand for [`AnimatedSize::new`] (vertical, following `MotionPrefs::layout`).
pub fn animate_size(id: impl Into<ElementId>, child: impl IntoElement) -> AnimatedSize {
    AnimatedSize::new(id, child)
}

impl AnimatedSize {
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            axis: SizeAxis::Vertical,
            enabled: None,
            timing: None,
            clip: true,
        }
    }

    pub fn axis(mut self, axis: SizeAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Per-container override of `MotionPrefs::layout.enabled`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Like [`Self::enabled`], keeping `None` as "follow the global setting".
    pub fn enabled_opt(mut self, enabled: Option<bool>) -> Self {
        self.enabled = enabled;
        self
    }

    /// Per-container override of `MotionPrefs::layout.timing`.
    pub fn timing(mut self, timing: Timing) -> Self {
        self.timing = Some(timing);
        self
    }

    /// Clip the child to the animated size (default true).
    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    fn animates_width(&self) -> bool {
        matches!(self.axis, SizeAxis::Horizontal | SizeAxis::Both)
    }

    fn animates_height(&self) -> bool {
        matches!(self.axis, SizeAxis::Vertical | SizeAxis::Both)
    }
}

#[derive(Debug, Clone)]
struct SizeState {
    width: SizeTrack,
    height: SizeTrack,
    measured: bool,
    /// The (width, height) laid out this frame, so prepaint knows whether layout lags the target.
    laid_out: (f32, f32),
}

#[doc(hidden)]
pub enum SizeMode {
    /// Disabled or reduced motion: the child is an ordinary layout child.
    PassThrough(LayoutId),
    /// First frame: laid out normally, then its size is recorded.
    First(LayoutId),
    /// Laid out at the animated size; the child is measured as a root in prepaint.
    Measured,
}

fn now(cx: &App) -> std::time::Instant {
    cx.background_executor().now()
}

impl IntoElement for AnimatedSize {
    type Element = Self;
    /// Wraps the child so that, when it is measured as a root, it fills the cross axis like
    /// an ordinary block child would (a root with an auto size shrinks to fit).
    fn into_element(self) -> Self {
        let Self {
            id,
            child,
            axis,
            enabled,
            timing,
            clip,
        } = self;
        let child = match axis {
            SizeAxis::Vertical => div().w_full().child(child).into_any_element(),
            SizeAxis::Horizontal => div().h_full().child(child).into_any_element(),
            SizeAxis::Both => child,
        };
        Self {
            id,
            child,
            axis,
            enabled,
            timing,
            clip,
        }
    }
}

impl Element for AnimatedSize {
    type RequestLayoutState = SizeMode;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, SizeMode) {
        let prefs = MotionPrefs::global(cx);
        let mut style = Style::default();
        match self.axis {
            SizeAxis::Vertical => style.size.width = relative(1.0).into(),
            SizeAxis::Horizontal => style.size.height = relative(1.0).into(),
            SizeAxis::Both => {}
        }
        let Some(global_id) = global_id.filter(|_| prefs.layout_enabled(self.enabled)) else {
            let child = self.child.request_layout(window, cx);
            return (
                window.request_layout(style, [child], cx),
                SizeMode::PassThrough(child),
            );
        };
        let timing = self.timing.unwrap_or(prefs.layout.timing);
        let t = now(cx);
        let (aw, ah) = (self.animates_width(), self.animates_height());
        let current = window.with_element_state(global_id, |state: Option<SizeState>, _| {
            let mut state = state.unwrap_or_else(|| SizeState {
                width: SizeTrack::new(0.0, timing),
                height: SizeTrack::new(0.0, timing),
                measured: false,
                laid_out: (0.0, 0.0),
            });
            state.width.set_timing(timing);
            state.height.set_timing(timing);
            let current = state.measured.then(|| {
                let w = state.width.value_at(t, &prefs).0;
                let h = state.height.value_at(t, &prefs).0;
                state.laid_out = (w, h);
                (w, h)
            });
            (current, state)
        });
        match current {
            None => {
                let child = self.child.request_layout(window, cx);
                (
                    window.request_layout(style, [child], cx),
                    SizeMode::First(child),
                )
            }
            Some((w, h)) => {
                if aw {
                    style.size.width = px(w).into();
                }
                if ah {
                    style.size.height = px(h).into();
                }
                (window.request_layout(style, None, cx), SizeMode::Measured)
            }
        }
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        mode: &mut SizeMode,
        window: &mut Window,
        cx: &mut App,
    ) {
        match *mode {
            SizeMode::PassThrough(_) => {
                self.child.prepaint(window, cx);
            }
            SizeMode::First(child) => {
                let measured = window.layout_bounds(child).size;
                if let Some(global_id) = global_id {
                    window.with_element_state(global_id, |state: Option<SizeState>, _| {
                        let mut state = state.unwrap_or_else(unreachable_state);
                        state.width.jump(f32::from(measured.width));
                        state.height.jump(f32::from(measured.height));
                        state.measured = true;
                        ((), state)
                    });
                }
                self.child.prepaint(window, cx);
            }
            SizeMode::Measured => {
                let available = match self.axis {
                    SizeAxis::Vertical => size(
                        AvailableSpace::Definite(bounds.size.width),
                        AvailableSpace::MinContent,
                    ),
                    SizeAxis::Horizontal => size(
                        AvailableSpace::MaxContent,
                        AvailableSpace::Definite(bounds.size.height),
                    ),
                    SizeAxis::Both => size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                };
                let measured = self.child.layout_as_root(available, window, cx);
                let prefs = MotionPrefs::global(cx);
                let t = now(cx);
                let (aw, ah) = (self.animates_width(), self.animates_height());
                let needs_frame = global_id.is_some_and(|global_id| {
                    window.with_element_state(global_id, |state: Option<SizeState>, _| {
                        let mut state = state.unwrap_or_else(unreachable_state);
                        let mut needs = false;
                        for (on, track, size, laid) in [
                            (
                                aw,
                                &mut state.width,
                                f32::from(measured.width),
                                state.laid_out.0,
                            ),
                            (
                                ah,
                                &mut state.height,
                                f32::from(measured.height),
                                state.laid_out.1,
                            ),
                        ] {
                            if !on {
                                continue;
                            }
                            track.observe(size, t, &prefs);
                            let (_, moving) = track.value_at(t, &prefs);
                            if !moving {
                                track.jump(track.target());
                            }
                            needs |= moving || (laid - track.target()).abs() > SIZE_EPSILON;
                        }
                        (needs, state)
                    })
                });
                if needs_frame {
                    window.request_animation_frame();
                }
                let origin = bounds.origin;
                if self.clip {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        self.child.prepaint_at(origin, window, cx);
                    });
                } else {
                    self.child.prepaint_at(origin, window, cx);
                }
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        mode: &mut SizeMode,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if matches!(mode, SizeMode::Measured) && self.clip {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                self.child.paint(window, cx);
            });
        } else {
            self.child.paint(window, cx);
        }
    }
}

/// Request layout always creates the state before prepaint reads it; this only guards the
/// type system (no panics in library code).
fn unreachable_state() -> SizeState {
    debug_assert!(false, "AnimatedSize state missing in prepaint");
    SizeState {
        width: SizeTrack::new(0.0, Timing::Spring(crate::spring::Spring::SNAPPY)),
        height: SizeTrack::new(0.0, Timing::Spring(crate::spring::Spring::SNAPPY)),
        measured: true,
        laid_out: (0.0, 0.0),
    }
}

/// A flex column of keyed children whose positions animate (FLIP) when the key sequence
/// changes: added, removed or reordered items slide from where they were.
///
/// ```ignore
/// AnimatedList::new("todo").gap(px(6.)).children(items.iter().map(|it| (it.id, row(it))))
/// ```
pub struct AnimatedList {
    id: ElementId,
    keys: Vec<ElementId>,
    children: Vec<AnyElement>,
    gap: Pixels,
    enabled: Option<bool>,
    timing: Option<Timing>,
}

impl AnimatedList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            keys: Vec::new(),
            children: Vec::new(),
            gap: px(0.),
            enabled: None,
            timing: None,
        }
    }

    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }

    /// Per-container override of `MotionPrefs::layout.enabled`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    pub fn enabled_opt(mut self, enabled: Option<bool>) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn timing(mut self, timing: Timing) -> Self {
        self.timing = Some(timing);
        self
    }

    /// Add a child identified by `key` (stable across reorders; unique in the list).
    pub fn keyed(mut self, key: impl Into<ElementId>, child: impl IntoElement) -> Self {
        self.keys.push(key.into());
        self.children.push(child.into_any_element());
        self
    }

    /// Add `(key, child)` pairs.
    pub fn children<K: Into<ElementId>, E: IntoElement>(
        mut self,
        items: impl IntoIterator<Item = (K, E)>,
    ) -> Self {
        for (k, c) in items {
            self = self.keyed(k, c);
        }
        self
    }
}

impl IntoElement for AnimatedList {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[doc(hidden)]
pub struct ListLayout {
    children: Vec<LayoutId>,
    animate: bool,
}

impl Element for AnimatedList {
    type RequestLayoutState = ListLayout;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ListLayout) {
        let animate = MotionPrefs::global(cx).layout_enabled(self.enabled);
        let children: Vec<LayoutId> = self
            .children
            .iter_mut()
            .map(|c| c.request_layout(window, cx))
            .collect();
        let mut style = Style {
            display: gpui_kit::Display::Flex,
            flex_direction: gpui_kit::FlexDirection::Column,
            ..Default::default()
        };
        style.gap.height = self.gap.into();
        let id = window.request_layout(style, children.iter().copied(), cx);
        (id, ListLayout { children, animate })
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut ListLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(global_id) = global_id.filter(|_| layout.animate) else {
            for child in &mut self.children {
                child.prepaint(window, cx);
            }
            return;
        };
        let prefs = MotionPrefs::global(cx);
        let timing = self.timing.unwrap_or(prefs.layout.timing);
        let t = now(cx);
        let hash = keys_hash(&self.keys);
        let (keys, children, ids) = (&self.keys, &mut self.children, &layout.children);
        let moving =
            window.with_element_state(global_id, |state: Option<FlipList<ElementId>>, window| {
                let mut state = state.unwrap_or_else(|| FlipList::new(timing));
                state.begin(hash, timing);
                let mut moving = false;
                for ((key, child), id) in keys.iter().zip(children.iter_mut()).zip(ids) {
                    let origin = window.layout_bounds(*id).origin - bounds.origin;
                    let (offset, m) = state.place(key, origin, t, &prefs);
                    moving |= m;
                    window.with_element_offset(offset, |window| {
                        child.prepaint(window, cx);
                    });
                }
                state.end();
                (moving, state)
            });
        if moving {
            window.request_animation_frame();
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut ListLayout,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        for child in &mut self.children {
            child.paint(window, cx);
        }
    }
}

/// A clipped vertical reveal: height is the child's measured height × `progress`, opacity is
/// `progress`. At `progress >= 1` it is a plain pass-through (no measuring, no clipping).
/// Drive `progress` from a [`crate::Presence`] (e.g. [`crate::KeyedPresence`]).
pub struct Reveal {
    id: ElementId,
    progress: f32,
    child: AnyElement,
}

/// Shorthand for [`Reveal::new`].
pub fn reveal(id: impl Into<ElementId>, progress: f32, child: impl IntoElement) -> Reveal {
    Reveal::new(id, progress, child)
}

impl Reveal {
    pub fn new(id: impl Into<ElementId>, progress: f32, child: impl IntoElement) -> Self {
        let progress = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let child = if progress < 1.0 {
            // A plain div adds no id to the path, so the child's state survives the switch.
            div()
                .w_full()
                .opacity(progress)
                .child(child)
                .into_any_element()
        } else {
            div().w_full().child(child).into_any_element()
        };
        Self {
            id: id.into(),
            progress,
            child,
        }
    }
}

impl IntoElement for Reveal {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RevealState {
    height: Option<f32>,
}

impl Element for Reveal {
    type RequestLayoutState = Option<LayoutId>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Option<LayoutId>) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        if self.progress >= 1.0 {
            let child = self.child.request_layout(window, cx);
            return (window.request_layout(style, [child], cx), Some(child));
        }
        let height = global_id.and_then(|global_id| {
            window.with_element_state(global_id, |state: Option<RevealState>, _| {
                let state = state.unwrap_or_default();
                (state.height, state)
            })
        });
        style.size.height = px(height.unwrap_or(0.0) * self.progress).into();
        (window.request_layout(style, None, cx), None)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Option<LayoutId>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let height = if let Some(child) = *child {
            let h = f32::from(window.layout_bounds(child).size.height);
            self.child.prepaint(window, cx);
            h
        } else {
            let measured = self.child.layout_as_root(
                size(
                    AvailableSpace::Definite(bounds.size.width),
                    AvailableSpace::MinContent,
                ),
                window,
                cx,
            );
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                self.child.prepaint_at(bounds.origin, window, cx);
            });
            f32::from(measured.height)
        };
        if let Some(global_id) = global_id {
            let changed = window.with_element_state(global_id, |state: Option<RevealState>, _| {
                let mut state = state.unwrap_or_default();
                let changed = state
                    .height
                    .is_none_or(|old| (old - height).abs() > SIZE_EPSILON);
                state.height = Some(height);
                (changed, state)
            });
            // A partially revealed child changed height: lay out again with the new height.
            if changed && child.is_none() {
                window.request_animation_frame();
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Option<LayoutId>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if child.is_some() {
            self.child.paint(window, cx);
        } else {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                self.child.paint(window, cx);
            });
        }
    }
}
