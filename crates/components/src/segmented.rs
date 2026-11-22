//! SegmentedControl: a Control (state entity + element) whose active indicator slides with
//! a spring. Only the control's own entity re-renders while it animates (conventions §6.5).
//!
//! Interaction (API conventions §12): a pointer press selects without taking keyboard focus; the
//! control is a tab stop only inside [`guiraffe_core::focus_scope`] or when opted in with
//! [`SegmentedControl::set_focusable`], and then ←/→/Home/End move the selection and a
//! focus-visible ring is drawn. AccessKit: `RadioGroup` of `RadioButton`s with selected state.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyBinding, MouseButton, ParentElement, Pixels, Render, Role, SharedString, TestSupportExt,
    Window, actions, canvas, div, prelude::*, px,
};
use guiraffe_core::{A11yDisabledExt, ColorName, GuiraffeTheme, Size, StateKind, Variant};
use guiraffe_motion::{Animated, Spring, Timing};

use crate::style::{hsla, metrics, radius, shadows, surfaces};

actions!(
    guiraffe_segmented,
    [SelectPrev, SelectNext, SelectFirst, SelectLast]
);

const CONTEXT: &str = "GrSegmentedControl";

/// Register keyboard bindings (called from [`crate::init`]).
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("right", SelectNext, Some(CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(CONTEXT)),
    ]);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentedEvent {
    Changed(usize),
}

/// Per-frame layout measurements, written during prepaint (never notifies by itself).
#[derive(Default)]
struct Layout {
    /// Window-space (x, width) of each segment.
    segments: Vec<(f32, f32)>,
}

pub struct SegmentedControl {
    items: Vec<SharedString>,
    selected: usize,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    full_width: bool,
    disabled: bool,
    /// `None`: a tab stop only inside a focus scope.
    focusable: Option<bool>,
    focus: FocusHandle,
    /// Indicator (left, width) relative to the control's border box.
    indicator: Animated<(f32, f32)>,
    laid_out: bool,
    layout: Rc<RefCell<Layout>>,
}

impl EventEmitter<SegmentedEvent> for SegmentedControl {}

impl Focusable for SegmentedControl {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Indicator `(left, width)` for segment `index`, relative to a container at `origin_x`.
pub fn indicator_box(segments: &[(f32, f32)], origin_x: f32, index: usize) -> Option<(f32, f32)> {
    segments
        .get(index)
        .map(|(x, w)| (x - origin_x, *w))
        .filter(|(_, w)| *w > 0.0)
}

impl SegmentedControl {
    pub fn new(
        items: impl IntoIterator<Item = impl Into<SharedString>>,
        cx: &mut App,
    ) -> Entity<Self> {
        let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
        cx.new(|cx| Self {
            items,
            selected: 0,
            size: None,
            full_width: false,
            disabled: false,
            focusable: None,
            focus: cx.focus_handle(),
            indicator: Animated::new((0.0, 0.0), Timing::Spring(Spring::SNAPPY)),
            laid_out: false,
            layout: Rc::default(),
        })
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn items(&self) -> &[SharedString] {
        &self.items
    }

    /// Replace the items, keeping the selection in range.
    pub fn set_items(
        &mut self,
        items: impl IntoIterator<Item = impl Into<SharedString>>,
        cx: &mut Context<Self>,
    ) {
        self.items = items.into_iter().map(Into::into).collect();
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.laid_out = false;
        cx.notify();
    }

    pub fn set_size(&mut self, size: Option<Size>, cx: &mut Context<Self>) {
        if self.size != size {
            self.size = size;
            cx.notify();
        }
    }

    pub fn set_full_width(&mut self, v: bool, cx: &mut Context<Self>) {
        if self.full_width != v {
            self.full_width = v;
            cx.notify();
        }
    }

    /// Disabled: no pointer or keyboard selection, disabled tokens, `not-allowed` cursor.
    pub fn set_disabled(&mut self, v: bool, cx: &mut Context<Self>) {
        if self.disabled != v {
            self.disabled = v;
            cx.notify();
        }
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Opt into (or out of) Tab navigation regardless of the enclosing focus scope.
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        if self.focusable != v {
            self.focusable = v;
            cx.notify();
        }
    }

    /// Select segment `index` (clamped). Emits [`SegmentedEvent::Changed`] only on change.
    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        let index = index.min(self.items.len() - 1);
        if index == self.selected {
            return;
        }
        self.selected = index;
        cx.emit(SegmentedEvent::Changed(index));
        cx.notify(); // the indicator target is resolved from layout on the next prepaint
    }

    fn on_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.select(self.selected.saturating_sub(1), cx);
    }
    fn on_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.select(self.selected + 1, cx);
    }
    fn on_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.select(0, cx);
    }
    fn on_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.select(self.items.len().saturating_sub(1), cx);
    }

    /// Called once per prepaint, after all segments recorded their boxes.
    fn sync_indicator(&mut self, container: Bounds<Pixels>, cx: &mut Context<Self>) -> bool {
        let target = indicator_box(
            &self.layout.borrow().segments,
            f32::from(container.origin.x),
            self.selected,
        );
        let Some(target) = target else { return false };
        if !self.laid_out {
            self.laid_out = true;
            self.indicator.jump(target);
            true
        } else {
            self.indicator.set(target, cx)
        }
    }
}

impl Render for SegmentedControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = surfaces(cx);
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let pad = metrics::track_pad(size);
        let seg_h = size.control_height() - 2.0 * pad;
        let r = radius(None, cx);
        let (left, width) = self.indicator.sample(window, cx);
        {
            let mut l = self.layout.borrow_mut();
            l.segments.resize(self.items.len(), (0.0, 0.0));
        }
        let selected = self.selected;
        let disabled = self.disabled;
        let theme = GuiraffeTheme::global(cx);
        let ring = theme
            .state(StateKind::FocusRing, Variant::Default, None)
            .border;
        let off = theme.state(StateKind::Disabled, Variant::Default, None);
        let quiet = theme.press_colors(Variant::Subtle, Some(ColorName::Gray));
        let tab = !disabled && guiraffe_core::tab_stop(self.focusable);
        let ring_on = guiraffe_core::ring_visible(&self.focus, window);
        let count = self.items.len();
        let entity = cx.entity();
        div()
            .id("segmented")
            .a11y_disabled(disabled)
            .test_support()
            .role(Role::RadioGroup)
            .track_focus(&self.focus.clone().tab_stop(tab))
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_first))
            .on_action(cx.listener(Self::on_last))
            .relative()
            .flex()
            .p(px(pad))
            .rounded(r)
            .bg(hsla(if disabled { off.background } else { s.track }))
            .when(self.full_width, |el| el.w_full())
            .when(self.laid_out && !self.items.is_empty(), |el| {
                el.child(
                    div()
                        .debug_selector(|| "segment-indicator".into())
                        .absolute()
                        .top(px(pad))
                        .left(px(left))
                        .w(px(width))
                        .h(px(seg_h))
                        .rounded(r)
                        .bg(hsla(s.raised))
                        .shadow(shadows(Size::Xs, cx)),
                )
            })
            .children(self.items.iter().enumerate().map(|(i, label)| {
                let layout = self.layout.clone();
                let on = i == selected;
                div()
                    .id(("segment", i))
                    .a11y_disabled(disabled)
                    .test_support()
                    .role(Role::RadioButton)
                    .aria_label(label.clone())
                    .aria_selected(on)
                    .aria_toggled(if on {
                        gpui_kit::Toggled::True
                    } else {
                        gpui_kit::Toggled::False
                    })
                    .aria_position_in_set(i + 1)
                    .aria_size_of_set(count)
                    .relative()
                    .flex_1()
                    .px(px(size.spacing()))
                    .h(px(seg_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(r)
                    .text_size(px(size.font_size()))
                    .map(|el| {
                        if disabled {
                            el.cursor_not_allowed().text_color(hsla(off.foreground))
                        } else {
                            let (hover, pressed) = (quiet.hover.background, quiet.pressed);
                            el.cursor_pointer()
                                .text_color(hsla(if on { s.text } else { s.dimmed }))
                                // The selected segment sits on the raised indicator: no tint.
                                .when(!on, |el| {
                                    el.hover(move |st| st.text_color(hsla(s.text)).bg(hsla(hover)))
                                        .active(move |st| st.bg(hsla(pressed.background)))
                                })
                        }
                    })
                    .child(label.clone())
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                if let Some(slot) = layout.borrow_mut().segments.get_mut(i) {
                                    *slot =
                                        (f32::from(bounds.origin.x), f32::from(bounds.size.width));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    // A pointer press selects but never takes keyboard focus (§12.3).
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .when(!disabled, |el| {
                        el.on_click(cx.listener(move |this, _, _, cx| this.select(i, cx)))
                    })
            }))
            // Last child: prepaints after every segment, so the target is computed once per frame.
            .child(
                canvas(
                    move |bounds, window, cx| {
                        if entity.update(cx, |this, cx| this.sync_indicator(bounds, cx)) {
                            // Retargeting happens after render sampled the old spring. Notify
                            // on the next frame so paint-time invalidation cannot be consumed
                            // by this frame, and sample() can start driving the animation.
                            let id = entity.entity_id();
                            window.on_next_frame(move |_, cx| cx.notify(id));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            // Last: the ring paints over the segments, outside the box (§12.2).
            .map(|el| guiraffe_core::focus_ring(el, ring_on, r, ring))
    }
}

#[cfg(test)]
mod tests {
    use super::indicator_box;

    #[test]
    fn indicator_box_is_relative_to_the_container() {
        let segs = [(104.0, 50.0), (154.0, 70.0), (224.0, 40.0)];
        assert_eq!(indicator_box(&segs, 100.0, 0), Some((4.0, 50.0)));
        assert_eq!(indicator_box(&segs, 100.0, 2), Some((124.0, 40.0)));
        assert_eq!(indicator_box(&segs, 100.0, 3), None);
        assert_eq!(
            indicator_box(&[(0.0, 0.0)], 0.0, 0),
            None,
            "unmeasured segment"
        );
        assert_eq!(indicator_box(&[], 0.0, 0), None);
    }
}

pub(crate) mod document;
