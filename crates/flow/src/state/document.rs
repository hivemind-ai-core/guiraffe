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

fn editor(value: &Value) -> Result<Editor, String> {
    let graph = if matches!(value, Value::Null) {
        Graph::default()
    } else {
        serde_json::from_value::<Graph>(value.to_json()).map_err(|error| error.to_string())?
    };
    editor_for_view(graph).map_err(|error| error.to_string())
}

fn arrange(flow: &mut FlowState, value: &Value, cx: &mut gpui_kit::Context<FlowState>) {
    use crate::core::{
        AutoLayout, LayoutConstraint, LayoutDirection, LayoutOptions, LayoutRouting,
    };

    if matches!(value, Value::Null) {
        flow.cancel_layout(cx);
        return;
    }
    let field = |name: &str| value.field(&[name.into()]);
    let n = |name, default| field(name).as_f64().unwrap_or(default);
    let ids = |name| {
        let values = field(name);
        (0..values.list_len().unwrap_or_default())
            .filter_map(move |index| values.list_get(index)?.as_f64().map(|value| value as u64))
            .collect()
    };
    let direction = match field("direction").as_str() {
        Some("right-to-left") => LayoutDirection::RightToLeft,
        Some("top-to-bottom") => LayoutDirection::TopToBottom,
        Some("bottom-to-top") => LayoutDirection::BottomToTop,
        _ => LayoutDirection::LeftToRight,
    };
    let routing = match field("routing").as_str() {
        Some("spline") => LayoutRouting::Spline,
        _ => LayoutRouting::Orthogonal,
    };
    let layout = match field("engine").as_str() {
        Some("tree") => AutoLayout::Tree,
        Some("radial") => AutoLayout::Radial,
        Some("grid") => AutoLayout::Grid {
            columns: n("columns", 4.).clamp(1., 1024.) as usize,
        },
        Some("force") => AutoLayout::Force {
            seed: n("seed", 0.) as u64,
            iterations: n("iterations", 80.).clamp(1., 2000.) as usize,
        },
        _ => AutoLayout::Layered,
    };
    let order = field("order");
    let constraints = field("constraints");
    let ordering = (0..order.list_len().unwrap_or_default())
        .filter_map(|index| {
            let item = order.list_get(index)?;
            let node = item.field(&["node".into()]).as_f64()? as u64;
            let priority = item.field(&["priority".into()]).as_f64()? as i32;
            Some((node, priority))
        })
        .collect();
    let constraints = (0..constraints.list_len().unwrap_or_default())
        .filter_map(|index| {
            let constraint = constraints.list_get(index)?;
            let first = constraint.field(&["first".into()]).as_f64()? as u64;
            let second = constraint.field(&["second".into()]).as_f64()? as u64;
            Some(match constraint.field(&["kind".into()]).as_str()? {
                "left-of" => LayoutConstraint::LeftOf(first, second),
                "above" => LayoutConstraint::Above(first, second),
                "align-x" => LayoutConstraint::AlignX(first, second),
                "align-y" => LayoutConstraint::AlignY(first, second),
                "same-rank" => LayoutConstraint::SameRank(first, second),
                _ => return None,
            })
        })
        .collect();
    let spacing = field("spacing");
    let options = LayoutOptions {
        direction,
        routing,
        spacing: Point::new(
            spacing.field(&["x".into()]).as_f64().unwrap_or(80.) as f32,
            spacing.field(&["y".into()]).as_f64().unwrap_or(40.) as f32,
        ),
        pinned: ids("pinned"),
        constraints,
        order: ordering,
        crossing_weight: n("crossingWeight", 1.) as f32,
        affected: field("affected").list_len().map(|_| ids("affected")),
    };
    flow.arrange(
        layout,
        options,
        std::time::Duration::from_millis(n("durationMs", 0.) as u64),
        cx,
    );
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
        FlowEvent::SelectionChanged { .. } => record([
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
    layout: Value,
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
                grid: crate::GridStyle::default(),
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
            layout: node.prop(13).clone(),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct FlowDocumentState {
    layout: Value,
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
                FlowEvent::SelectionChanged { .. } => 1,
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
                layout: Value::Null,
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
                            state.layout = Value::Null;
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
                if state.layout != self.layout {
                    state.layout = self.layout;
                    arrange(flow, &state.layout, cx);
                }
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
