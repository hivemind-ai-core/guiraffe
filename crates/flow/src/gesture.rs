//! Pointer feedback is a separate view; notifying it never writes document state.
use crate::*;
use std::sync::Arc;
pub struct FlowGesture {
    pub(crate) scene: Arc<core::Scene>,
    pub(crate) gesture: Option<Gesture>,
    pub(crate) viewport: Viewport,
}
impl Render for FlowGesture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scene = self.scene.clone();
        let gesture = self.gesture.clone();
        let viewport = self.viewport;
        let accent = GuiraffeTheme::global(cx).primary_color();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                paint::paint_gesture(&scene, gesture.as_ref(), viewport, bounds, accent, window)
            },
        )
        .absolute()
        .size_full()
    }
}
