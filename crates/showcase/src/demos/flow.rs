//! guiraffe-flow: a react-flow style node graph editor.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, px,
};
use guiraffe_components::Text;
use guiraffe_core::ColorName;
use guiraffe_flow::{
    FlowCanvas, FlowState,
    core::{Edge, EdgeType, Graph, Node, Point},
};

use super::{column, example};
use crate::catalog::Entry;

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

struct FlowDemo {
    state: Option<Entity<FlowState>>,
    error: Option<SharedString>,
}

fn flow(_: &mut Window, cx: &mut App) -> AnyView {
    let checked = graph().and_then(guiraffe_flow::core::Editor::new);
    let demo = match checked {
        Ok(editor) => FlowDemo {
            state: Some(cx.new(|cx| FlowState::from_editor(editor, cx))),
            error: None,
        },
        Err(e) => FlowDemo {
            state: None,
            error: Some(e.to_string().into()),
        },
    };
    cx.new(|_| demo).into()
}

impl Render for FlowDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let body = div().h(px(460.)).overflow_hidden();
        let body = match (&self.state, &self.error) {
            (Some(s), _) => body.child(FlowCanvas::new(s)),
            (None, Some(e)) => body.child(Text::new(e.clone()).color(ColorName::Red)),
            (None, None) => body,
        };
        column([example("Event pipeline (animated edges carry data)", body)])
    }
}
