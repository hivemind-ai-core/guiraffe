//! Native bar drawing surface; the pure model interpolates sparse pointer events.
use crate::{Axis, BarCurve, PlotSurface, ValueRange};
use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Render, Role, Styled, TestSupportExt,
    Window, canvas, div, prelude::*, px,
};
use guiraffe_core::{GuiraffeTheme, Variant};
use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::Rc,
};
#[derive(Default)]
struct BarColumns {
    extrema: Vec<(f64, f64)>,
    count: usize,
    dirty: Option<Range<usize>>,
}

impl BarColumns {
    fn invalidate(&mut self, changed: Range<usize>) {
        self.dirty = Some(match self.dirty.take() {
            Some(dirty) => dirty.start.min(changed.start)..dirty.end.max(changed.end),
            None => changed,
        });
    }

    fn update(&mut self, values: &[f64], width: f32) {
        let columns = (width.ceil() as usize).min(16_384).min(values.len());
        if columns == 0 || columns == values.len() {
            self.extrema.clear();
            return;
        }
        if self.count != values.len() || self.extrema.len() != columns {
            self.count = values.len();
            self.extrema
                .reserve_exact(columns.saturating_sub(self.extrema.len()));
            self.extrema.resize(columns, (0., 0.));
            self.dirty = Some(0..values.len());
        }
        let Some(dirty) = self.dirty.take() else {
            return;
        };
        let first = dirty.start * columns / self.count;
        let last = (dirty.end - 1) * columns / self.count + 1;
        for column in first..last {
            let start = (column * self.count).div_ceil(columns);
            let end = ((column + 1) * self.count).div_ceil(columns);
            self.extrema[column] = values[start..end]
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &value| {
                    (lo.min(value), hi.max(value))
                });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarEvent {
    pub changed: std::ops::Range<usize>,
}
pub struct BarCurveView {
    model: BarCurve,
    surface: PlotSurface,
    body: Rc<Cell<Bounds<Pixels>>>,
    columns: Rc<RefCell<BarColumns>>,
    drag: Option<(f64, f64)>,
    editable: bool,
    focus: FocusHandle,
    focusable: Option<bool>,
}
impl EventEmitter<BarEvent> for BarCurveView {}
impl Focusable for BarCurveView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BarCurveView {
    pub fn new(model: BarCurve, cx: &mut App) -> Entity<Self> {
        let (lo, hi) = model.range();
        let n = model.values().len().max(1);
        let mut surface = PlotSurface::new(Axis::new(0., n as f64, 600.));
        surface.range = Some(ValueRange::new(lo, hi, 200.));
        surface.add_layer(crate::Gridlines::default());
        cx.new(|cx| Self {
            model,
            surface,
            body: Rc::default(),
            columns: Rc::default(),
            drag: None,
            editable: true,
            focus: cx.focus_handle(),
            focusable: None,
        })
    }
    pub fn model(&self) -> &BarCurve {
        &self.model
    }
    pub fn set_focusable(&mut self, on: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = on;
        cx.notify();
    }
    pub fn set_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.editable = on;
        self.drag = None;
        cx.notify();
    }
    pub fn set_model(&mut self, model: BarCurve, cx: &mut Context<Self>) {
        self.model = model;
        self.columns.borrow_mut().dirty = Some(0..self.model.values().len());
        self.drag = None;
        let (lo, hi) = self.model.range();
        self.surface.range = Some(ValueRange::new(lo, hi, 200.));
        cx.notify();
    }
    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn draw_to(&mut self, x: f64, value: f64, cx: &mut Context<Self>) {
        if !self.editable {
            return;
        }
        let to = (x, value);
        let from = self.drag.unwrap_or(to);
        if let Some(changed) = self.model.draw(from, to) {
            self.columns.borrow_mut().invalidate(changed.clone());
            cx.emit(BarEvent { changed });
            cx.notify();
        }
        self.drag = Some(to);
    }
    pub fn release(&mut self) {
        self.drag = None;
    }
    fn pointer(&self, p: gpui_kit::Point<Pixels>) -> (f64, f64) {
        let b = self.body.get();
        let w = f64::from(f32::from(b.size.width)).max(1.);
        let h = f64::from(f32::from(b.size.height)).max(1.);
        let (lo, hi) = self.model.range();
        (
            (f64::from(f32::from(p.x - b.origin.x)) / w * self.model.values().len() as f64 - 0.5)
                .clamp(0., self.model.values().len().saturating_sub(1) as f64),
            hi - f64::from(f32::from(p.y - b.origin.y)) / h * (hi - lo),
        )
    }
}
impl Render for BarCurveView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let width = f64::from(f32::from(self.body.get().size.width));
        if width > 0. {
            self.surface.axis.set_width(width);
            self.surface
                .fit(0., self.model.values().len().max(1) as f64, 0.);
        }
        let body = self.body.clone();
        let owner = cx.entity_id();
        let view = cx.weak_entity();
        let columns = self.columns.clone();
        let (lo, hi) = self.model.range();
        let bipolar = self.model.bipolar;
        let fill = theme.variant(None, Variant::Filled).background.to_hsla();
        let ring_on = guiraffe_core::ring_visible(&self.focus, window);
        let ring = theme
            .state(guiraffe_core::StateKind::FocusRing, Variant::Default, None)
            .border;
        let tab = guiraffe_core::tab_stop(self.focusable);
        let root = div()
            .id("bar-curve")
            .test_support()
            .role(Role::Group)
            .aria_label("Bar curve editor")
            .track_focus(&self.focus.clone().tab_stop(tab))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .debug_selector(|| "bar-curve".into())
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.surfaces().body.to_hsla())
            .cursor(if self.editable {
                gpui_kit::CursorStyle::Crosshair
            } else {
                gpui_kit::CursorStyle::OperationNotAllowed
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    let (x, v) = this.pointer(e.position);
                    this.drag = None;
                    this.draw_to(x, v, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) && this.drag.is_some() {
                    let (x, v) = this.pointer(e.position);
                    this.draw_to(x, v, cx);
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
            .child(self.surface.element(theme.clone(), false, 24., 0.))
            .child(
                canvas(
                    |_, _, _| (),
                    move |b, _, window, cx| {
                        let Some(view) = view.upgrade() else { return };
                        let values = view.read(cx).model.values();
                        let mut columns = columns.borrow_mut();
                        let width = f32::from(b.size.width);
                        if width <= 0. {
                            return;
                        }
                        columns.update(values, width);
                        let dense = !columns.extrema.is_empty();
                        let count = if dense {
                            columns.extrema.len()
                        } else {
                            values.len()
                        };
                        let w = width / count.max(1) as f32;
                        let inset = if dense || w <= 2. { 0. } else { 1. };
                        let h = f64::from(f32::from(b.size.height));
                        let range = ValueRange::new(lo, hi, h);
                        let base = if bipolar {
                            range.y(0_f64.clamp(lo, hi))
                        } else {
                            h
                        };
                        let extrema = (0..count).map(|i| {
                            if dense {
                                columns.extrema[i]
                            } else {
                                (values[i], values[i])
                            }
                        });
                        window.paint_layer(b, |window| {
                            for (i, (low, high)) in extrema.enumerate() {
                                let top = range.y(high).min(base);
                                let bottom = range.y(low).max(base);
                                window.paint_quad(gpui_kit::fill(
                                    Bounds::new(
                                        gpui_kit::point(
                                            b.origin.x + px(i as f32 * w + inset),
                                            b.origin.y + px(top as f32),
                                        ),
                                        gpui_kit::size(
                                            px((w - 2. * inset).max(0.)),
                                            px((bottom - top).max(1.) as f32),
                                        ),
                                    ),
                                    fill,
                                ));
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(self.surface.value_labels(theme));
        guiraffe_core::focus_ring(div().size_full().child(root), ring_on, px(0.), ring)
    }
}

#[cfg(test)]
mod tests {
    use super::BarColumns;

    #[test]
    fn dirty_range_excludes_the_next_column_when_sizes_do_not_divide() {
        let mut values = [0.; 5];
        let mut columns = BarColumns::default();
        columns.update(&values, 3.);
        values[1] = 1.;
        values[2] = 0.5;
        columns.invalidate(1..2);
        columns.update(&values, 3.);
        assert_eq!(columns.extrema, [(0., 1.), (0., 0.), (0., 0.)]);
    }

    #[test]
    fn aggregation_keeps_extrema_and_recomputes_only_dirty_columns() {
        for size in [10_000, 100_000, 10_000_000] {
            let mut values = vec![0.; size];
            let mut columns = BarColumns::default();
            columns.update(&values, 100.);
            assert_eq!(columns.extrema.len(), 100);
            values[0] = -1.;
            values[1] = 1.;
            // Sentinel outside the reported dirty range detects full rebuilds.
            values[size - 1] = 0.5;
            columns.invalidate(0..2);
            columns.update(&values, 100.);
            assert_eq!(columns.extrema[0], (-1., 1.));
            assert_eq!(columns.extrema[99], (0., 0.));
        }
    }
}
