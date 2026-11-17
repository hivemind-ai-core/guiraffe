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
    /// A range with `lo`/`hi` swapped if needed and a non-degenerate span.
    pub fn new(lo: f64, hi: f64, height: f64) -> Self {
        let lo = if lo.is_finite() { lo } else { 0. };
        let hi = if hi.is_finite() { hi } else { 1. };
        let height = if height.is_finite() && height > 0. {
            height
        } else {
            1.
        };
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let hi = if hi - lo < 1e-9 { lo + 1.0 } else { hi };
        Self { lo, hi, height }
    }

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
    let keys = curve.keys();
    let lo = keys.partition_point(|k| k.t < axis.t(x - KEY_GRAB));
    let hi = keys.partition_point(|k| k.t <= axis.t(x + KEY_GRAB));
    keys.get(lo..hi)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(i, k)| (i, (axis.x(k.t) - x).hypot(range.y(k.v) - y)))
        .filter(|(_, d)| *d <= KEY_GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| lo + i)
}

/// Effective handle locations include visible defaults for newly enabled Bezier keys.
fn tangent_handles(curve: &Curve, index: usize) -> Option<[(bool, (f64, f64)); 2]> {
    let key = curve.keys().get(index)?;
    let previous = index.checked_sub(1).and_then(|i| curve.keys().get(i));
    if key.interp != crate::Interp::Bezier
        && !previous.is_some_and(|k| k.interp == crate::Interp::Bezier)
    {
        return None;
    }
    let left = previous.map(|k| (key.t - k.t) / 3.).unwrap_or_else(|| {
        curve
            .keys()
            .get(index + 1)
            .map(|k| (k.t - key.t) / 3.)
            .unwrap_or(1. / 3.)
    });
    let right = curve
        .keys()
        .get(index + 1)
        .map(|k| (k.t - key.t) / 3.)
        .unwrap_or(left);
    Some([
        (
            true,
            if key.in_handle == (0., 0.) {
                (-left, 0.)
            } else {
                key.in_handle
            },
        ),
        (
            false,
            if key.out_handle == (0., 0.) {
                (right, 0.)
            } else {
                key.out_handle
            },
        ),
    ])
}
#[derive(Debug, Clone, PartialEq)]
pub enum CurveEvent {
    KeyMoved { index: usize, t: f64, v: f64 },
    Changed,
    SelectionChanged,
}
#[derive(Clone)]
pub struct CurveSeries {
    pub name: String,
    pub curve: Rc<Curve>,
    pub color: ColorName,
    pub visible: bool,
}
impl CurveSeries {
    pub fn new(name: impl Into<String>, curve: Curve, color: ColorName) -> Self {
        Self {
            name: name.into(),
            curve: Rc::new(curve),
            color,
            visible: true,
        }
    }
}
#[derive(Clone, Copy)]
enum Drag {
    Key(usize, usize),
    Handle(usize, usize, bool),
}
pub struct CurveView {
    curves: Vec<CurveSeries>,
    active: usize,
    surface: crate::PlotSurface,
    selected: Option<(usize, usize)>,
    drag: Option<Drag>,
    body: Rc<Cell<Bounds<Pixels>>>,
    pending_fit: Option<(f64, f64)>,
    tangent_mode: crate::TangentMode,
    editable: bool,
}
impl EventEmitter<CurveEvent> for CurveView {}
impl CurveView {
    pub fn new(curve: Curve, t0: f64, t1: f64, lo: f64, hi: f64, cx: &mut App) -> Entity<Self> {
        let mut surface =
            crate::PlotSurface::timeline(Axis::new(t0, t1, 600.), crate::TimeFormat::Seconds);
        surface.range = Some(ValueRange::new(lo, hi, 300.));
        cx.new(|_| Self {
            curves: vec![CurveSeries::new("Curve", curve, ColorName::Blue)],
            active: 0,
            surface,
            selected: None,
            drag: None,
            body: Rc::default(),
            pending_fit: Some((t0, t1)),
            tangent_mode: Default::default(),
            editable: true,
        })
    }
    pub fn curve(&self) -> &Curve {
        &self.curves[self.active].curve
    }
    pub fn curves(&self) -> &[CurveSeries] {
        &self.curves
    }
    pub fn axis(&self) -> &Axis {
        &self.surface.axis
    }
    pub fn range(&self) -> &ValueRange {
        self.surface.range.as_ref().unwrap_or(&DEFAULT_RANGE)
    }
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.selected
    }
    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn add_curve(&mut self, curve: CurveSeries, cx: &mut Context<Self>) -> usize {
        self.curves.push(curve);
        cx.notify();
        self.curves.len() - 1
    }
    pub fn set_curve_visible(&mut self, index: usize, visible: bool, cx: &mut Context<Self>) {
        if let Some(c) = self.curves.get_mut(index) {
            c.visible = visible;
            cx.notify();
        }
    }
    pub fn set_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.editable = on;
        self.drag = None;
        cx.notify();
    }
    pub fn set_tangent_mode(&mut self, mode: crate::TangentMode) {
        self.tangent_mode = mode;
    }
    pub fn select_key(&mut self, curve: usize, key: usize, cx: &mut Context<Self>) -> bool {
        if !self
            .curves
            .get(curve)
            .is_some_and(|c| key < c.curve.keys().len())
        {
            return false;
        }
        self.active = curve;
        self.selected = Some((curve, key));
        cx.emit(CurveEvent::SelectionChanged);
        cx.notify();
        true
    }
    pub fn add_key(&mut self, key: crate::Key, cx: &mut Context<Self>) -> Option<usize> {
        if !self.editable {
            return None;
        }
        let i = Rc::make_mut(&mut self.curves[self.active].curve).set(key)?;
        self.select_key(self.active, i, cx);
        cx.emit(CurveEvent::Changed);
        Some(i)
    }
    pub fn insert_at(&mut self, time: f64, cx: &mut Context<Self>) -> Option<usize> {
        if !self.editable {
            return None;
        }
        let i = Rc::make_mut(&mut self.curves[self.active].curve).insert_at(time)?;
        self.select_key(self.active, i, cx);
        cx.emit(CurveEvent::Changed);
        Some(i)
    }
    pub fn remove_selected(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((c, i)) = self.selected else {
            return false;
        };
        if !self.editable {
            return false;
        }
        if Rc::make_mut(&mut self.curves[c].curve).remove(i) {
            self.selected = None;
            self.drag = None;
            cx.emit(CurveEvent::Changed);
            cx.notify();
            true
        } else {
            false
        }
    }
    pub fn set_interpolation(&mut self, interp: crate::Interp, cx: &mut Context<Self>) -> bool {
        let Some((c, i)) = self.selected else {
            return false;
        };
        if self.editable && Rc::make_mut(&mut self.curves[c].curve).set_interpolation(i, interp) {
            cx.emit(CurveEvent::Changed);
            cx.notify();
            true
        } else {
            false
        }
    }
    pub fn set_tangent(
        &mut self,
        incoming: bool,
        handle: (f64, f64),
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((c, i)) = self.selected else {
            return false;
        };
        if self.editable
            && Rc::make_mut(&mut self.curves[c].curve).set_tangent(
                i,
                incoming,
                handle,
                self.tangent_mode,
            )
        {
            cx.emit(CurveEvent::Changed);
            cx.notify();
            true
        } else {
            false
        }
    }
    pub fn split_segment(&mut self, index: usize, t: f64, cx: &mut Context<Self>) -> Option<usize> {
        if !self.editable {
            return None;
        }
        let i = Rc::make_mut(&mut self.curves[self.active].curve).split_segment(index, t)?;
        self.select_key(self.active, i, cx);
        cx.emit(CurveEvent::Changed);
        Some(i)
    }
    pub fn remove_segment(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if self.editable && Rc::make_mut(&mut self.curves[self.active].curve).remove_segment(index)
        {
            self.selected = None;
            cx.emit(CurveEvent::Changed);
            cx.notify();
            true
        } else {
            false
        }
    }
    pub fn fit_view(&mut self, normalize: bool, cx: &mut Context<Self>) {
        let mut times = (f64::INFINITY, f64::NEG_INFINITY);
        let mut values = times;
        for c in self.curves.iter().filter(|c| c.visible) {
            for k in c.curve.keys() {
                times = (times.0.min(k.t), times.1.max(k.t));
                values = (values.0.min(k.v), values.1.max(k.v));
            }
        }
        self.surface.fit(times.0, times.1, 0.05);
        if normalize && values.0.is_finite() {
            self.surface.range = Some(ValueRange::new(values.0, values.1, self.range().height));
        }
        cx.notify();
    }
    pub fn press(&mut self, x: f64, y: f64) -> bool {
        self.drag = None;
        if let Some((c, i)) = self.selected
            && let Some(key) = self.curves[c].curve.keys().get(i)
        {
            for (incoming, h) in tangent_handles(&self.curves[c].curve, i)
                .into_iter()
                .flatten()
            {
                if h != (0., 0.)
                    && (self.surface.axis.x(key.t + h.0) - x).hypot(self.range().y(key.v + h.1) - y)
                        <= KEY_GRAB
                {
                    self.drag = self.editable.then_some(Drag::Handle(c, i, incoming));
                    return true;
                }
            }
        }
        for (c, series) in self
            .curves
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, c)| c.visible)
        {
            if let Some(i) = pick_key(&series.curve, &self.surface.axis, self.range(), x, y) {
                self.selected = Some((c, i));
                self.active = c;
                self.drag = self.editable.then_some(Drag::Key(c, i));
                return true;
            }
        }
        self.selected = None;
        false
    }
    pub fn drag_to(&mut self, x: f64, y: f64, cx: &mut Context<Self>) {
        if !self.editable {
            return;
        }
        let Some(drag) = self.drag else { return };
        let (t, v) = (self.surface.axis.t(x), self.range().v(y));
        match drag {
            Drag::Key(c, i) => {
                if Rc::make_mut(&mut self.curves[c].curve).move_key(i, t, v) {
                    if let Some(k) = self.curves[c].curve.keys().get(i) {
                        cx.emit(CurveEvent::KeyMoved {
                            index: i,
                            t: k.t,
                            v: k.v,
                        });
                    }
                    cx.notify();
                }
            }
            Drag::Handle(c, i, incoming) => {
                if let Some(k) = self.curves[c].curve.keys().get(i).copied() {
                    self.set_tangent(incoming, (t - k.t, v - k.v), cx);
                }
            }
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
const DEFAULT_RANGE: ValueRange = ValueRange {
    lo: 0.,
    hi: 1.,
    height: 1.,
};
impl Render for CurveView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let b = self.body.get();
        let (w, h) = (
            f64::from(f32::from(b.size.width)),
            f64::from(f32::from(b.size.height)),
        );
        if w > 0. && h > 0. {
            self.surface.axis.set_width(w);
            if let Some(r) = self.surface.range.as_mut() {
                r.height = h;
            }
            if let Some((a, b)) = self.pending_fit.take() {
                self.surface.fit(a, b, 0.02);
            }
        }
        let (axis, range) = (self.surface.axis, *self.range());
        let curves = self.curves.clone();
        let selected = self.selected;
        let body = self.body.clone();
        let owner = cx.entity_id();
        let paint_theme = theme.clone();
        div()
            .id("curve-body")
            .debug_selector(|| "curve-body".into())
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(theme.surfaces().body.to_hsla())
            .cursor(gpui_kit::CursorStyle::Crosshair)
            .on_scroll(cx.listener(|this, e: &gpui_kit::ScrollEvent, _, cx| {
                let d = e.delta.pixel_delta(px(16.));
                let (x, _) = this.local(e.position);
                if this.surface.navigate(
                    x,
                    f64::from(f32::from(d.x)),
                    f64::from(f32::from(d.y)),
                    e.modifiers.control || e.modifiers.platform,
                ) {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    let (x, y) = this.local(e.position);
                    this.press(x, y);
                    cx.emit(CurveEvent::SelectionChanged);
                    cx.notify();
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
                    move |b, _, cx| crate::measure::record(&body, b, cx, owner),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(self.surface.element(theme, false, 24., 0.))
            .child(
                canvas(
                    |_, _, _| (),
                    move |b, _, window, _| {
                        let at = |x: f64, y: f64| {
                            point(b.origin.x + px(x as f32), b.origin.y + px(y as f32))
                        };
                        let width = f64::from(f32::from(b.size.width));
                        for (c, series) in curves.iter().enumerate().filter(|(_, c)| c.visible) {
                            if series.curve.keys().is_empty() {
                                continue;
                            }
                            let stroke = paint_theme
                                .variant(Some(series.color), guiraffe_core::Variant::Filled)
                                .background
                                .to_hsla();
                            let mut path = PathBuilder::stroke(px(1.5));
                            let n = (width / 2.).ceil().max(1.) as usize;
                            for i in 0..=n {
                                let x = width * i as f64 / n as f64;
                                let p = at(x, range.y(series.curve.eval(axis.t(x))));
                                if i == 0 {
                                    path.move_to(p)
                                } else {
                                    path.line_to(p)
                                }
                            }
                            if let Ok(path) = path.build() {
                                window.paint_path(path, stroke);
                            }
                            let keys = series.curve.keys();
                            let lo =
                                keys.partition_point(|k| k.t < axis.start - KEY_GRAB * axis.scale);
                            let hi =
                                keys.partition_point(|k| k.t <= axis.end() + KEY_GRAB * axis.scale);
                            for (offset, key) in keys.get(lo..hi).unwrap_or(&[]).iter().enumerate()
                            {
                                let k = lo + offset;
                                let chosen = selected == Some((c, k));
                                let x = axis.x(key.t);
                                let y = range.y(key.v);
                                if chosen {
                                    for (_, handle) in
                                        tangent_handles(&series.curve, k).into_iter().flatten()
                                    {
                                        let p =
                                            at(axis.x(key.t + handle.0), range.y(key.v + handle.1));
                                        let mut path = PathBuilder::stroke(px(1.));
                                        path.move_to(at(x, y));
                                        path.line_to(p);
                                        if let Ok(path) = path.build() {
                                            window.paint_path(path, stroke);
                                        }
                                        window.paint_quad(gpui_kit::fill(
                                            Bounds::new(
                                                point(p.x - px(3.), p.y - px(3.)),
                                                gpui_kit::size(px(6.), px(6.)),
                                            ),
                                            stroke,
                                        ));
                                    }
                                }
                                let r = if chosen { 6. } else { 4. };
                                let mut path = PathBuilder::fill();
                                path.move_to(at(x, y - r));
                                path.line_to(at(x + r, y));
                                path.line_to(at(x, y + r));
                                path.line_to(at(x - r, y));
                                path.close();
                                if let Ok(path) = path.build() {
                                    window.paint_path(
                                        path,
                                        if chosen {
                                            paint_theme.surfaces().text.to_hsla()
                                        } else {
                                            stroke
                                        },
                                    );
                                }
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

    #[test]
    fn value_range_new_swaps_and_widens() {
        let r = ValueRange::new(1.0, -1.0, 100.0);
        assert_eq!((r.lo, r.hi), (-1.0, 1.0));
        let d = ValueRange::new(2.0, 2.0, 100.0);
        assert!(d.y(2.0).is_finite() && d.hi > d.lo);
    }
}

#[cfg(test)]
mod tangent_defaults {
    use super::*;
    #[test]
    fn newly_enabled_bezier_has_separate_pickable_handles() {
        let mut c = Curve::default();
        c.set(crate::Key::new(0., 0., crate::Interp::Linear));
        c.set(crate::Key::new(1., 1., crate::Interp::Linear));
        assert!(tangent_handles(&c, 0).is_none());
        c.set_interpolation(0, crate::Interp::Bezier);
        let handles = tangent_handles(&c, 0).expect("bezier handles");
        assert_eq!(handles[0].1, (-1. / 3., 0.));
        assert_eq!(handles[1].1, (1. / 3., 0.));
    }
}
