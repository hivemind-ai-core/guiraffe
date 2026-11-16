//! SegmentedControl: a Control (state entity + element) whose active indicator slides with
//! a spring. Only the control's own entity re-renders while it animates (conventions §6.5).

use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels, Render,
    SharedString, Styled, Window, canvas, div, prelude::*, px,
};
use guiraffe_core::{Size, Variant};
use guiraffe_motion::{Animated, Spring, Timing};

use crate::style::{hsla, radius, shadows, surfaces, theme, variant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentedEvent {
    Changed(usize),
}

pub struct SegmentedControl {
    items: Vec<SharedString>,
    selected: usize,
    size: Size,
    full_width: bool,
    /// Indicator left edge and width (px), animated toward the measured segment.
    indicator: Animated<(f32, f32)>,
    /// Measured (left, width) per segment, filled during paint.
    measured: Measured,
}

impl EventEmitter<SegmentedEvent> for SegmentedControl {}

impl SegmentedControl {
    pub fn new(
        items: impl IntoIterator<Item = impl Into<SharedString>>,
        cx: &mut App,
    ) -> Entity<Self> {
        let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
        cx.new(|_| Self {
            items,
            selected: 0,
            size: Size::Sm,
            full_width: false,
            indicator: Animated::new((0.0, 0.0), Timing::Spring(Spring::SNAPPY)),
            measured: Default::default(),
        })
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn items(&self) -> &[SharedString] {
        &self.items
    }

    pub fn set_size(&mut self, size: Size, cx: &mut Context<Self>) {
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
        let index = index.min(self.items.len().saturating_sub(1));
        if index == self.selected || self.items.is_empty() {
            return;
        }
        self.selected = index;
        if let Some(target) = indicator_box(&self.measured.borrow(), index) {
            self.indicator.set(target, cx);
        }
        cx.emit(SegmentedEvent::Changed(index));
        cx.notify();
    }
}

impl Render for SegmentedControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = surfaces(cx);
        let dark = theme(cx).scheme == guiraffe_core::Scheme::Dark;
        let track = if dark {
            s.surface
        } else {
            guiraffe_core::ColorName::Gray.shade(1)
        };
        let active_bg = if dark {
            variant(cx, Some(guiraffe_core::ColorName::Dark), Variant::Default).background
        } else {
            s.body
        };
        let (left, width) = self.indicator.sample(window, cx);
        let pad = 4.0;
        let h = self.size.control_height() - 2.0;
        let measured = self.measured.clone();
        let selected = self.selected;
        let entity = cx.entity();
        let r = radius(None, cx);
        div()
            .relative()
            .flex()
            .p(px(pad))
            .rounded(r)
            .bg(hsla(track))
            .when(self.full_width, |el| el.w_full())
            .child(
                div()
                    .absolute()
                    .top(px(pad))
                    .left(px(left))
                    .w(px(width))
                    .h(px(h - pad * 2.0 + 2.0))
                    .rounded(r)
                    .bg(hsla(active_bg))
                    .shadow(shadows(Size::Xs, cx)),
            )
            .children(self.items.iter().enumerate().map(|(i, label)| {
                let m = measured.clone();
                let e = entity.clone();
                let active = i == selected;
                div()
                    .id(("segment", i))
                    .relative()
                    .flex_1()
                    .px(px(self.size.spacing()))
                    .h(px(h - pad * 2.0 + 2.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(self.size.font_size()))
                    .text_color(hsla(if active { s.text } else { s.dimmed }))
                    .child(label.clone())
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                record(&m, i, bounds.origin.x, bounds.size.width, &e, selected, cx);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.select(i, cx)))
            }))
    }
}

/// Indicator `(left, width)` for segment `index`, relative to the control's padding box.
/// `measured` holds window-space `(x, width)` per segment.
pub fn indicator_box(measured: &[(f32, f32)], index: usize) -> Option<(f32, f32)> {
    const PAD: f32 = 4.0;
    let origin = measured.first()?.0;
    measured.get(index).map(|(x, w)| (x - origin + PAD, *w))
}

type Measured = std::rc::Rc<std::cell::RefCell<Vec<(f32, f32)>>>;

/// Record segment `i`'s laid-out box (during prepaint) and keep the indicator on the selected
/// segment: snap on first layout (no fly-in from zero), animate on later layout changes.
fn record(
    m: &Measured,
    i: usize,
    x: Pixels,
    w: Pixels,
    e: &Entity<SegmentedControl>,
    selected: usize,
    cx: &mut App,
) {
    {
        let mut v = m.borrow_mut();
        if v.len() <= i {
            v.resize(i + 1, (0.0, 0.0));
        }
        v[i] = (f32::from(x), f32::from(w));
    }
    let Some(target) = indicator_box(&m.borrow(), selected) else {
        return;
    };
    e.update(cx, |this, cx| {
        if this.indicator.target() != &target {
            if this.indicator.target() == &(0.0, 0.0) {
                this.indicator.jump(target);
            } else {
                this.indicator.set(target, cx);
            }
            cx.notify();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::indicator_box;

    #[test]
    fn indicator_box_is_relative_to_first_segment() {
        let m = [(100.0, 50.0), (150.0, 70.0), (220.0, 40.0)];
        assert_eq!(indicator_box(&m, 0), Some((4.0, 50.0)));
        assert_eq!(indicator_box(&m, 2), Some((124.0, 40.0)));
        assert_eq!(indicator_box(&m, 3), None);
        assert_eq!(indicator_box(&[], 0), None);
    }
}
