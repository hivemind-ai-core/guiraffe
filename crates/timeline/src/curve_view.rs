//! Curve editor: a sampled polyline of a [`Curve`] over the shared [`Axis`] (time) and a value
//! range, with draggable keys. Pure mapping (`ValueRange`, `pick_key`) is split out and tested.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, PathBuilder, Pixels, Render, Styled, Window, canvas, div, point,
    prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme};

use crate::{axis::Axis, curves::Curve};

/// Vertical value mapping: `hi` at y = 0, `lo` at y = height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueRange {
    pub lo: f64,
    pub hi: f64,
    pub height: f64,
}

impl ValueRange {
    pub fn y(&self, v: f64) -> f64 {
        let span = (self.hi - self.lo).max(f64::EPSILON);
        (self.hi - v) / span * self.height
    }

    pub fn v(&self, y: f64) -> f64 {
        let span = (self.hi - self.lo).max(f64::EPSILON);
        self.hi - y / self.height.max(f64::EPSILON) * span
    }
}

/// Pixel radius within which a press grabs a key.
pub const KEY_GRAB: f64 = 7.0;

/// Index of the nearest key within [`KEY_GRAB`] of pixel `(x, y)`.
pub fn pick_key(curve: &Curve, axis: &Axis, range: &ValueRange, x: f64, y: f64) -> Option<usize> {
    curve
        .keys()
        .iter()
        .enumerate()
        .map(|(i, k)| (i, (axis.x(k.t) - x).hypot(range.y(k.v) - y)))
        .filter(|(_, d)| *d <= KEY_GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[derive(Debug, Clone, PartialEq)]
pub enum CurveEvent {
    KeyMoved { index: usize, t: f64, v: f64 },
}

pub struct CurveView {
    curve: Curve,
    axis: Axis,
    range: ValueRange,
    drag: Option<usize>,
    body: Rc<Cell<Bounds<Pixels>>>,
    /// Range to fit once the real width is known.
    pending_fit: Option<(f64, f64)>,
}

impl EventEmitter<CurveEvent> for CurveView {}

impl CurveView {
    pub fn new(curve: Curve, t0: f64, t1: f64, lo: f64, hi: f64, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            curve,
            axis: Axis::new(t0, t1, 600.0),
            range: ValueRange {
                lo,
                hi,
                height: 300.0,
            },
            drag: None,
            body: Rc::default(),
            pending_fit: Some((t0, t1)),
        })
    }

    pub fn curve(&self) -> &Curve {
        &self.curve
    }

    pub fn axis(&self) -> &Axis {
        &self.axis
    }

    pub fn range(&self) -> &ValueRange {
        &self.range
    }

    pub fn press(&mut self, x: f64, y: f64) -> bool {
        self.drag = pick_key(&self.curve, &self.axis, &self.range, x, y);
        self.drag.is_some()
    }

    pub fn drag_to(&mut self, x: f64, y: f64, cx: &mut Context<Self>) {
        let Some(i) = self.drag else { return };
        let (t, v) = (self.axis.t(x), self.range.v(y));
        if self.curve.move_key(i, t, v) {
            if let Some(k) = self.curve.keys().get(i) {
                cx.emit(CurveEvent::KeyMoved {
                    index: i,
                    t: k.t,
                    v: k.v,
                });
            }
            cx.notify();
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    fn local(&self, p: gpui_kit::Point<Pixels>) -> (f64, f64) {
        let b = self.body.get();
        (
            f64::from(f32::from(p.x - b.origin.x)),
            f64::from(f32::from(p.y - b.origin.y)),
        )
    }
}

impl Render for CurveView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let b = self.body.get();
        let (w, h) = (
            f64::from(f32::from(b.size.width)),
            f64::from(f32::from(b.size.height)),
        );
        if w > 0.0 && h > 0.0 {
            self.axis.set_width(w);
            self.range.height = h;
            if let Some((a, b)) = self.pending_fit.take() {
                self.axis.fit(a, b, 0.02);
            }
        }
        let (axis, range, curve) = (self.axis, self.range, self.curve.clone());
        let stroke = ColorName::Blue.shade(6).to_hsla();
        let key_c = s.text.to_hsla();
        let zero = s.border.to_hsla();
        let body = self.body.clone();
        div()
            .id("curve-body")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(s.body.to_hsla())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    let (x, y) = this.local(e.position);
                    this.press(x, y);
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    let (x, y) = this.local(e.position);
                    this.drag_to(x, y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.release()),
            )
            .child(
                canvas(
                    move |b, window, _| crate::measure::record(&body, b, window),
                    move |b, _, window, _| {
                        let o = b.origin;
                        let at = |x: f64, y: f64| point(o.x + px(x as f32), o.y + px(y as f32));
                        let width = f64::from(f32::from(b.size.width));
                        // Zero line.
                        let mut z = PathBuilder::stroke(px(1.));
                        z.move_to(at(0.0, range.y(0.0)));
                        z.line_to(at(width, range.y(0.0)));
                        if let Ok(p) = z.build() {
                            window.paint_path(p, zero);
                        }
                        if curve.keys().is_empty() {
                            return;
                        }
                        // One sample per 2 px: cost bounded by width, not key count.
                        let mut pb = PathBuilder::stroke(px(1.5));
                        let n = (width / 2.0).ceil().max(1.0) as usize;
                        for i in 0..=n {
                            let x = i as f64 * 2.0;
                            let p = at(x, range.y(curve.eval(axis.t(x))));
                            if i == 0 { pb.move_to(p) } else { pb.line_to(p) }
                        }
                        if let Ok(p) = pb.build() {
                            window.paint_path(p, stroke);
                        }
                        for k in curve.keys() {
                            let c = at(axis.x(k.t), range.y(k.v));
                            let r = px(4.);
                            let mut d = PathBuilder::fill();
                            d.move_to(point(c.x, c.y - r));
                            d.line_to(point(c.x + r, c.y));
                            d.line_to(point(c.x, c.y + r));
                            d.line_to(point(c.x - r, c.y));
                            d.close();
                            if let Ok(p) = d.build() {
                                window.paint_path(p, key_c);
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::{Interp, Key};

    fn curve() -> Curve {
        let mut c = Curve::default();
        c.set(Key::new(0.0, 0.0, Interp::Linear));
        c.set(Key::new(1.0, 1.0, Interp::Linear));
        c.set(Key::new(2.0, 0.0, Interp::Linear));
        c
    }

    #[test]
    fn value_range_round_trips() {
        let r = ValueRange {
            lo: -1.0,
            hi: 1.0,
            height: 200.0,
        };
        assert_eq!(r.y(1.0), 0.0);
        assert_eq!(r.y(-1.0), 200.0);
        assert!((r.v(r.y(0.3)) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn pick_key_takes_nearest_within_radius() {
        let (c, a) = (curve(), Axis::new(0.0, 2.0, 200.0));
        let r = ValueRange {
            lo: 0.0,
            hi: 1.0,
            height: 100.0,
        };
        assert_eq!(pick_key(&c, &a, &r, 101.0, 2.0), Some(1));
        assert_eq!(pick_key(&c, &a, &r, 50.0, 50.0), None);
    }

    #[test]
    fn dragging_a_key_moves_it_and_emits() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| CurveView::new(curve(), 0.0, 2.0, 0.0, 1.0, cx));
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &CurveEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            let (a, r) = (*v.axis(), *v.range());
            assert!(v.press(a.x(1.0), r.y(1.0)));
            v.drag_to(a.x(1.2), r.y(0.5), cx);
            v.release();
            v.drag_to(a.x(0.5), r.y(0.0), cx); // released: ignored
        });
        let k = view.read_with(&cx, |v, _| v.curve().keys()[1]);
        assert!(
            (k.t - 1.2).abs() < 1e-9 && (k.v - 0.5).abs() < 1e-9,
            "{k:?}"
        );
        assert_eq!(events.borrow().len(), 1);
    }
}
