//! Dope sheet: one row per animated channel, keyframes as diamonds on the shared time axis.
//! Drag a key to retime it (frame-snapped when the format is `Frames`). Keys within a channel
//! stay sorted and never share a time.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Pixels, Render, Styled, Window, canvas, div, prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme};

use crate::axis::{Axis, TimeFormat};

pub const ROW_H: f32 = 24.0;
pub const NAME_W: f32 = 140.0;
pub const KEY_GRAB: f64 = 6.0;

/// A channel's key times, kept sorted and distinct.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Channel {
    pub name: String,
    keys: Vec<f64>,
}

impl Channel {
    pub fn new(name: impl Into<String>, mut keys: Vec<f64>) -> Self {
        keys.retain(|k| k.is_finite());
        keys.sort_by(f64::total_cmp);
        keys.dedup();
        Self {
            name: name.into(),
            keys,
        }
    }

    pub fn keys(&self) -> &[f64] {
        &self.keys
    }

    /// Retime key `i` to `t`; refuses collisions. Returns the key's new index.
    pub fn retime(&mut self, i: usize, t: f64) -> Option<usize> {
        if !t.is_finite()
            || i >= self.keys.len()
            || self.keys[i] == t
            || self.keys.iter().enumerate().any(|(j, k)| j != i && *k == t)
        {
            return None;
        }
        self.keys.remove(i);
        let at = self.keys.partition_point(|k| *k < t);
        self.keys.insert(at, t);
        Some(at)
    }
}

/// Snap `t` to whole frames when the format is frame-based.
pub fn snap_time(t: f64, format: TimeFormat) -> f64 {
    match format {
        TimeFormat::Frames(fps) if fps > 0.0 => (t * fps).round() / fps,
        _ => t,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DopeEvent {
    KeyRetimed { channel: usize, from: f64, to: f64 },
}

pub struct DopeSheet {
    channels: Vec<Channel>,
    axis: Axis,
    format: TimeFormat,
    drag: Option<(usize, usize)>,
    body: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<DopeEvent> for DopeSheet {}

impl DopeSheet {
    pub fn new(
        channels: Vec<Channel>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| Self {
            channels,
            axis: Axis::new(0.0, length.max(1e-3), 800.0),
            format,
            drag: None,
            body: Rc::default(),
        })
    }

    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    pub fn axis(&self) -> &Axis {
        &self.axis
    }

    pub fn press(&mut self, row: usize, x: f64) -> bool {
        let axis = self.axis;
        self.drag = self.channels.get(row).and_then(|c| {
            c.keys()
                .iter()
                .enumerate()
                .map(|(i, k)| (i, (axis.x(*k) - x).abs()))
                .filter(|(_, d)| *d <= KEY_GRAB)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| (row, i))
        });
        self.drag.is_some()
    }

    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some((row, i)) = self.drag else { return };
        let to = snap_time(self.axis.t(x), self.format).max(0.0);
        let Some(ch) = self.channels.get_mut(row) else {
            return;
        };
        let Some(&from) = ch.keys().get(i) else {
            return;
        };
        if let Some(ni) = ch.retime(i, to) {
            self.drag = Some((row, ni));
            cx.emit(DopeEvent::KeyRetimed {
                channel: row,
                from,
                to,
            });
            cx.notify();
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    fn local(&self, p: gpui_kit::Point<Pixels>) -> (f64, f32) {
        let b = self.body.get();
        (
            f64::from(f32::from(p.x - b.origin.x)),
            f32::from(p.y - b.origin.y),
        )
    }
}

impl Render for DopeSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let w = f64::from(f32::from(self.body.get().size.width));
        if w > 0.0 {
            self.axis.set_width(w);
        }
        let axis = self.axis;
        let line = s.border.to_hsla();
        let key_c = ColorName::Yellow.shade(6).to_hsla();
        let body = self.body.clone();
        div()
            .size_full()
            .flex()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(12.))
            .child(
                div()
                    .w(px(NAME_W))
                    .flex_none()
                    .border_r_1()
                    .border_color(line)
                    .children(self.channels.iter().map(|c| {
                        div()
                            .h(px(ROW_H))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(line)
                            .child(c.name.clone())
                    })),
            )
            .child(
                div()
                    .id("dope-body")
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, _, _| {
                            let (x, y) = this.local(e.position);
                            if y >= 0.0 {
                                this.press((y / ROW_H) as usize, x);
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                        if e.pressed_button == Some(MouseButton::Left) {
                            let (x, _) = this.local(e.position);
                            this.drag_to(x, cx);
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.release()),
                    )
                    .child(
                        canvas(move |b, _, _| body.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .children(self.channels.iter().enumerate().flat_map(|(r, c)| {
                        c.keys()
                            .iter()
                            .map(|k| {
                                div()
                                    .absolute()
                                    .top(px(r as f32 * ROW_H + ROW_H / 2.0 - 5.0))
                                    .left(px(axis.x(*k) as f32 - 5.0))
                                    .size(px(10.))
                                    .rounded(px(2.))
                                    .bg(key_c)
                            })
                            .collect::<Vec<_>>()
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_sort_dedup_and_refuse_collisions() {
        let mut c = Channel::new("x", vec![2.0, 1.0, 2.0, f64::NAN]);
        assert_eq!(c.keys(), &[1.0, 2.0]);
        assert_eq!(c.retime(0, 2.0), None, "collision");
        assert_eq!(c.retime(0, 3.0), Some(1), "reorders");
        assert_eq!(c.keys(), &[2.0, 3.0]);
        assert_eq!(c.retime(9, 1.0), None);
    }

    #[test]
    fn frames_snap() {
        assert_eq!(snap_time(0.51, TimeFormat::Frames(10.0)), 0.5);
        assert_eq!(snap_time(0.51, TimeFormat::Seconds), 0.51);
    }

    #[test]
    fn dragging_retimes_with_frame_snap_and_follows_reorder() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            DopeSheet::new(
                vec![Channel::new("pos", vec![0.5, 1.0])],
                TimeFormat::Frames(10.0),
                2.0,
                cx,
            )
        });
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &DopeEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            assert!(v.press(0, a.x(0.5)));
            v.drag_to(a.x(1.42), cx); // -> 1.4, now index 1
            v.drag_to(a.x(1.61), cx); // same key keeps moving -> 1.6
            v.release();
        });
        assert_eq!(
            view.read_with(&cx, |v, _| v.channels()[0].keys().to_vec()),
            vec![1.0, 1.6]
        );
        assert_eq!(events.borrow().len(), 2);
    }
}
