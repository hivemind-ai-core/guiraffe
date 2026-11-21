//! Document adapter for the dependency-based schedule.

use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{GanttEvent, GanttView};
use crate::{LockStyle, ResizeEdges, Task, TimeFormat, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[1],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
            render: |node, _, _, _| GanttDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn id(value: &Value) -> Option<u32> {
    value
        .as_f64()
        .filter(|number| (0.0..=f64::from(u32::MAX)).contains(number) && number.fract() == 0.0)
        .map(|number| number as u32)
}

fn parse_tasks(values: &Value) -> Vec<Task> {
    let id_field = ["id".into()];
    let name = ["name".into()];
    let duration = ["duration".into()];
    let not_before = ["notBefore".into()];
    let after = ["after".into()];
    items(values)
        .filter_map(|value| {
            let duration = value.field(&duration).as_f64()?;
            let not_before = value.field(&not_before).as_f64().unwrap_or_default();
            (duration.is_finite() && duration > 0.0 && (not_before + duration).is_finite())
                .then_some(Task {
                    id: id(value.field(&id_field))?,
                    name: value.field(&name).as_str()?.to_owned(),
                    duration,
                    not_before,
                    after: items(value.field(&after)).filter_map(id).collect(),
                })
        })
        .collect()
}

fn style(value: &Value, default: LockStyle) -> LockStyle {
    match value.as_str() {
        Some("none") => LockStyle::None,
        Some("badge") => LockStyle::Badge,
        Some("dimmed") => LockStyle::Dimmed,
        Some("hatched") => LockStyle::Hatched,
        _ => default,
    }
}

#[derive(IntoElement)]
struct GanttDocument {
    key: ElementId,
    tasks: Value,
    format: TimeFormat,
    resize: ResizeEdges,
    editable: bool,
    focusable: Option<bool>,
    key_step: Option<f64>,
    lock_style: LockStyle,
    global_lock_style: LockStyle,
    locked: Value,
    selected: Option<u32>,
    handlers: [Option<EventHandler>; 2],
}

impl GanttDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            tasks: node.prop(0).clone(),
            format: match node.prop(1).as_str() {
                Some("frames") => {
                    TimeFormat::Frames(node.prop(2).as_f64().unwrap_or(30.0).clamp(1.0, 240.0))
                }
                Some("seconds") => TimeFormat::Seconds,
                _ => TimeFormat::Days,
            },
            resize: match node.prop(3).as_str() {
                Some("none") => ResizeEdges::None,
                Some("start") => ResizeEdges::Start,
                Some("end") => ResizeEdges::End,
                _ => ResizeEdges::Both,
            },
            editable: node.prop(4).as_bool().unwrap_or(true),
            focusable: node.prop(5).as_bool(),
            key_step: node.prop(6).as_f64(),
            lock_style: style(node.prop(7), LockStyle::Badge),
            global_lock_style: style(node.prop(8), LockStyle::None),
            locked: node.prop(9).clone(),
            selected: id(node.prop(10)),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct GanttState {
    view: Entity<GanttView>,
    tasks: Value,
    locked: Value,
    selected: Option<u32>,
    handlers: [Option<EventHandler>; 2],
    _subscription: Subscription,
}

impl RenderOnce for GanttDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let view = GanttView::new(parse_tasks(&self.tasks), self.format, cx);
            let subscription = cx.subscribe_in(
                &view,
                window,
                |state: &mut GanttState, _, event, window, cx| {
                    let (event_index, id, not_before, duration) = match *event {
                        GanttEvent::Rescheduled { id, not_before } => (0, id, not_before, None),
                        GanttEvent::Resized {
                            id,
                            not_before,
                            duration,
                        } => (1, id, not_before, Some(duration)),
                    };
                    if let Some(handler) = &state.handlers[event_index] {
                        let mut payload = BTreeMap::from([
                            ("id".into(), Value::Num(f64::from(id))),
                            ("notBefore".into(), Value::Num(not_before)),
                        ]);
                        if let Some(duration) = duration {
                            payload.insert("duration".into(), Value::Num(duration));
                        }
                        handler(Value::Record(Arc::new(payload)), window, cx);
                    }
                },
            );
            GanttState {
                view,
                tasks: self.tasks.clone(),
                locked: Value::Null,
                selected: None,
                handlers: [None, None],
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            state.view.update(cx, |view, cx| {
                if state.tasks != self.tasks {
                    state.tasks = self.tasks;
                    view.set_tasks(parse_tasks(&state.tasks), cx);
                    view.select(view.selected(), cx);
                }
                if view.format != self.format {
                    view.format = self.format;
                    view.surface = crate::PlotSurface::timeline(*view.axis(), self.format);
                    cx.notify();
                }
                if view.resize != self.resize {
                    view.set_resize_edges(self.resize, cx);
                }
                view.set_editable(self.editable, cx);
                if view.focusable != self.focusable {
                    view.set_focusable(self.focusable, cx);
                }
                view.set_key_step(self.key_step);
                if view.lock_style != self.lock_style
                    || view.global_lock_style != self.global_lock_style
                {
                    view.set_lock_style(self.lock_style, self.global_lock_style, cx);
                }
                if state.locked != self.locked {
                    state.locked = self.locked;
                    view.locked = Rc::new(items(&state.locked).filter_map(id).collect());
                    cx.notify();
                }
                if self.selected != state.selected {
                    state.selected = self.selected;
                    view.select(self.selected, cx);
                }
            });
            state.view.clone()
        })
    }
}
