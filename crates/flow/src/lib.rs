//! Retained GPUI flow canvas. Model and geometry live in `guiraffe-flow-core`.
mod interaction;
mod paint;
#[cfg(test)]
mod tests;
use core::{
    ConnectionValidator, FlowError, Graph, Node, Point as WorldPoint, Rect, Viewport, Visible,
};
use gpui_kit::{prelude::*, *};
pub use guiraffe_flow_core as core;
pub use interaction::{Gesture, Interaction};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Custom content is laid out within the node's scaled frame. Renderers receive
/// zoom explicitly so they can scale typography and internal spacing too.
pub trait NodeRenderer: 'static {
    fn render(&self, node: &Node, zoom: f32, window: &mut Window, cx: &mut App) -> AnyElement;
}
pub struct DefaultNodeRenderer;
impl NodeRenderer for DefaultNodeRenderer {
    fn render(&self, node: &Node, zoom: f32, _: &mut Window, _: &mut App) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .text_size(px(14. * zoom))
            .text_color(rgb(0xe2e8f0))
            .child(node.label().to_owned())
            .into_any_element()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Background {
    #[default]
    Dots,
    Lines,
    Cross,
    None,
}
#[derive(Clone, Debug)]
pub struct FlowOptions {
    pub background: Background,
    pub minimap: bool,
    pub controls: bool,
    pub toolbar: bool,
    pub snap_grid: Option<f32>,
}
impl Default for FlowOptions {
    fn default() -> Self {
        Self {
            background: Background::Dots,
            minimap: true,
            controls: true,
            toolbar: true,
            snap_grid: Some(20.),
        }
    }
}
#[derive(Clone, Debug)]
pub enum FlowEvent {
    Changed,
    SelectionChanged,
    ViewportChanged(Viewport),
    Error(FlowError),
}
/// UI entity. Use `edit` to mutate the document so caches and history stay coherent.
pub struct Flow {
    interaction: Interaction,
    options: FlowOptions,
    renderer: Rc<dyn NodeRenderer>,
    validator: Option<Rc<dyn ConnectionValidator>>,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    visible: Visible,
    meshes: Rc<RefCell<paint::MeshCache>>,
    last_error: Option<FlowError>,
}
impl EventEmitter<FlowEvent> for Flow {}
impl Flow {
    pub fn new(graph: Graph, cx: &mut Context<Self>) -> Result<Self, FlowError> {
        Ok(Self {
            interaction: Interaction::new(graph)?,
            options: FlowOptions::default(),
            renderer: Rc::new(DefaultNodeRenderer),
            validator: None,
            focus: cx.focus_handle(),
            bounds: Rc::new(Cell::new(Bounds::default())),
            visible: Visible::default(),
            meshes: Rc::new(RefCell::new(paint::MeshCache::default())),
            last_error: None,
        })
    }
    pub fn options(mut self, options: FlowOptions) -> Self {
        self.interaction.grid = options.snap_grid;
        self.options = options;
        self
    }
    pub fn node_renderer(mut self, renderer: impl NodeRenderer) -> Self {
        self.renderer = Rc::new(renderer);
        self
    }
    pub fn connection_validator(mut self, validator: impl ConnectionValidator + 'static) -> Self {
        self.validator = Some(Rc::new(validator));
        self
    }
    pub fn graph(&self) -> &Graph {
        self.interaction.editor.graph()
    }
    pub fn viewport(&self) -> Viewport {
        self.interaction.viewport
    }
    pub fn last_error(&self) -> Option<&FlowError> {
        self.last_error.as_ref()
    }
    pub fn set_viewport(
        &mut self,
        viewport: Viewport,
        cx: &mut Context<Self>,
    ) -> Result<(), FlowError> {
        if !viewport.valid() {
            return Err(FlowError::InvalidGeometry);
        }
        self.interaction.cancel();
        self.interaction.viewport = viewport;
        cx.emit(FlowEvent::ViewportChanged(viewport));
        cx.notify();
        Ok(())
    }
    pub fn edit(
        &mut self,
        operation: impl FnOnce(&mut Graph) -> Result<(), FlowError>,
        cx: &mut Context<Self>,
    ) -> Result<bool, FlowError> {
        self.interaction.cancel();
        let changed = self.interaction.editor.edit(operation)?;
        self.changed(changed, cx);
        Ok(changed)
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.interaction.cancel();
        let result = self.interaction.editor.undo();
        self.finish(result, cx);
    }
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        self.interaction.cancel();
        let result = self.interaction.editor.redo();
        self.finish(result, cx);
    }
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.interaction.cancel();
        if let Some(bounds) = self.interaction.editor.scene().bounds {
            self.interaction.viewport.fit(bounds, self.size(), 40.);
        }
        cx.emit(FlowEvent::ViewportChanged(self.viewport()));
        cx.notify();
    }
    fn size(&self) -> WorldPoint {
        let b = self.bounds.get();
        WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32())
    }
    fn local(&self, p: Point<Pixels>) -> WorldPoint {
        let origin = self.bounds.get().origin;
        WorldPoint::new((p.x - origin.x).as_f32(), (p.y - origin.y).as_f32())
    }
    fn changed(&mut self, changed: bool, cx: &mut Context<Self>) {
        if changed {
            self.meshes.borrow_mut().clear();
            cx.emit(FlowEvent::Changed);
        }
        cx.notify();
    }
    fn finish(&mut self, result: Result<bool, FlowError>, cx: &mut Context<Self>) {
        match result {
            Ok(changed) => {
                self.last_error = None;
                self.changed(changed, cx);
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                cx.emit(FlowEvent::Error(error));
                cx.notify();
            }
        }
    }
    fn zoom(&mut self, at: WorldPoint, factor: f32, cx: &mut Context<Self>) {
        self.interaction.cancel();
        self.interaction.viewport.zoom_at(at, factor);
        cx.emit(FlowEvent::ViewportChanged(self.viewport()));
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let mods = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        if mods.control || mods.platform {
            match key {
                "z" if mods.shift => self.redo(cx),
                "z" => self.undo(cx),
                "y" => self.redo(cx),
                "a" => {
                    let ids = self.graph().nodes.keys().copied().collect();
                    self.interaction.editor.select(&ids, false);
                    cx.emit(FlowEvent::SelectionChanged);
                    cx.notify();
                }
                _ => {}
            }
            return;
        }
        match key {
            "escape" => {
                self.interaction.cancel();
                cx.notify();
            }
            "delete" | "backspace" => {
                self.interaction.cancel();
                let result = self.interaction.editor.delete_selected();
                self.finish(result, cx);
            }
            "+" | "=" => self.zoom(self.size() * 0.5, 1.2, cx),
            "-" => self.zoom(self.size() * 0.5, 1. / 1.2, cx),
            "f" => self.fit(cx),
            "left" | "right" | "up" | "down" => {
                self.interaction.cancel();
                let delta = match key {
                    "left" => WorldPoint::new(40., 0.),
                    "right" => WorldPoint::new(-40., 0.),
                    "up" => WorldPoint::new(0., 40.),
                    _ => WorldPoint::new(0., -40.),
                };
                self.interaction.viewport.pan = self.viewport().pan + delta;
                cx.emit(FlowEvent::ViewportChanged(self.viewport()));
                cx.notify();
            }
            _ => {}
        }
    }
}
impl Focusable for Flow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Flow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = self.viewport();
        let size = if self.size() == WorldPoint::default() {
            WorldPoint::new(
                window.viewport_size().width.as_f32(),
                window.viewport_size().height.as_f32(),
            )
        } else {
            self.size()
        };
        self.interaction.editor.scene().visible(
            viewport.visible(size).expand(12. / viewport.zoom),
            &mut self.visible,
        );
        if let Some(Gesture::Move { origins, .. }) = &self.interaction.gesture {
            self.visible.nodes.extend(origins.keys().copied());
            for id in origins.keys() {
                self.visible
                    .edges
                    .extend(self.interaction.editor.scene().incident_edges(*id));
            }
            self.visible.nodes.sort_unstable();
            self.visible.nodes.dedup();
            self.visible.edges.sort_unstable();
            self.visible.edges.dedup();
        }
        let mut nodes = Vec::with_capacity(self.visible.nodes.len());
        for id in &self.visible.nodes {
            let node = &self.graph().nodes[id];
            let position = viewport.screen(self.interaction.preview(*id).unwrap_or(node.pos));
            let content = self.renderer.render(node, viewport.zoom, window, cx);
            let mut frame = div()
                .id(("flow-node", *id))
                .test_support()
                .absolute()
                .left(px(position.x))
                .top(px(position.y))
                .w(px(node.size.x * viewport.zoom))
                .h(px(node.size.y * viewport.zoom))
                .rounded(px(10. * viewport.zoom))
                .bg(rgb(0x202a3b))
                .border_1()
                .border_color(rgb(if node.selected { 0x8b9cff } else { 0x465169 }))
                .child(content);
            for handle in &node.handles {
                let p = viewport.screen(handle.position(Rect {
                    origin: self.interaction.preview(*id).unwrap_or(node.pos),
                    size: node.size,
                })) - position;
                frame = frame.child(
                    div()
                        .absolute()
                        .left(px(p.x - 5.))
                        .top(px(p.y - 5.))
                        .size(px(10.))
                        .rounded_full()
                        .bg(rgb(0x9eafff)),
                );
            }
            nodes.push(frame);
        }
        let labels: Vec<_> = self
            .visible
            .edges
            .iter()
            .filter_map(|id| {
                let edge = &self.graph().edges[id];
                if edge.label.is_empty() || viewport.zoom < 0.4 {
                    return None;
                }
                let path = &self.interaction.editor.scene().paths[id];
                let source = self.interaction.preview(edge.source.node);
                let target = self.interaction.preview(edge.target.node);
                let label = if source.is_some() || target.is_some() {
                    core::EdgePath::new(self.graph(), edge, source, target)
                        .map_or(path.label, |p| p.label)
                } else {
                    path.label
                };
                let p = viewport.screen(label);
                Some(
                    div()
                        .absolute()
                        .left(px(p.x))
                        .top(px(p.y - 10.))
                        .max_w(px(180.))
                        .overflow_hidden()
                        .rounded_md()
                        .bg(rgb(0x162033))
                        .text_color(rgb(0xb8c6e3))
                        .text_size(px(11.))
                        .px_1()
                        .child(edge.label.clone()),
                )
            })
            .collect();
        let bounds = self.bounds.clone();
        let weak = cx.entity().downgrade();
        let meshes = self.meshes.clone();
        let frame = paint::Frame::new(
            &self.interaction,
            &self.visible,
            self.options.background,
            self.options.minimap,
        );
        let mut root = div()
            .id("flow")
            .test_support()
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x111827))
            .child(
                canvas(
                    move |b, _, cx| {
                        if bounds.get() != b {
                            bounds.set(b);
                            cx.defer(move |cx| {
                                let _ = weak.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    move |b, _, window, _| frame.paint(b, &mut meshes.borrow_mut(), window),
                )
                .absolute()
                .size_full(),
            )
            .children(labels)
            .children(nodes)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let p = this.local(event.position);
                    let m = event.modifiers;
                    // Minimap is painted in the lower right; clicks recenter the camera.
                    if this.options.minimap && paint::minimap_rect(this.size()).contains(p) {
                        this.interaction.cancel();
                        if let Some(bounds) = this.interaction.editor.scene().bounds {
                            let world = paint::minimap_world(p, this.size(), bounds);
                            this.interaction.viewport.pan =
                                this.size() * 0.5 - world * this.viewport().zoom;
                            cx.emit(FlowEvent::ViewportChanged(this.viewport()));
                        }
                    } else {
                        this.interaction
                            .press(p, m.shift || m.control || m.platform, m.shift);
                        cx.emit(FlowEvent::SelectionChanged);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.interaction.gesture.is_some() {
                    this.interaction.motion(this.local(event.position));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.interaction.motion(this.local(event.position));
                    let result = this.interaction.release(this.validator.as_deref());
                    this.finish(result, cx);
                    cx.emit(FlowEvent::SelectionChanged);
                    cx.emit(FlowEvent::ViewportChanged(this.viewport()));
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.interaction.motion(this.local(event.position));
                    let result = this.interaction.release(this.validator.as_deref());
                    this.finish(result, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(20.));
                if event.modifiers.control || event.modifiers.platform {
                    this.zoom(
                        this.local(event.position),
                        (delta.y.as_f32() * 0.008).exp(),
                        cx,
                    );
                } else {
                    this.interaction.cancel();
                    this.interaction.viewport.pan =
                        this.viewport().pan + WorldPoint::new(delta.x.as_f32(), delta.y.as_f32());
                    cx.emit(FlowEvent::ViewportChanged(this.viewport()));
                    cx.notify();
                }
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                this.zoom(this.local(event.position), 1. + event.delta, cx)
            }))
            .on_key_down(cx.listener(|this, event, _, cx| this.key(event, cx)));
        if self.options.controls {
            root = root.child(self.controls(cx));
        }
        if self.options.toolbar
            && let Some(node) = self.graph().nodes.values().find(|n| n.selected)
        {
            let p = viewport.screen(self.interaction.preview(node.id).unwrap_or(node.pos));
            root = root.child(
                div().absolute().left(px(p.x)).top(px(p.y - 34.)).child(
                    gpui_kit::component::button::Button::new("flow-delete")
                        .label("Delete selected")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.interaction.editor.delete_selected();
                            this.finish(result, cx);
                        })),
                ),
            );
        }
        if let Some(error) = &self.last_error {
            root = root.child(
                div()
                    .absolute()
                    .left_2()
                    .bottom_2()
                    .text_color(rgb(0xffb4b4))
                    .child(error.to_string()),
            );
        }
        root
    }
}
impl Flow {
    fn controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::button::Button;
        div()
            .absolute()
            .top_2()
            .left_2()
            .flex()
            .gap_1()
            .child(
                Button::new("flow-zoom-in")
                    .label("+")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom(this.size() * 0.5, 1.2, cx))),
            )
            .child(
                Button::new("flow-zoom-out").label("−").on_click(
                    cx.listener(|this, _, _, cx| this.zoom(this.size() * 0.5, 1. / 1.2, cx)),
                ),
            )
            .child(
                Button::new("flow-fit")
                    .label("Fit")
                    .on_click(cx.listener(|this, _, _, cx| this.fit(cx))),
            )
            .child(
                Button::new("flow-undo")
                    .label("Undo")
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                Button::new("flow-redo")
                    .label("Redo")
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
    }
}
