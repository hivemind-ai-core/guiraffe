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
    clips::Edge,
    edit::{Grab, ResizeEdges, cursor_for, grab_at},
    gantt::{ScheduleError, Scheduled, Task, schedule},
};

pub const ROW_H: f32 = 32.0;
pub const RULER_H: f32 = 28.0;
pub const LABEL_W: f32 = 160.0;

#[derive(Debug, Clone, PartialEq)]
pub enum GanttEvent {
    /// A task was dragged to a new earliest start.
    Rescheduled { id: u32, not_before: f64 },
    /// A task edge was dragged: new earliest start and duration.
    Resized {
        id: u32,
        not_before: f64,
        duration: f64,
    },
}

/// Shortest duration an edge drag can produce (seconds).
pub const MIN_DURATION: f64 = 1e-3;

#[derive(Clone, Copy)]
struct Drag {
    id: u32,
    grab: Grab,
}

/// Per-row data derived from tasks + schedule; rebuilt only when either changes, shared
/// with render closures by `Rc` (no per-frame copies).
#[derive(Default)]
struct Geometry {
    rows: Vec<(gpui_kit::SharedString, Option<Scheduled>)>,
    /// (predecessor row, successor row)
    deps: Vec<(usize, usize)>,
}

fn geometry(
    tasks: &[Task],
    sched: &Result<HashMap<u32, Scheduled>, ScheduleError>,
) -> Rc<Geometry> {
    let m = sched.as_ref().ok();
    let index: HashMap<u32, usize> = tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    Rc::new(Geometry {
        rows: tasks
            .iter()
            .map(|t| (t.name.clone().into(), m.and_then(|m| m.get(&t.id)).copied()))
            .collect(),
        deps: tasks
            .iter()
            .enumerate()
            .flat_map(|(i, t)| {
                t.after
                    .iter()
                    .filter_map(|d| index.get(d).map(|&j| (j, i)))
                    .collect::<Vec<_>>()
            })
            .collect(),
    })
}

pub struct GanttView {
    geometry: Rc<Geometry>,
    tasks: Vec<Task>,
    schedule: Result<HashMap<u32, Scheduled>, ScheduleError>,
    axis: Axis,
    format: TimeFormat,
    drag: Option<Drag>,
    body: Rc<std::cell::Cell<Bounds<Pixels>>>,
    /// Range to fit once the real width is known.
    pending_fit: Option<(f64, f64)>,
    resize: ResizeEdges,
    /// Tasks that cannot be moved or resized.
    locked: std::collections::HashSet<u32>,
    /// When false, nothing can be edited.
    editable: bool,
    /// What the pointer is over, for the cursor.
    hover: Option<Grab>,
}

impl EventEmitter<GanttEvent> for GanttView {}

impl GanttView {
    pub fn new(tasks: Vec<Task>, format: TimeFormat, cx: &mut App) -> Entity<Self> {
        let schedule = schedule(&tasks);
        let (lo, hi) = span(&schedule);
        let axis = Axis::new(lo, hi, 800.0);
        cx.new(|_| Self {
            geometry: geometry(&tasks, &schedule),
            tasks,
            schedule,
            axis,
            format,
            drag: None,
            body: Rc::default(),
            pending_fit: Some((lo, hi)),
            resize: ResizeEdges::default(),
            locked: Default::default(),
            editable: true,
            hover: None,
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
        self.geometry = geometry(&self.tasks, &self.schedule);
        self.pending_fit = Some(span(&self.schedule));
        cx.notify();
    }

    /// Move task `id`'s earliest start and reschedule (dependents follow).
    pub fn reschedule(&mut self, id: u32, not_before: f64, cx: &mut Context<Self>) {
        if !not_before.is_finite() {
            return;
        }
        // Predecessors bound the earliest start: clamp so the constraint never drifts below
        // what is visible.
        let bound = self.predecessor_bound(id);
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let not_before = not_before.max(bound);
        if t.not_before == not_before {
            return;
        }
        let start = |s: &Result<HashMap<u32, Scheduled>, ScheduleError>| {
            s.as_ref()
                .ok()
                .and_then(|m| m.get(&id))
                .map(|x| x.early_start)
        };
        let before = start(&self.schedule);
        t.not_before = not_before;
        self.schedule = schedule(&self.tasks);
        self.geometry = geometry(&self.tasks, &self.schedule);
        if before != start(&self.schedule) {
            cx.emit(GanttEvent::Rescheduled { id, not_before });
        }
        cx.notify();
    }

    /// Latest early finish among `id`'s predecessors (−∞ if none or unscheduled).
    fn predecessor_bound(&self, id: u32) -> f64 {
        let (Ok(m), Some(t)) = (
            self.schedule.as_ref(),
            self.tasks.iter().find(|t| t.id == id),
        ) else {
            return f64::NEG_INFINITY;
        };
        t.after
            .iter()
            .filter_map(|d| m.get(d))
            .map(|x| x.early_finish)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// What a press at (`row`, `x`) would grab, honouring locks and the resize policy.
    pub fn grab(&self, row: usize, x: f64) -> Option<(u32, Grab)> {
        if !self.editable {
            return None;
        }
        let task = self.tasks.get(row)?;
        if self.locked.contains(&task.id) {
            return None;
        }
        let sc = self.schedule.as_ref().ok()?.get(&task.id)?;
        let x0 = self.axis.x(sc.early_start);
        let x1 = self.axis.x(sc.early_finish).max(x0 + 2.0);
        grab_at(x0, x1, x, self.axis.t(x), sc.early_start, self.resize).map(|g| (task.id, g))
    }

    /// Begin a drag at body-local pixel `x` on `row`; only a press on an editable bar grabs.
    pub fn press(&mut self, row: usize, x: f64) -> bool {
        self.drag = self.grab(row, x).map(|(id, grab)| Drag { id, grab });
        self.drag.is_some()
    }

    /// Continue the active drag to body-local pixel `x`.
    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some(d) = self.drag else {
            return;
        };
        let t = self.axis.t(x);
        match d.grab {
            Grab::Move { grab_dt } => self.reschedule(d.id, t - grab_dt, cx),
            Grab::Trim(edge) => self.resize_edge(d.id, edge, t, cx),
        }
    }

    /// Move one edge of task `id` to time `t`. The end edge changes the duration; the start
    /// edge moves the start and keeps the finish. Never below [`MIN_DURATION`], never before
    /// predecessors.
    pub fn resize_edge(&mut self, id: u32, edge: Edge, t: f64, cx: &mut Context<Self>) {
        if !t.is_finite() {
            return;
        }
        let bound = self.predecessor_bound(id);
        let Some(sc) = self
            .schedule
            .as_ref()
            .ok()
            .and_then(|m| m.get(&id))
            .copied()
        else {
            return;
        };
        let (start, duration) = match edge {
            Edge::End => (sc.early_start, (t - sc.early_start).max(MIN_DURATION)),
            Edge::Start => {
                let start = t.max(bound).min(sc.early_finish - MIN_DURATION);
                (start, sc.early_finish - start)
            }
        };
        let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let not_before = if edge == Edge::Start {
            start
        } else {
            task.not_before
        };
        if task.duration == duration && task.not_before == not_before {
            return;
        }
        task.duration = duration;
        task.not_before = not_before;
        self.schedule = schedule(&self.tasks);
        self.geometry = geometry(&self.tasks, &self.schedule);
        cx.emit(GanttEvent::Resized {
            id,
            not_before,
            duration,
        });
        cx.notify();
    }

    /// Which bar edges resize (default: both).
    pub fn set_resize_edges(&mut self, edges: ResizeEdges, cx: &mut Context<Self>) {
        self.resize = edges;
        cx.notify();
    }

    /// Lock or unlock one task's position and size.
    pub fn set_locked(&mut self, id: u32, locked: bool, cx: &mut Context<Self>) {
        let changed = if locked {
            self.locked.insert(id)
        } else {
            self.locked.remove(&id)
        };
        if changed {
            cx.notify();
        }
    }

    pub fn is_locked(&self, id: u32) -> bool {
        self.locked.contains(&id)
    }

    /// Allow or forbid all edits (a global lock).
    pub fn set_editable(&mut self, editable: bool, cx: &mut Context<Self>) {
        if self.editable != editable {
            self.editable = editable;
            self.drag = None;
            cx.notify();
        }
    }

    fn hover_at(&mut self, row: usize, x: f64, cx: &mut Context<Self>) {
        let g = self.grab(row, x).map(|(_, g)| g);
        if cursor_for(g) != cursor_for(self.hover) {
            self.hover = g;
            cx.notify();
        } else {
            self.hover = g;
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
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
    b.cubic_bezier_to(to, point(mid, from.y), point(mid, to.y));
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
        if let Some(w) = crate::measure::width(&self.body) {
            self.axis.set_width(w);
            if let Some((a, b)) = self.pending_fit.take() {
                self.axis.fit(a, b, 0.05);
            }
        }
        let axis = self.axis;
        let step = axis.tick_step(90.0, self.format);
        let ticks = axis.ticks(step);
        let format = self.format;
        let error = self.schedule.as_ref().err().map(|e| format!("{e:?}"));
        let geo = self.geometry.clone();
        let editable = self.editable;
        let locked_ids = &self.locked;
        let ids: Vec<u32> = if self.locked.is_empty() {
            Vec::new()
        } else {
            self.tasks.iter().map(|t| t.id).collect()
        };
        let rows = geo.clone();
        let body = self.body.clone();
        let line = s.border.to_hsla();
        let arrow_color = s.dimmed.to_hsla();
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
                                    .whitespace_nowrap()
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
                            .children(rows.rows.iter().map(|(name, sc)| {
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
                            .cursor(cursor_for(self.drag.map(|d| d.grab).or(self.hover)))
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_scroll(
                                cx.listener(|this, e: &ScrollEvent, _, cx| this.on_wheel(e, cx)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, _, _| {
                                    let b = this.body.get();
                                    let y = f32::from(e.position.y - b.origin.y);
                                    if y >= 0.0 {
                                        let x = f64::from(f32::from(e.position.x - b.origin.x));
                                        this.press((y / ROW_H) as usize, x);
                                    }
                                }),
                            )
                            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                                let b = this.body.get();
                                let x = f64::from(f32::from(e.position.x - b.origin.x));
                                let y = f32::from(e.position.y - b.origin.y);
                                if e.pressed_button == Some(MouseButton::Left) {
                                    this.drag_to(x, cx);
                                } else if y >= 0.0 {
                                    this.hover_at((y / ROW_H) as usize, x, cx);
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, _| this.release()),
                            )
                            .child(
                                canvas(
                                    move |b, window, _| crate::measure::record(&body, b, window),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .children(rows.rows.iter().enumerate().map(|(i, (_, sc))| {
                                let locked = !editable
                                    || ids.get(i).is_some_and(|id| locked_ids.contains(id));
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
                                        .when(locked, |el| el.opacity(0.55).border_dashed())
                                });
                                div().children(bar)
                            }))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |b, _, window, _| {
                                        for (from, to) in &geo.deps {
                                            let (Some(Some(a)), Some(Some(c))) = (
                                                geo.rows.get(*from).map(|r| r.1),
                                                geo.rows.get(*to).map(|r| r.1),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tasks() -> Vec<Task> {
        vec![
            Task {
                id: 1,
                name: "a".into(),
                not_before: 0.0,
                duration: 2.0,
                after: vec![],
            },
            Task {
                id: 2,
                name: "b".into(),
                not_before: 0.0,
                duration: 1.0,
                after: vec![1],
            },
        ]
    }

    #[test]
    fn rescheduling_moves_dependents_and_emits() {
        let mut cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| GanttView::new(tasks(), TimeFormat::Seconds, cx));
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _sub = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &GanttEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx, |v, cx| v.reschedule(1, 5.0, cx));
        let b = view.read_with(&cx, |v, _| {
            v.schedule().as_ref().ok().and_then(|m| m.get(&2).copied())
        });
        assert_eq!(b.map(|s| s.early_start), Some(7.0));
        assert_eq!(
            *events.borrow(),
            vec![GanttEvent::Rescheduled {
                id: 1,
                not_before: 5.0
            }]
        );
        // No-op and non-finite moves emit nothing.
        view.update(&mut cx, |v, cx| {
            v.reschedule(1, 5.0, cx);
            v.reschedule(1, f64::NAN, cx);
            v.reschedule(99, 1.0, cx);
        });
        assert_eq!(events.borrow().len(), 1);
    }

    #[test]
    fn cycles_surface_as_errors_not_panics() {
        let mut t = tasks();
        t[0].after = vec![2];
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| GanttView::new(t, TimeFormat::Seconds, cx));
        assert!(view.read_with(&cx, |v, _| v.schedule().is_err()));
    }

    #[test]
    fn press_only_grabs_on_the_bar() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| GanttView::new(tasks(), TimeFormat::Seconds, cx));
        view.update(&mut cx.clone(), |v, _| {
            let a = *v.axis();
            assert!(!v.press(1, a.x(0.5)), "empty space left of b's bar");
            assert!(v.press(1, a.x(2.5)), "on b's bar [2,3]");
            assert!(!v.press(5, a.x(2.5)), "no such row");
        });
    }

    #[test]
    fn dragging_a_pinned_task_left_neither_drifts_nor_emits() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| GanttView::new(tasks(), TimeFormat::Seconds, cx));
        let events = Rc::new(std::cell::RefCell::new(0));
        let ev = events.clone();
        let _s =
            cx.update(|cx| cx.subscribe(&view, move |_, _: &GanttEvent, _| *ev.borrow_mut() += 1));
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            assert!(v.press(1, a.x(2.5)));
            v.drag_to(a.x(0.1), cx);
            v.drag_to(a.x(-3.0), cx);
            v.release();
        });
        let nb = view.read_with(&cx, |v, _| {
            v.tasks.iter().find(|t| t.id == 2).map(|t| t.not_before)
        });
        assert_eq!(nb, Some(2.0), "clamped to predecessor finish");
        assert_eq!(*events.borrow(), 0, "start never changed");
    }

    #[test]
    fn edges_resize_and_locks_block_edits() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| GanttView::new(tasks(), TimeFormat::Seconds, cx));
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &GanttEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            // b spans [2,3]. Drag its end to 4.5.
            assert!(v.press(1, a.x(3.0)));
            v.drag_to(a.x(4.5), cx);
            v.release();
            // Drag a's start right to 0.5: a = [0.5, 2], b unchanged at 2.
            assert!(v.press(0, a.x(0.0)));
            v.drag_to(a.x(0.5), cx);
            v.release();
            // Start edge cannot cross the finish.
            assert!(v.press(0, a.x(0.5)));
            v.drag_to(a.x(9.0), cx);
            v.release();
        });
        let sc = view.read_with(&cx, |v, _| {
            v.schedule().as_ref().ok().cloned().unwrap_or_default()
        });
        let b = sc.get(&2).copied();
        assert_eq!(b.map(|b| (b.early_start, b.early_finish)), Some((2.0, 4.5)));
        let a = sc.get(&1).copied();
        assert!(
            a.is_some_and(
                |a| a.early_finish == 2.0 && (2.0 - a.early_start - MIN_DURATION).abs() < 1e-9
            ),
            "{a:?}"
        );
        assert!(matches!(
            events.borrow()[0],
            GanttEvent::Resized { id: 2, .. }
        ));

        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            v.set_locked(2, true, cx);
            assert!(!v.press(1, a.x(3.0)), "locked task");
            v.set_locked(2, false, cx);
            v.set_resize_edges(ResizeEdges::None, cx);
            assert!(
                matches!(v.grab(1, a.x(4.5)), Some((2, Grab::Move { .. }))),
                "end edge falls through to move"
            );
            v.set_editable(false, cx);
            assert!(v.grab(1, a.x(3.0)).is_none(), "global lock");
        });
    }
}
