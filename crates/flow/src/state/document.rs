//! DataUi adapter for the retained node graph.

use std::sync::Arc;

use gpui_kit::{
    App, AppContext, ElementId, Entity, IntoElement, ParentElement as _, RenderOnce, Subscription,
    Window, div,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{FlowCanvas, FlowState};
use crate::{
    Background, FlowEvent, FlowOptions,
    core::{ConnectionPolicy, Editor, Graph, Point, Viewport},
    editor_for_view, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| FlowDocument::new(node).into_any_element(),
        }],
    );
}

fn json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => (*value).into(),
        Value::Num(value)
            if (0.0..(1_u64 << 53) as f64).contains(value) && value.fract() == 0.0 =>
        {
            (*value as u64).into()
        }
        Value::Num(value)
            if (-(1_i64 << 53) as f64..0.0).contains(value) && value.fract() == 0.0 =>
        {
            (*value as i64).into()
        }
        Value::Num(value) => (*value).into(),
        Value::Str(value) => value.as_ref().into(),
        Value::Record(value) => serde_json::Value::Object(
            value
                .iter()
                .map(|(key, value)| (key.to_string(), json(value)))
                .collect(),
        ),
        Value::List(_) | Value::Sequence(_) => serde_json::Value::Array(
            (0..value.list_len().unwrap_or_default())
                .filter_map(|i| value.list_get(i))
                .map(json)
                .collect(),
        ),
    }
}

fn editor(value: &Value) -> Result<Editor, String> {
    let graph = if matches!(value, Value::Null) {
        Graph::default()
    } else {
        serde_json::from_value::<Graph>(json(value)).map_err(|error| error.to_string())?
    };
    editor_for_view(graph).map_err(|error| error.to_string())
}

fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned().into(), value))
            .collect(),
    ))
}

fn event_payload(event: &FlowEvent, state: &FlowState) -> Value {
    match event {
        FlowEvent::Changed => serde_json::to_value(state.graph())
            .ok()
            .and_then(|value| Value::item_from_json(&value).ok())
            .unwrap_or_default(),
        FlowEvent::SelectionChanged => record([
            (
                "nodes",
                Value::List(
                    state
                        .selected_nodes()
                        .iter()
                        .map(|id| Value::Num(*id as f64))
                        .collect(),
                ),
            ),
            (
                "edges",
                Value::List(
                    state
                        .selected_edges()
                        .iter()
                        .map(|id| Value::Num(*id as f64))
                        .collect(),
                ),
            ),
        ]),
        FlowEvent::ViewportChanged(view) => record([
            ("panX", Value::Num(view.pan.x.into())),
            ("panY", Value::Num(view.pan.y.into())),
            ("zoom", Value::Num(view.zoom.into())),
        ]),
        FlowEvent::Error(error) => error.to_string().into(),
    }
}

#[derive(IntoElement)]
struct FlowDocument {
    key: ElementId,
    graph: Value,
    options: FlowOptions,
    policy: ConnectionPolicy,
    viewport: Option<Viewport>,
    handlers: [Option<EventHandler>; 4],
}

impl FlowDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let flag = |i, default| node.prop(i).as_bool().unwrap_or(default);
        let n = |i, default| node.prop(i).as_f64().map_or(default, |v| v as f32);
        let grid = n(5, 20.0);
        Self {
            key: node.state_id(),
            graph: node.prop(0).clone(),
            options: FlowOptions {
                background: match node.prop(1).as_str() {
                    Some("lines") => Background::Lines,
                    Some("cross") => Background::Cross,
                    Some("none") => Background::None,
                    _ => Background::Dots,
                },
                minimap: flag(2, true),
                controls: flag(3, true),
                toolbar: flag(4, true),
                snap_grid: (grid.is_finite() && grid > 0.0).then_some(grid),
                fit_on_init: flag(6, true),
            },
            policy: ConnectionPolicy {
                allow_self: flag(7, false),
                allow_parallel: flag(8, false),
                allow_cycles: flag(9, true),
            },
            viewport: (10..=12).any(|i| node.is_set(i)).then(|| Viewport {
                pan: Point::new(n(10, 0.0), n(11, 0.0)),
                zoom: n(12, 1.0),
            }),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct FlowDocumentState {
    graph: Value,
    viewport: Option<Viewport>,
    flow: Option<(Entity<FlowState>, Subscription)>,
    invalid: Option<String>,
    handlers: [Option<EventHandler>; 4],
}

fn mount_flow(
    editor: Editor,
    options: FlowOptions,
    policy: ConnectionPolicy,
    window: &mut Window,
    cx: &mut gpui_kit::Context<FlowDocumentState>,
) -> (Entity<FlowState>, Subscription) {
    let flow = cx.new(|cx| {
        FlowState::from_editor(editor, cx)
            .options(options)
            .connection_policy(policy)
    });
    let subscription = cx.subscribe_in(
        &flow,
        window,
        |state: &mut FlowDocumentState, owner, event, window, cx| {
            let index = match event {
                FlowEvent::Changed => 0,
                FlowEvent::SelectionChanged => 1,
                FlowEvent::ViewportChanged(_) => 2,
                FlowEvent::Error(_) => 3,
            };
            if let Some(handler) = &state.handlers[index] {
                let payload = event_payload(event, owner.read(cx));
                handler(payload, window, cx);
            }
        },
    );
    (flow, subscription)
}

impl RenderOnce for FlowDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let (flow, invalid) = match editor(&self.graph) {
                Ok(editor) => (
                    Some(mount_flow(
                        editor,
                        self.options.clone(),
                        self.policy,
                        window,
                        cx,
                    )),
                    None,
                ),
                Err(error) => (None, Some(error)),
            };
            FlowDocumentState {
                graph: self.graph.clone(),
                viewport: None,
                flow,
                invalid,
                handlers: std::array::from_fn(|_| None),
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            if state.graph != self.graph {
                state.graph = self.graph;
                match editor(&state.graph) {
                    Ok(mut editor) => {
                        editor.policy = self.policy;
                        if let Some((flow, _)) = &state.flow {
                            flow.update(cx, |flow, cx| {
                                flow.cancel_layout(cx);
                                flow.editor = editor;
                                flow.last_error = None;
                                flow.sync(cx);
                            });
                        } else {
                            state.flow = Some(mount_flow(
                                editor,
                                self.options.clone(),
                                self.policy,
                                window,
                                cx,
                            ));
                            state.viewport = None;
                        }
                        state.invalid = None;
                    }
                    Err(error) => state.invalid = Some(error),
                }
            }
            if let Some(error) = &state.invalid {
                return div()
                    .child(format!("Invalid flow graph: {error}"))
                    .into_any_element();
            }
            let Some((flow, _)) = &state.flow else {
                return div().into_any_element();
            };
            flow.update(cx, |flow, cx| {
                flow.editor.policy = self.policy;
                flow.canvas.update(cx, |canvas, cx| {
                    canvas.interaction.editor.policy = self.policy;
                    if canvas.options != self.options {
                        canvas.options = self.options;
                        cx.notify();
                    }
                });
                if self.viewport != state.viewport {
                    state.viewport = self.viewport;
                    if let Some(viewport) = self.viewport
                        && let Err(error) = flow.set_viewport(viewport, cx)
                    {
                        flow.last_error = Some(error.clone());
                        cx.emit(FlowEvent::Error(error));
                    }
                }
            });
            FlowCanvas::new(flow).into_any_element()
        })
    }
}
