//! Retained node entities, with GPUI paint reuse when layout and state match.
use crate::NodeRenderer;
use gpui_kit::{prelude::*, *};
use guiraffe_core::GuiraffeTheme;
use guiraffe_flow_core::{Node, NodeFrame, Point as WorldPoint, PortRing, Rect, Scene};
use std::rc::Rc;
pub(crate) fn handle_bounds(centre: WorldPoint, zoom: f32) -> Rect {
    let radius = crate::HANDLE_RADIUS * zoom;
    let centre = centre * zoom;
    Rect::new(
        centre.x - radius,
        centre.y - radius,
        2. * radius,
        2. * radius,
    )
}

/// Which of `node`'s ports have an edge, as bits by handle index (the first 64). Only
/// rings show it, so a node without ring ports costs nothing.
pub(crate) fn connected_ports(scene: &Scene, node: &Node) -> u64 {
    if !node.handles.iter().any(|h| h.ring == PortRing::Ring) {
        return 0;
    }
    let graph = scene.graph();
    let mut mask = 0u64;
    for id in scene.incident_edges(node.id) {
        let Some(edge) = graph.edges.get(id) else {
            continue;
        };
        for end in [&edge.source, &edge.target] {
            if end.node != node.id {
                continue;
            }
            if let Some(index) = node.handles.iter().position(|h| h.id == end.handle)
                && index < 64
            {
                mask |= 1 << index;
            }
        }
    }
    mask
}

pub(crate) struct NodeCard {
    pub node: Node,
    /// Connected ports, from [`connected_ports`].
    pub connected: u64,
    pub revision: u64,
    pub zoom: f32,
    pub renderer: Rc<dyn NodeRenderer>,
}
impl Render for NodeCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        render_card(
            &self.node,
            self.connected,
            self.zoom,
            self.renderer.as_ref(),
            None,
            window,
            cx,
        )
    }
}

pub(crate) fn render_card(
    node: &Node,
    connected: u64,
    zoom: f32,
    renderer: &dyn NodeRenderer,
    style: Option<StyleRefinement>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    let accent = theme.primary_color();
    let radius = theme.default_radius.radius();
    let palette = handle_palette(node, theme);
    // The border of a node is painted over the children of the node, so
    // the handles are children of a frame around the border: the handles
    // are then painted above the border and sit whole on it.
    let mut frame = div().id(("flow-node", node.id)).relative().size_full();
    if let Some(style) = style {
        *frame.style() = style;
    }
    let body = div().size_full();
    let body = match node.frame {
        NodeFrame::None => body,
        shape => {
            let body =
                body.bg(surfaces.surface.to_hsla())
                    .border_1()
                    .border_color(if node.selected {
                        accent.to_hsla()
                    } else {
                        surfaces.border.to_hsla()
                    });
            if shape == NodeFrame::Pill {
                body.rounded_full()
            } else {
                body.rounded(px(radius * zoom))
            }
        }
    };
    let mut frame = frame
        .test_support()
        .child(body.child(renderer.render(node, zoom, window, cx)));
    for (index, handle) in node.handles.iter().enumerate() {
        let bounds = handle_bounds(
            handle.position(Rect {
                origin: WorldPoint::default(),
                size: node.size,
            }),
            zoom,
        );
        let id = match handle.id.as_str() {
            "in" => SharedString::new_static("handle-in"),
            "out" => SharedString::new_static("handle-out"),
            id => SharedString::from(format!("handle-{id}")),
        };
        let ink = palette
            .as_ref()
            .and_then(|palette| crate::ink::resolve(palette, handle.color))
            .unwrap_or(accent)
            .to_hsla();
        // The dot is centred on the border of the node; a ring keeps the same hit target.
        let port = div()
            .id(id)
            .test_support()
            .absolute()
            .left(px(bounds.origin.x))
            .top(px(bounds.origin.y))
            .size(px(bounds.size.x))
            .rounded_full();
        let port = match handle.ring {
            PortRing::Dot => port.bg(ink),
            PortRing::Ring if index < 64 && connected & (1 << index) != 0 => port.bg(ink),
            PortRing::Ring => {
                let port = if zoom >= 1.25 {
                    port.border_2()
                } else {
                    port.border_1()
                };
                port.bg(surfaces.surface.to_hsla())
                    .border_color(ink)
                    .hover(move |style| style.bg(ink))
            }
        };
        let port = match &handle.tooltip {
            Some(tip) => {
                let tip = SharedString::from(tip.as_str().to_owned());
                port.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
                .into_any_element()
            }
            None => port.into_any_element(),
        };
        frame = frame.child(port);
    }
    frame
}

/// The palette, resolved only for a node with a coloured port.
fn handle_palette(node: &Node, theme: &GuiraffeTheme) -> Option<crate::ink::Palette> {
    node.handles
        .iter()
        .any(|h| h.color.is_some())
        .then(|| crate::ink::palette(theme))
}
