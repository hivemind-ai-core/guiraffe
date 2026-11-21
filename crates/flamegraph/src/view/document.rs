//! DataUi adapter for a document-backed profile.

use std::sync::Arc;

use gpui_kit::{
    App, AppContext, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{ColourScheme, FlameGraph, Orientation};
use crate::{FlameLayout, Frame, FrameClicked, FrameSource, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| FlameDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

struct Profile(Vec<ProfileFrame>);

struct ProfileFrame {
    id: u64,
    parent: Option<u64>,
    self_value: f64,
    total: f64,
    label: SharedString,
    package: SharedString,
}

impl Profile {
    fn new(frames: &Value) -> Self {
        let field = |name: &str| [SharedString::from(name.to_owned())];
        let id = field("id");
        let parent = field("parent");
        let own = field("self");
        let total = field("total");
        let label = field("label");
        let package = field("package");
        let id_value = |v: &Value| {
            v.as_f64()
                .filter(|n| *n >= 0.0 && *n < (1_u64 << 53) as f64 && n.fract() == 0.0)
                .map(|n| n as u64)
        };
        Self(
            items(frames)
                .filter_map(|frame| {
                    Some(ProfileFrame {
                        id: id_value(frame.field(&id))?,
                        parent: id_value(frame.field(&parent)),
                        self_value: frame.field(&own).as_f64()?,
                        total: frame.field(&total).as_f64()?,
                        label: frame.field(&label).display(),
                        package: frame.field(&package).display(),
                    })
                })
                .collect(),
        )
    }
}

impl FrameSource for Profile {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn frame(&self, index: usize) -> Frame<'_> {
        let entry = &self.0[index];
        Frame {
            id: entry.id,
            parent: entry.parent,
            self_value: entry.self_value,
            total: entry.total,
            label: &entry.label,
            package: &entry.package,
        }
    }
}

#[derive(IntoElement)]
struct FlameDocument {
    key: ElementId,
    frames: Value,
    orientation: Orientation,
    colour: ColourScheme,
    search: SharedString,
    on_frame: Option<EventHandler>,
}

impl FlameDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            frames: node.prop(0).clone(),
            orientation: match node.prop(1).as_str() {
                Some("icicle") => Orientation::Icicle,
                _ => Orientation::Flame,
            },
            colour: match node.prop(2).as_str() {
                Some("package") => ColourScheme::Package,
                Some("heat") => ColourScheme::Heat,
                _ => ColourScheme::Name,
            },
            search: node.prop(3).display(),
            on_frame: node.handler(0),
        }
    }
}

struct FlameState {
    graph: Entity<FlameGraph>,
    frames: Value,
    search: SharedString,
    on_frame: Option<EventHandler>,
    subscription: Subscription,
}

fn build_graph(
    frames: &Value,
    window: &mut Window,
    cx: &mut gpui_kit::Context<FlameState>,
) -> (Entity<FlameGraph>, Subscription) {
    let profile = Arc::new(Profile::new(frames));
    let graph = cx.new(|cx| {
        FlameGraph::from_source(profile, cx).unwrap_or_else(|error| {
            let mut graph =
                FlameGraph::new(Arc::new(FlameLayout::empty(Arc::new(Profile(Vec::new())))));
            graph.load_failure = Some(error);
            graph
        })
    });
    let subscription = cx.subscribe_in(
        &graph,
        window,
        |state: &mut FlameState, _, event: &FrameClicked, window, cx| {
            if let Some(handler) = &state.on_frame {
                let value = Value::Record(Arc::new(
                    [
                        ("id".into(), Value::Num(event.id as f64)),
                        (
                            "path".into(),
                            Value::List(
                                event.path.iter().map(|id| Value::Num(*id as f64)).collect(),
                            ),
                        ),
                        ("value".into(), Value::Num(event.value)),
                    ]
                    .into(),
                ));
                handler(value, window, cx);
            }
        },
    );
    (graph, subscription)
}

impl RenderOnce for FlameDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let (graph, subscription) = build_graph(&self.frames, window, cx);
            FlameState {
                graph,
                frames: self.frames.clone(),
                search: SharedString::default(),
                on_frame: None,
                subscription,
            }
        });
        state.update(cx, |state, cx| {
            let changed = state.frames != self.frames;
            if changed {
                state.frames = self.frames;
                (state.graph, state.subscription) = build_graph(&state.frames, window, cx);
            }
            state.on_frame = self.on_frame;
            let search_changed = state.search != self.search;
            state.search = self.search;
            state.graph.update(cx, |graph, cx| {
                if graph.orientation != self.orientation {
                    graph.set_orientation(self.orientation, cx);
                }
                if graph.colour != self.colour {
                    graph.set_colour_scheme(self.colour, cx);
                }
                if changed || search_changed {
                    graph.set_search(&state.search, cx);
                }
            });
            state.graph.clone()
        })
    }
}
