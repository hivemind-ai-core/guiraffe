//! guiraffe-flow: a react-flow style node graph editor.

use std::cell::RefCell;

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement,
    SharedString, Styled, Window, component::button::Button, div, px,
};
use gpui_kit::component::Sizable;
use guiraffe_components::Text;
use guiraffe_core::ColorName;
use guiraffe_flow::{
    Background, FlowCanvas, FlowEvent, FlowOptions, FlowState,
    core::{ConnectionPolicy, Edge, EdgeType, Graph, Node, Point},
};

use super::column;
use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{Applied, Knob, KnobValues, Preview, playground_with};

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "flow",
        name: "Node graph",
        summary: "Drag nodes, connect handles, box-select, zoom and pan; undo/redo, minimap and controls. FlowState owns the document; FlowCanvas renders its viewport.",
        covers: &["FlowCanvas", "FlowState", "FlowViewport", "FlowGesture"],
        demo: flow,
    }]
}

fn graph() -> Result<Graph, guiraffe_flow::core::FlowError> {
    let edge = |id, s, t, label: &str, animated, edge_type| Edge {
        label: label.into(),
        animated,
        edge_type,
        ..Edge::new(id, s, t)
    };
    Graph::from_parts(
        [
            Node::new(1, "Webhook", Point::new(0., 140.)),
            Node::new(2, "Parse JSON", Point::new(240., 60.)),
            Node::new(3, "Validate", Point::new(240., 240.)),
            Node::new(4, "Enrich (LLM)", Point::new(500., 60.)),
            Node::new(5, "Dead letter", Point::new(500., 300.)),
            Node::new(6, "Postgres", Point::new(760., 120.)),
            Node::new(7, "Notify Slack", Point::new(760., 260.)),
        ],
        [
            edge(1, 1, 2, "payload", true, EdgeType::Bezier),
            edge(2, 1, 3, "", false, EdgeType::Bezier),
            edge(3, 2, 4, "", false, EdgeType::SmoothStep),
            edge(4, 3, 5, "invalid", false, EdgeType::Step),
            edge(5, 4, 6, "rows", true, EdgeType::Bezier),
            edge(6, 4, 7, "", false, EdgeType::Bezier),
            edge(7, 5, 7, "alert", false, EdgeType::Straight),
        ],
    )
}

/// The canvas is rebuilt when a knob changes: options are fixed at construction.
struct FlowDemo {
    state: RefCell<Option<Entity<FlowState>>>,
    error: Option<SharedString>,
    log: Entity<EventLog>,
    applied: Applied,
}

fn flow(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live node graph (drag nodes, connect handles, box-select)",
        &[
            Knob::new("Background", &["dots", "lines", "cross", "none"]),
            Knob::toggle("Minimap").default(1),
            Knob::toggle("Controls").default(1),
            Knob::toggle("Toolbar").default(1),
            Knob::new("Snap grid", &["off", "20", "40"]).default(1),
            Knob::toggle("Allow cycles").default(1),
        ],
        |_, cx| FlowDemo {
            state: RefCell::new(None),
            error: None,
            log: EventLog::new(cx),
            applied: Applied::default(),
        },
        flow_preview,
        window,
        cx,
    )
}

fn flow_preview(
    d: &FlowDemo,
    v: &KnobValues,
    _: &mut Window,
    cx: &mut Context<Preview<FlowDemo>>,
) -> AnyElement {
    if d.applied.changed(v) {
        let options = FlowOptions {
            background: v
                .pick(
                    0,
                    &[
                        Background::Dots,
                        Background::Lines,
                        Background::Cross,
                        Background::None,
                    ],
                )
                .unwrap_or_default(),
            minimap: v.on(1),
            controls: v.on(2),
            toolbar: v.on(3),
            snap_grid: v.pick(4, &[None, Some(20.), Some(40.)]).flatten(),
            ..FlowOptions::default()
        };
        let policy = ConnectionPolicy {
            allow_cycles: v.on(5),
            ..ConnectionPolicy::default()
        };
        if let Ok(editor) = graph().and_then(guiraffe_flow::core::Editor::new) {
            let state = cx.new(|cx| {
                FlowState::from_editor(editor, cx)
                    .options(options)
                    .connection_policy(policy)
            });
            EventLog::watch_with(
                &d.log,
                &state,
                |e: &FlowEvent| match e {
                    FlowEvent::ViewportChanged(_) => "ViewportChanged".into(),
                    other => format!("{other:?}").into(),
                },
                cx,
            );
            *d.state.borrow_mut() = Some(state);
        }
    }
    let state = d.state.borrow().clone();
    let body = div().h(px(400.)).overflow_hidden();
    let body = match (state.clone(), &d.error) {
        (Some(s), _) => body.child(FlowCanvas::new(&s)),
        (None, Some(e)) => body.child(Text::new(e.clone()).color(ColorName::Red)),
        (None, None) => body.child(Text::new("Graph failed to build").color(ColorName::Red)),
    };
    let button = |id: &'static str, label: &'static str, f: fn(&mut FlowState, &mut Context<FlowState>)| {
        let state = state.clone();
        Button::new(id).label(label).small().on_click(move |_, _, cx| {
            if let Some(s) = &state {
                s.update(cx, f);
            }
        })
    };
    column([
        body.into_any_element(),
        div()
            .flex()
            .gap(px(24.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("flow-undo", "Undo", |s, cx| s.undo(cx)))
                    .child(button("flow-redo", "Redo", |s, cx| s.redo(cx)))
                    .child(button("flow-fit", "Fit view", |s, cx| s.fit(cx))),
            )
            .child(div().flex_1().child(d.log.clone()))
            .into_any_element(),
    ])
}
