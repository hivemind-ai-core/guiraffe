//! The Gantt chart view: ruler, task bars, dependency arrows, critical path, drag to
//! reschedule, wheel scroll, shift+wheel pan and ctrl+wheel zoom.

use std::{collections::HashMap, rc::Rc};

use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, PathBuilder, Pixels, Render, Role,
    ScrollEvent, Styled, TestSupportExt, Window, canvas, div, point, prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, StateKind, Variant, focus_ring, ring_visible};

use crate::{
    axis::{Axis, TimeFormat, label},
    clips::Edge,
    edit::{
        EndEarlier, EndLater, Grab, KEY_CONTEXT, MoveEarlier, MoveLater, Nudge, ResizeEdges,
        SelectNextItem, SelectPrevItem, StartEarlier, StartLater, cursor_for, grab_at, key_step,
    },
    gantt::{ScheduleError, SchedulePlan, Scheduled, Task, task_index},
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

/// Static display metadata and dependency adjacency, rebuilt only on task replacement.
struct Geometry {
    rows: Vec<Row>,
    plan: Result<SchedulePlan, ScheduleError>,
    index: HashMap<u32, usize>,
}

struct Row {
    id: u32,
    name: gpui_kit::SharedString,
}

fn geometry(tasks: &[Task]) -> Rc<Geometry> {
    let index = task_index(tasks);
    let plan = SchedulePlan::new(tasks, &index);
    Rc::new(Geometry {
        rows: tasks
            .iter()
            .map(|t| Row {
                id: t.id,
                name: t.name.clone().into(),
            })
            .collect(),
        plan,
        index,
    })
}

pub struct GanttView {
    geometry: Rc<Geometry>,
    tasks: Vec<Task>,
    schedule: Rc<Result<HashMap<u32, Scheduled>, ScheduleError>>,
    surface: crate::PlotSurface,
    format: TimeFormat,
    lock_style: crate::LockStyle,
    global_lock_style: crate::LockStyle,
    drag: Option<Drag>,
    body: Rc<std::cell::Cell<Bounds<Pixels>>>,
    scroll_y: f64,
    /// Range to fit once the real width is known.
    pending_fit: Option<(f64, f64)>,
    resize: ResizeEdges,
    /// Tasks that cannot be moved or resized.
    locked: Rc<std::collections::HashSet<u32>>,
    /// When false, nothing can be edited.
    editable: bool,
    /// What the pointer is over, for the cursor.
    hover: Option<Grab>,
    /// The selected task (click a bar, or ↑/↓ when focused); keyboard edits act on it.
    selected: Option<u32>,
    focus: FocusHandle,
    /// `None`: a tab stop only inside a focus scope.
    focusable: Option<bool>,
    /// Keyboard step in seconds (`None`: a quarter of the ruler tick).
    key_step: Option<f64>,
}

impl EventEmitter<GanttEvent> for GanttView {}

impl Focusable for GanttView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl GanttView {
    pub fn new(tasks: Vec<Task>, format: TimeFormat, cx: &mut App) -> Entity<Self> {
        let geometry = geometry(&tasks);
        let schedule = geometry.schedule(&tasks);
        let (lo, hi) = span(&schedule);
        let axis = Axis::new(lo, hi, 800.0);
        cx.new(|cx| Self {
            geometry,
            tasks,
            schedule: Rc::new(schedule),
            surface: crate::PlotSurface::timeline(axis, format),
            format,
            lock_style: crate::LockStyle::Badge,
            global_lock_style: crate::LockStyle::None,
            drag: None,
            body: Rc::default(),
            scroll_y: 0.0,
            pending_fit: Some((lo, hi)),
            resize: ResizeEdges::default(),
            locked: Default::default(),
            editable: true,
            hover: None,
            selected: None,
            focus: cx.focus_handle(),
            focusable: None,
            key_step: None,
        })
    }

    /// The selected task id.
    pub fn selected(&self) -> Option<u32> {
        self.selected
    }

    pub fn select(&mut self, id: Option<u32>, cx: &mut Context<Self>) {
        let id = id.filter(|id| self.geometry.index.contains_key(id));
        if self.selected != id {
            self.selected = id;
            cx.notify();
        }
    }

    /// `Some(true)` enables Tab and focus on item clicks. `None` follows the
    /// enclosing Tab scope and preserves focus on clicks. `Some(false)` opts out.
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = v;
        cx.notify();
    }

    /// Keyboard step in seconds (`None`: a quarter of the ruler tick).
    pub fn set_key_step(&mut self, step: Option<f64>) {
        self.key_step = step.filter(|s| s.is_finite() && *s > 0.0);
    }

    /// Keyboard edit of the selected task by `dir` steps (no-op when locked / not editable).
    pub fn nudge(&mut self, what: Nudge, dir: f64, cx: &mut Context<Self>) {
        let Some(id) = self.selected else { return };
        if !self.editable || self.locked.contains(&id) {
            return;
        }
        let Some(sc) = self
            .schedule()
            .as_ref()
            .ok()
            .and_then(|m| m.get(&id))
            .copied()
        else {
            return;
        };
        let d = dir
            * key_step(
                self.key_step,
                self.surface.axis.tick_step(90.0, self.format),
            );
        match what {
            Nudge::Move => self.reschedule(id, sc.early_start + d, cx),
            Nudge::End => self.resize_edge(id, Edge::End, sc.early_finish + d, cx),
            Nudge::Start => self.resize_edge(id, Edge::Start, sc.early_start + d, cx),
        }
    }

    fn select_step(&mut self, dir: i32, cx: &mut Context<Self>) {
        let cur = self
            .selected
            .and_then(|id| self.geometry.index.get(&id).copied());
        let next = crate::edit::cycle(cur, self.tasks.len(), dir);
        if let Some(row) = next {
            let top = row_top(row);
            let height = f64::from(f32::from(self.body.get().size.height).max(ROW_H));
            let offset = self
                .scroll_y
                .clamp((top + f64::from(ROW_H) - height).max(0.0), top);
            if offset != self.scroll_y {
                self.scroll_y = offset;
                cx.notify();
            }
        }
        self.select(next.and_then(|i| self.tasks.get(i)).map(|t| t.id), cx);
    }

    pub fn schedule(&self) -> &Result<HashMap<u32, Scheduled>, ScheduleError> {
        &self.schedule
    }

    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn set_lock_style(
        &mut self,
        item: crate::LockStyle,
        global: crate::LockStyle,
        cx: &mut Context<Self>,
    ) {
        self.lock_style = item;
        self.global_lock_style = global;
        cx.notify();
    }
    pub fn axis(&self) -> &Axis {
        &self.surface.axis
    }

    pub fn set_tasks(&mut self, tasks: Vec<Task>, cx: &mut Context<Self>) {
        self.tasks = tasks;
        self.geometry = geometry(&self.tasks);
        self.schedule = Rc::new(self.geometry.schedule(&self.tasks));
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
        let Some(&row) = self.geometry.index.get(&id) else {
            return;
        };
        let t = &mut self.tasks[row];
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
        self.schedule = Rc::new(self.geometry.schedule(&self.tasks));
        if before != start(&self.schedule) {
            cx.emit(GanttEvent::Rescheduled { id, not_before });
        }
        cx.notify();
    }

    /// Latest early finish among `id`'s predecessors (−∞ if none or unscheduled).
    fn predecessor_bound(&self, id: u32) -> f64 {
        let (Ok(m), Some(t)) = (
            self.schedule().as_ref(),
            self.geometry
                .index
                .get(&id)
                .and_then(|&i| self.tasks.get(i)),
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
        let sc = self.schedule().as_ref().ok()?.get(&task.id)?;
        let x0 = self.surface.axis.x(sc.early_start);
        let x1 = self.surface.axis.x(sc.early_finish).max(x0 + 2.0);
        grab_at(
            x0,
            x1,
            x,
            self.surface.axis.t(x),
            sc.early_start,
            self.resize,
        )
        .map(|g| (task.id, g))
    }

    /// Begin a drag at body-local pixel `x` on `row`; only a press on an editable bar grabs.
    pub fn press(&mut self, row: usize, x: f64) -> bool {
        self.press_item(row, x);
        self.drag.is_some()
    }

    fn press_item(&mut self, row: usize, x: f64) -> bool {
        let mut hit = false;
        if let Some(task) = self.tasks.get(row)
            && let Some(sc) = self.schedule().as_ref().ok().and_then(|m| m.get(&task.id))
        {
            let (x0, x1) = (
                self.surface.axis.x(sc.early_start),
                self.surface.axis.x(sc.early_finish),
            );
            if x >= x0 && x <= x1.max(x0 + 2.0) {
                self.selected = Some(task.id);
                hit = true;
            }
        }
        self.drag = self.grab(row, x).map(|(id, grab)| Drag { id, grab });
        hit
    }

    /// Continue the active drag to body-local pixel `x`.
    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some(d) = self.drag else {
            return;
        };
        let t = self.surface.axis.t(x);
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
            .schedule()
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
        let Some(&row) = self.geometry.index.get(&id) else {
            return;
        };
        let task = &mut self.tasks[row];
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
        self.schedule = Rc::new(self.geometry.schedule(&self.tasks));
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
            Rc::make_mut(&mut self.locked).insert(id)
        } else {
            Rc::make_mut(&mut self.locked).remove(&id)
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

    fn max_scroll(&self) -> f64 {
        (row_top(self.tasks.len()) - f64::from(f32::from(self.body.get().size.height))).max(0.0)
    }

    fn on_wheel(&mut self, e: &ScrollEvent, cx: &mut Context<Self>) {
        let d = e.delta.pixel_delta(px(16.0));
        let b = self.body.get();
        let x = f64::from(f32::from(e.position.x - b.origin.x));
        let zoom = e.modifiers.control || e.modifiers.platform;
        let vertical =
            !zoom && !e.modifiers.shift && f32::from(d.y) != 0.0 && self.max_scroll() > 0.0;
        if vertical {
            let next = (self.scroll_y - f64::from(f32::from(d.y))).clamp(0.0, self.max_scroll());
            if self.scroll_y != next {
                self.scroll_y = next;
                cx.notify();
            }
        }
        let changed = self.surface.navigate(
            x,
            f64::from(f32::from(d.x)),
            if vertical {
                0.0
            } else {
                f64::from(f32::from(d.y))
            },
            zoom,
        );
        if changed {
            cx.notify();
        }
    }
}

fn bar_interval(axis: Axis, scheduled: Scheduled) -> Option<(f32, f32)> {
    let start = axis.x(scheduled.early_start) as f32;
    let end = axis.x(scheduled.early_finish) as f32;
    // Preserve a two-pixel milestone when the interval projects below one pixel.
    let width = (end - start).max(2.0);
    (start.is_finite() && width.is_finite() && start + width > 0.0 && start < axis.width as f32)
        .then_some((start, width))
}

fn row_top(row: usize) -> f64 {
    row as f64 * f64::from(ROW_H)
}

fn visible_rows(count: usize, offset: f64, height: f32) -> std::ops::Range<usize> {
    let first = (offset / f64::from(ROW_H)) as usize;
    let end = ((offset + f64::from(height)) / f64::from(ROW_H)).ceil() as usize;
    first.min(count)..end.min(count)
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

impl Geometry {
    fn schedule(&self, tasks: &[Task]) -> Result<HashMap<u32, Scheduled>, ScheduleError> {
        self.plan
            .as_ref()
            .map(|plan| plan.schedule(tasks))
            .map_err(Clone::clone)
    }

    fn visible_arrows(
        &self,
        schedule: &HashMap<u32, Scheduled>,
        axis: Axis,
        scroll_y: f64,
        height: f32,
        mut draw: impl FnMut((f32, f32), (f32, f32)),
    ) {
        let Ok(plan) = &self.plan else {
            return;
        };
        let row_y = |row: usize| (row_top(row) - scroll_y) as f32 + ROW_H / 2.0;
        let visible = |x: f32, y: f32| x >= 0.0 && x < axis.width as f32 && y >= 0.0 && y < height;
        for row in visible_rows(self.rows.len(), scroll_y, height) {
            let Some(sc) = schedule.get(&self.rows[row].id) else {
                continue;
            };
            let y = row_y(row);
            let finish = axis.x(sc.early_finish) as f32;
            if visible(finish, y) {
                for &to in plan.successors.get(row) {
                    if let Some(target) = schedule.get(&self.rows[to].id) {
                        let start = axis.x(target.early_start) as f32;
                        if start.is_finite() {
                            draw((finish, y), (start, row_y(to)));
                        }
                    }
                }
            }
            let start = axis.x(sc.early_start) as f32;
            if visible(start, y) {
                for &from in plan.predecessors.get(row) {
                    if let Some(source) = schedule.get(&self.rows[from].id) {
                        let finish = axis.x(source.early_finish) as f32;
                        let source_y = row_y(from);
                        // A visible source already produced this edge.
                        if finish.is_finite() && !visible(finish, source_y) {
                            draw((finish, source_y), (start, y));
                        }
                    }
                }
            }
        }
    }
}

fn paint_bar(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    fill: Hsla,
    border: Hsla,
    selection: Option<Hsla>,
) {
    let corners = gpui_kit::Corners::all(px(4.0)).clamp_radii_for_quad_size(bounds.size);
    let border_style = gpui_kit::BorderStyle::Solid;
    window.paint_quad(gpui_kit::quad(
        bounds,
        corners,
        fill,
        gpui_kit::Edges::default(),
        gpui_kit::transparent_black(),
        border_style,
    ));
    // Match the styled bar: draw the selection overlay inside the bar's outline.
    if let Some(selection) = selection {
        window.paint_quad(gpui_kit::quad(
            bounds,
            corners,
            gpui_kit::transparent_black(),
            gpui_kit::Edges::all(px(2.0)),
            selection,
            border_style,
        ));
    }
    window.paint_quad(gpui_kit::quad(
        bounds,
        corners,
        gpui_kit::transparent_black(),
        gpui_kit::Edges::all(px(1.0)),
        border,
        border_style,
    ));
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let normal = t.variant(None, Variant::Light);
        let critical = t.variant(Some(ColorName::Red), Variant::Filled);
        let error_fg = t.variant(Some(ColorName::Red), Variant::Light).foreground;
        let ring = t.state(StateKind::FocusRing, Variant::Light, None).border;
        let ring_on = ring_visible(&self.focus, window);
        let tab = guiraffe_core::tab_stop(self.focusable);
        let selected_row = self
            .selected
            .and_then(|id| self.geometry.index.get(&id).copied());
        self.scroll_y = self.scroll_y.min(self.max_scroll());
        let scroll_y = self.scroll_y;
        let height = if self.body.get().size.height > px(0.0) {
            f32::from(self.body.get().size.height)
        } else {
            (f32::from(window.viewport_size().height) - RULER_H).max(0.0)
        };
        let visible = visible_rows(self.tasks.len(), scroll_y, height);
        let header_rows = visible.clone();
        let bar_rows = visible.clone();
        let row_count = self.tasks.len();
        if let Some(w) = crate::measure::width(&self.body) {
            self.surface.axis.set_width(w);
            if let Some((a, b)) = self.pending_fit.take() {
                self.surface.fit(a, b, 0.05);
            }
        }
        let axis = self.surface.axis;
        let format = self.format;

        let error = self.schedule().as_ref().err().map(|e| format!("{e:?}"));
        let geo = self.geometry.clone();
        let schedule = self.schedule.clone();
        let paint_schedule = schedule.clone();
        let scheduled = self.schedule().as_ref().ok();
        let editable = self.editable;
        let item_style = self.lock_style;
        let global_style = self.global_lock_style;
        let locked_ids = &self.locked;
        let paint_locks = self.locked.clone();
        let paint_geo = geo.clone();
        let paint_rows = visible.clone();
        let selected_color = s.text.to_hsla();
        let lock_style = move |locked: bool| {
            if locked {
                item_style
            } else if !editable {
                global_style
            } else {
                crate::LockStyle::None
            }
        };
        let body = self.body.clone();
        let owner = cx.entity_id();
        let line = s.border.to_hsla();
        let arrow_color = s.dimmed.to_hsla();
        let root = div()
            .id("gantt")
            .test_support()
            .role(Role::ListBox)
            .aria_label("Gantt chart")
            .track_focus(&self.focus.clone().tab_stop(tab))
            .key_context(KEY_CONTEXT)
            .on_action(
                cx.listener(|this, _: &MoveEarlier, _, cx| this.nudge(Nudge::Move, -1.0, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveLater, _, cx| this.nudge(Nudge::Move, 1.0, cx)))
            .on_action(cx.listener(|this, _: &EndEarlier, _, cx| this.nudge(Nudge::End, -1.0, cx)))
            .on_action(cx.listener(|this, _: &EndLater, _, cx| this.nudge(Nudge::End, 1.0, cx)))
            .on_action(
                cx.listener(|this, _: &StartEarlier, _, cx| this.nudge(Nudge::Start, -1.0, cx)),
            )
            .on_action(cx.listener(|this, _: &StartLater, _, cx| this.nudge(Nudge::Start, 1.0, cx)))
            .on_action(cx.listener(|this, _: &SelectPrevItem, _, cx| this.select_step(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNextItem, _, cx| this.select_step(1, cx)))
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
                    .flex_none()
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
                            .child(self.surface.element(t.clone(), true, f64::from(ROW_H), 0.)),
                    ),
            )
            .when_some(error, |el, e| {
                el.child(div().p(px(8.)).text_color(error_fg.to_hsla()).child(e))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| this.on_wheel(e, cx)))
                    .child(
                        div()
                            .relative()
                            .overflow_hidden()
                            .w(px(LABEL_W))
                            .flex_none()
                            .border_r_1()
                            .border_color(line)
                            .children(header_rows.map(|i| {
                                let Row { id, name } = &geo.rows[i];
                                let id = *id;
                                let locked = self.locked.contains(&id);
                                let crit = scheduled
                                    .and_then(|m| m.get(&id))
                                    .is_some_and(Scheduled::critical);
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .top(px((row_top(i) - scroll_y) as f32))
                                    .h(px(ROW_H))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(line)
                                    .when(crit, |el| el.text_color(critical.background.to_hsla()))
                                    .child(crate::locks::probe(
                                        "task-lock",
                                        i,
                                        gpui_kit::component::button::Button::new(("task-lock", i))
                                            .icon(gpui_kit::component::Icon::new(if locked {
                                                gpui_kit::assets::IconName::Lock
                                            } else {
                                                gpui_kit::assets::IconName::LockOpen
                                            }))
                                            .tab_stop(false)
                                            .tooltip("Toggle task lock")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.set_locked(id, !this.is_locked(id), cx)
                                            })),
                                    ))
                                    .child(name.clone())
                            })),
                    )
                    .child(
                        div()
                            .id("gantt-body")
                            .debug_selector(|| "gantt-body".into())
                            .cursor(cursor_for(self.drag.map(|d| d.grab).or(self.hover)))
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                                    // Claim focus only for a clicked item in an explicitly opted-in editor.
                                    window.prevent_default();
                                    let selected = this.selected;
                                    let cursor =
                                        cursor_for(this.drag.map(|d| d.grab).or(this.hover));
                                    let b = this.body.get();
                                    let y = f32::from(e.position.y - b.origin.y);
                                    if y >= 0.0 {
                                        let x = f64::from(f32::from(e.position.x - b.origin.x));
                                        let hit = this.press_item(
                                            ((f64::from(y) + this.scroll_y) / f64::from(ROW_H))
                                                as usize,
                                            x,
                                        );
                                        if hit && this.focusable == Some(true) {
                                            this.focus.focus(window, cx);
                                        }
                                        if selected != this.selected
                                            || cursor
                                                != cursor_for(
                                                    this.drag.map(|d| d.grab).or(this.hover),
                                                )
                                        {
                                            cx.notify();
                                        }
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
                                    this.hover_at(
                                        ((f64::from(y) + this.scroll_y) / f64::from(ROW_H))
                                            as usize,
                                        x,
                                        cx,
                                    );
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
                            .child(self.surface.element(
                                t.clone(),
                                false,
                                f64::from(ROW_H),
                                scroll_y,
                            ))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |b, _, window, _| {
                                        let Ok(schedule) = paint_schedule.as_ref() else {
                                            return;
                                        };
                                        for i in paint_rows {
                                            let row = &paint_geo.rows[i];
                                            let Some(&sc) = schedule.get(&row.id) else {
                                                continue;
                                            };
                                            if lock_style(paint_locks.contains(&row.id))
                                                != crate::LockStyle::None
                                            {
                                                continue;
                                            }
                                            let Some((x, width)) = bar_interval(axis, sc) else {
                                                continue;
                                            };
                                            let bounds = Bounds {
                                                origin: b.origin
                                                    + point(
                                                        px(x),
                                                        px((row_top(i) - scroll_y) as f32 + 6.0),
                                                    ),
                                                size: gpui_kit::size(px(width), px(ROW_H - 12.0)),
                                            };
                                            let colors =
                                                if sc.critical() { critical } else { normal };
                                            paint_bar(
                                                window,
                                                bounds,
                                                colors.background.to_hsla(),
                                                colors.foreground.to_hsla(),
                                                (selected_row == Some(i)).then_some(selected_color),
                                            );
                                        }
                                    },
                                )
                                .absolute()
                                .size_full(),
                            )
                            .children(bar_rows.filter_map(|i| {
                                let Row { id, name } = &geo.rows[i];
                                let sc = *scheduled?.get(id)?;
                                let (x, width) = bar_interval(axis, sc)?;
                                let colors = if sc.critical() { critical } else { normal };
                                let on = selected_row == Some(i);
                                let lock = lock_style(locked_ids.contains(id));
                                Some(
                                    div()
                                        .id(("gantt-bar", i))
                                        .test_support()
                                        .role(Role::ListBoxOption)
                                        .aria_label(format!(
                                            "{name}: {} – {}",
                                            label(sc.early_start, format),
                                            label(sc.early_finish, format)
                                        ))
                                        .aria_selected(on)
                                        .aria_position_in_set(i + 1)
                                        .aria_size_of_set(row_count)
                                        .absolute()
                                        .top(px((row_top(i) - scroll_y) as f32 + 6.0))
                                        .left(px(x))
                                        .w(px(width))
                                        .h(px(ROW_H - 12.0))
                                        .when(lock != crate::LockStyle::None, |el| {
                                            let el = lock.decorate(
                                                el.rounded(px(4.))
                                                    .border_1()
                                                    .bg(colors.background.to_hsla())
                                                    .border_color(colors.foreground.to_hsla()),
                                                t,
                                            );
                                            el.when(on, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .inset_0()
                                                        .rounded(px(4.))
                                                        .border_2()
                                                        .border_color(selected_color),
                                                )
                                            })
                                        }),
                                )
                            }))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |b, _, window, _| {
                                        let Ok(schedule) = schedule.as_ref() else {
                                            return;
                                        };
                                        geo.visible_arrows(
                                            schedule,
                                            axis,
                                            scroll_y,
                                            f32::from(b.size.height),
                                            |(x0, y0), (x1, y1)| {
                                                let p0 = b.origin + point(px(x0), px(y0));
                                                let p1 = b.origin + point(px(x1), px(y1));
                                                arrow(window, p0, p1, arrow_color);
                                            },
                                        );
                                    },
                                )
                                .absolute()
                                .size_full(),
                            ),
                    ),
            );
        focus_ring(root, ring_on, px(0.), ring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_rows_are_independent_of_total_rows() {
        assert_eq!(visible_rows(10_000, 45.0, 640.0), 1..22);
        assert_eq!(visible_rows(10_000_000, 45.0, 640.0), 1..22);
        assert_eq!(visible_rows(5, 128.0, 640.0), 4..5);
        let deep_offset = row_top(9_000_000) + 31.0;
        assert_eq!(
            visible_rows(10_000_000, deep_offset, 2.0),
            9_000_000..9_000_002
        );
    }

    #[test]
    fn bars_cull_outside_time_view_and_keep_subpixel_markers() {
        let axis = Axis::new(0.0, 100.0, 100.0);
        let bar = |start, finish| Scheduled {
            early_start: start,
            early_finish: finish,
            late_start: start,
            late_finish: finish,
        };
        assert_eq!(bar_interval(axis, bar(-12.0, -1.0)), None);
        assert_eq!(bar_interval(axis, bar(100.0, 102.0)), None);
        assert_eq!(bar_interval(axis, bar(-12.0, 8.0)), Some((-12.0, 20.0)));
        assert_eq!(bar_interval(axis, bar(10.0, 10.01)), Some((10.0, 2.0)));
    }

    #[test]
    fn arrows_include_only_incident_edges_and_emit_shared_edges_once() {
        let visible = |n| {
            let tasks: Vec<_> = (0..n)
                .map(|id: u32| Task {
                    id,
                    name: String::new(),
                    not_before: 0.0,
                    duration: 0.0,
                    after: id.checked_sub(1).into_iter().collect(),
                })
                .collect();
            let geometry = geometry(&tasks);
            let schedule = geometry.schedule(&tasks).expect("acyclic chain");
            let mut edges = Vec::new();
            geometry.visible_arrows(
                &schedule,
                Axis::new(-1.0, 1.0, 100.0),
                f64::from(ROW_H),
                ROW_H * 2.0,
                |(_, from), (_, to)| edges.push((from, to)),
            );
            edges
        };
        let expected = vec![(16.0, 48.0), (-16.0, 16.0), (48.0, 80.0)];
        assert_eq!(visible(10_000), expected);
        assert_eq!(visible(100_000), expected);
    }

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
