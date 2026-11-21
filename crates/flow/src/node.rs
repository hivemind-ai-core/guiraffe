//! Retained node entities, with GPUI paint reuse when layout and state match.
use crate::NodeRenderer;
use gpui_kit::{prelude::*, *};
use guiraffe_core::GuiraffeTheme;
use guiraffe_flow_core::{Node, Point as WorldPoint, Rect};
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

pub(crate) struct NodeCard {
    pub node: Node,
    pub revision: u64,
    pub zoom: f32,
    pub renderer: Rc<dyn NodeRenderer>,
}
impl Render for NodeCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        render_card(
            &self.node,
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
    zoom: f32,
    renderer: &dyn NodeRenderer,
    style: Option<StyleRefinement>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    let accent = theme.primary_color();
    // The border of a node is painted over the children of the node, so
    // the handles are children of a frame around the border: the handles
    // are then painted above the border and sit whole on it.
    let mut frame = div().id(("flow-node", node.id)).relative().size_full();
    if let Some(style) = style {
        *frame.style() = style;
    }
    let mut frame = frame.test_support().child(
        div()
            .size_full()
            .rounded(px(theme.default_radius.radius() * zoom))
            .bg(surfaces.surface.to_hsla())
            .border_1()
            .border_color(if node.selected {
                accent.to_hsla()
            } else {
                surfaces.border.to_hsla()
            })
            .child(renderer.render(node, zoom, window, cx)),
    );
    for handle in &node.handles {
        let bounds = handle_bounds(
            handle.position(Rect {
                origin: WorldPoint::default(),
                size: node.size,
            }),
            zoom,
        );
        frame = frame.child(
            div()
                .id(SharedString::from(format!("handle-{}", handle.id)))
                .test_support()
                .absolute()
                // The dot is centred on the border of the node.
                .left(px(bounds.origin.x))
                .top(px(bounds.origin.y))
                .size(px(bounds.size.x))
                .rounded_full()
                .bg(accent.to_hsla()),
        );
    }
    frame
}
