//! Retained node entities, with GPUI paint reuse when layout and state match.
use crate::NodeRenderer;
use gpui_kit::{prelude::*, *};
use guiraffe_core::GuiraffeTheme;
use guiraffe_flow_core::{Node, Point as WorldPoint, Rect};
use std::rc::Rc;
pub(crate) struct NodeCard {
    pub node: Node,
    pub revision: u64,
    pub zoom: f32,
    pub renderer: Rc<dyn NodeRenderer>,
}
impl Render for NodeCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let accent = theme.primary_color();
        let mut frame = div()
            .id(("flow-node", self.node.id))
            .test_support()
            .relative()
            .size_full()
            .rounded(px(theme.default_radius.radius() * self.zoom))
            .bg(surfaces.surface.to_hsla())
            .border_1()
            .border_color(if self.node.selected {
                accent.to_hsla()
            } else {
                surfaces.border.to_hsla()
            })
            .child(self.renderer.render(&self.node, self.zoom, window, cx));
        for handle in &self.node.handles {
            let p = handle.position(Rect {
                origin: WorldPoint::default(),
                size: self.node.size,
            }) * self.zoom;
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(p.x - 5.))
                    .top(px(p.y - 5.))
                    .size(px(10.))
                    .rounded_full()
                    .bg(accent.to_hsla()),
            );
        }
        frame
    }
}
