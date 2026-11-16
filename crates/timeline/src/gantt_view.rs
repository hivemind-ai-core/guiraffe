//! The Gantt chart view: ruler, task bars, dependency arrows, critical path, drag to
//! reschedule, wheel pan / ctrl+wheel zoom.

use std::{collections::HashMap, rc::Rc};

use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, Hsla, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, PathBuilder, Pixels, Render, ScrollEvent, Styled, Window,
    canvas, div, point, prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Variant};

use crate::{
    axis::{Axis, TimeFormat, label},
    gantt::{ScheduleError, Scheduled, Task, schedule},
};

pub const ROW_H: f32 = 32.0;
pub const RULER_H: f32 = 28.0;
pub const LABEL_W: f32 = 160.0;

#[derive(Debug, Clone, PartialEq)]
pub enum GanttEvent {
    /// A task was dragged to a new earliest start.
    Rescheduled { id: u32, not_before: f64 },
}

struct Drag {
    id: u32,
    grab_dt: f64,
}

pub struct GanttView {
    tasks: Vec<Task>,
    schedule: Result<HashMap<u32, Scheduled>, ScheduleError>,
    axis: Axis,
    format: TimeFormat,
    drag: Option<Drag>,
    body: Rc<std::cell::Cell<Bounds<Pixels>>>,
}

impl EventEmitter<GanttEvent> for GanttView {}

impl GanttView {
    pub fn new(tasks: Vec<Task>, format: TimeFormat, cx: &mut App) -> Entity<Self> {
        let schedule = schedule(&tasks);
        let (lo, hi) = span(&schedule);
        let mut axis = Axis::new(lo, hi, 800.0);
        axis.fit(lo, hi, 0.05);
        cx.new(|_| Self {
            tasks,
            schedule,
            axis,
            format,
            drag: None,
            body: Rc::default(),
        })
    }

    pub fn schedule(&self) -> &Result<HashMap<u32, Scheduled>, ScheduleError> {
        &self.schedule
    }

    pub fn axis(&self) -> &Axis {
        &self.axis
    }

    pub fn set_tasks(&mut self, tasks: Vec<Task>, cx: &mut Context<Self>) {
        self.tasks = tasks;
        self.schedule = schedule(&self.tasks);
        cx.notify();
    }

    /// Move task `id`'s earliest start and reschedule (dependents follow).
    pub fn reschedule(&mut self, id: u32, not_before: f64, cx: &mut Context<Self>) {
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        if t.not_before == not_before || !not_before.is_finite() {
            return;
        }
        t.not_before = not_before;
        self.schedule = schedule(&self.tasks);
        cx.emit(GanttEvent::Rescheduled { id, not_before });
        cx.notify();
    }

    fn on_wheel(&mut self, e: &ScrollEvent, cx: &mut Context<Self>) {
        let d = e.delta.pixel_delta(px(16.0));
        let b = self.body.get();
        let x = f64::from(f32::from(e.position.x - b.origin.x));
        let changed = if e.modifiers.control || e.modifiers.platform {
            self.axis
                .zoom_at(x, (1.0 + f64::from(f32::from(d.y)) * 0.002).max(0.1))
        } else {
            let dx = if f32::from(d.x) != 0.0 { d.x } else { d.y };
            self.axis.pan_px(-f64::from(f32::from(dx)));
            true
        };
        if changed {
            cx.notify();
        }
    }

    fn row_at(&self, y: Pixels) -> Option<usize> {
        let b = self.body.get();
        let r = f32::from(y - b.origin.y) / ROW_H;
        (r >= 0.0)
            .then_some(r as usize)
            .filter(|r| *r < self.tasks.len())
    }
}

fn span(s: &Result<HashMap<u32, Scheduled>, ScheduleError>) -> (f64, f64) {
    let Ok(m) = s else { return (0.0, 1.0) };
    let lo = m
        .values()
        .map(|x| x.early_start)
        .fold(f64::INFINITY, f64::min);
    let hi = m
        .values()
        .map(|x| x.early_finish)
        .fold(f64::NEG_INFINITY, f64::max);
    if lo.is_finite() && hi > lo {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

fn arrow(
    window: &mut Window,
    from: gpui_kit::Point<Pixels>,
    to: gpui_kit::Point<Pixels>,
    color: Hsla,
) {
    let mid = px((f32::from(from.x) + f32::from(to.x)) / 2.0).max(from.x + px(8.0));
    let mut b = PathBuilder::stroke(px(1.5));
    b.move_to(from);
    b.cubic_bezier_to(point(mid, to.y), point(mid, from.y), to);
    if let Ok(p) = b.build() {
        window.paint_path(p, color);
    }
    let mut head = PathBuilder::fill();
    head.move_to(to);
    head.line_to(point(to.x - px(6.0), to.y - px(4.0)));
    head.line_to(point(to.x - px(6.0), to.y + px(4.0)));
    head.close();
    if let Ok(p) = head.build() {
        window.paint_path(p, color);
    }
}

impl Render for GanttView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let normal = t.variant(None, Variant::Light);
        let critical = t.variant(Some(ColorName::Red), Variant::Filled);
        let body_w = f64::from(f32::from(self.body.get().size.width)).max(1.0);
        self.axis.set_width(body_w);
        let axis = self.axis;
        let step = axis.tick_step(90.0, self.format);
        let ticks = axis.ticks(step);
        let format = self.format;
        let sched = self.schedule.as_ref().ok().cloned().unwrap_or_default();
        let error = self.schedule.as_ref().err().map(|e| format!("{e:?}"));
        let rows: Vec<(u32, String, Option<Scheduled>)> = self
            .tasks
            .iter()
            .map(|t| (t.id, t.name.clone(), sched.get(&t.id).copied()))
            .collect();
        let index: HashMap<u32, usize> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id, i))
            .collect();
        let deps: Vec<(usize, usize)> = self
            .tasks
            .iter()
            .enumerate()
            .flat_map(|(i, t)| {
                t.after
                    .iter()
                    .filter_map(|d| index.get(d).map(|&j| (j, i)))
                    .collect::<Vec<_>>()
            })
            .collect();
        let body = self.body.clone();
        let line = s.border.to_hsla();
        let arrow_color = s.dimmed.to_hsla();
        let sched2 = sched.clone();
        let ids: Vec<u32> = self.tasks.iter().map(|t| t.id).collect();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(13.))
            .child(
                // Ruler.
                div()
                    .h(px(RULER_H))
                    .flex()
                    .border_b_1()
                    .border_color(line)
                    .child(
                        div()
                            .w(px(LABEL_W))
                            .flex_none()
                            .border_r_1()
                            .border_color(line),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .children(ticks.iter().map(move |tt| {
                                div()
                                    .absolute()
                                    .left(px(axis.x(*tt) as f32))
                                    .h_full()
                                    .pl(px(4.))
                                    .border_l_1()
                                    .border_color(line)
                                    .text_size(px(11.))
                                    .text_color(s.dimmed.to_hsla())
                                    .child(label(*tt, format))
                            })),
                    ),
            )
            .when_some(error, |el, e| {
                el.child(
                    div()
                        .p(px(8.))
                        .text_color(ColorName::Red.shade(7).to_hsla())
                        .child(e),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .child(
                        div()
                            .w(px(LABEL_W))
                            .flex_none()
                            .border_r_1()
                            .border_color(line)
                            .children(rows.iter().map(|(_, name, sc)| {
                                let crit = sc.is_some_and(|x| x.critical());
                                div()
                                    .h(px(ROW_H))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(line)
                                    .when(crit, |el| el.text_color(critical.background.to_hsla()))
                                    .child(name.clone())
                            })),
                    )
                    .child(
                        div()
                            .id("gantt-body")
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_scroll(
                                cx.listener(|this, e: &ScrollEvent, _, cx| this.on_wheel(e, cx)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, _, _| {
                                    let Some(row) = this.row_at(e.position.y) else {
                                        return;
                                    };
                                    let id = this.tasks[row].id;
                                    let start = this
                                        .schedule
                                        .as_ref()
                                        .ok()
                                        .and_then(|m| m.get(&id))
                                        .map(|x| x.early_start)
                                        .unwrap_or(0.0);
                                    let at = this.axis.t(f64::from(f32::from(
                                        e.position.x - this.body.get().origin.x,
                                    )));
                                    this.drag = Some(Drag {
                                        id,
                                        grab_dt: at - start,
                                    });
                                }),
                            )
                            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button != Some(MouseButton::Left) {
                                    return;
                                }
                                let Some(d) = this.drag.as_ref().map(|d| (d.id, d.grab_dt)) else {
                                    return;
                                };
                                let at = this.axis.t(f64::from(f32::from(
                                    e.position.x - this.body.get().origin.x,
                                )));
                                this.reschedule(d.0, (at - d.1).max(0.0), cx);
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, _| this.drag = None),
                            )
                            .child(
                                canvas(move |b, _, _| body.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                            .children(rows.iter().enumerate().map(|(i, (_, _, sc))| {
                                let bar = sc.map(|x| {
                                    let (x0, x1) = (
                                        axis.x(x.early_start) as f32,
                                        axis.x(x.early_finish) as f32,
                                    );
                                    let v = if x.critical() { critical } else { normal };
                                    div()
                                        .absolute()
                                        .top(px(i as f32 * ROW_H + 6.0))
                                        .left(px(x0))
                                        .w(px((x1 - x0).max(2.0)))
                                        .h(px(ROW_H - 12.0))
                                        .rounded(px(4.))
                                        .bg(v.background.to_hsla())
                                        .border_1()
                                        .border_color(v.foreground.to_hsla())
                                });
                                div().children(bar)
                            }))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |b, _, window, _| {
                                        for (from, to) in &deps {
                                            let (Some(a), Some(c)) = (
                                                ids.get(*from).and_then(|id| sched2.get(id)),
                                                ids.get(*to).and_then(|id| sched2.get(id)),
                                            ) else {
                                                continue;
                                            };
                                            let y = |row: usize| {
                                                b.origin.y + px(row as f32 * ROW_H + ROW_H / 2.0)
                                            };
                                            let p0 = point(
                                                b.origin.x + px(axis.x(a.early_finish) as f32),
                                                y(*from),
                                            );
                                            let p1 = point(
                                                b.origin.x + px(axis.x(c.early_start) as f32),
                                                y(*to),
                                            );
                                            arrow(window, p0, p1, arrow_color);
                                        }
                                    },
                                )
                                .absolute()
                                .size_full(),
                            ),
                    ),
            )
    }
}
