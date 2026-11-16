//! SegmentedControl: a Control (state entity + element) whose active indicator slides with
//! a spring. Only the control's own entity re-renders while it animates (conventions §6.5),
//! and it is keyboard-operable (←/→/Home/End when focused).

use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyBinding, ParentElement, Pixels, Render, SharedString, Window, actions, canvas, div,
    prelude::*, px,
};
use guiraffe_core::Size;
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
        self.select(self.selected.saturating_sub(1), cx);
    }
    fn on_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.select(self.selected + 1, cx);
    }
    fn on_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select(0, cx);
    }
    fn on_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        self.select(self.items.len().saturating_sub(1), cx);
    }

    /// Called once per prepaint, after all segments recorded their boxes.
    fn sync_indicator(&mut self, container: Bounds<Pixels>, cx: &mut Context<Self>) {
        let target = indicator_box(
            &self.layout.borrow().segments,
            f32::from(container.origin.x),
            self.selected,
        );
        let Some(target) = target else { return };
        if !self.laid_out {
            self.laid_out = true;
            self.indicator.jump(target);
            cx.notify();
        } else if self.indicator.set(target, cx) {
            cx.notify();
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
        let focused = self.focus.is_focused(window);
        let entity = cx.entity();
        div()
            .id("segmented")
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_first))
            .on_action(cx.listener(Self::on_last))
            .relative()
            .flex()
            .p(px(pad))
            .rounded(r)
            .bg(hsla(s.track))
            .when(focused, |el| {
                el.border_1().border_color(hsla(
                    guiraffe_core::GuiraffeTheme::global(cx).primary_color(),
                ))
            })
            .when(self.full_width, |el| el.w_full())
            .when(self.laid_out && !self.items.is_empty(), |el| {
                el.child(
                    div()
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
                div()
                    .id(("segment", i))
                    .relative()
                    .flex_1()
                    .px(px(size.spacing()))
                    .h(px(seg_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(size.font_size()))
                    .text_color(hsla(if i == selected { s.text } else { s.dimmed }))
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.select(i, cx);
                    }))
            }))
            // Last child: prepaints after every segment, so the target is computed once per frame.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, cx| this.sync_indicator(bounds, cx))
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
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
