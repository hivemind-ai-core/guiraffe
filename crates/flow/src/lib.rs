//! Retained GPUI flow canvas. Model and geometry live in `guiraffe-flow-core`.
mod interaction;
mod node;
mod paint;
#[cfg(test)]
mod tests;
use core::{ConnectionValidator, FlowError, Graph, Node, Point as WorldPoint, Viewport, Visible};
use gpui_kit::{prelude::*, *};
use guiraffe_core::{ColorName, GuiraffeTheme, Size};
pub use guiraffe_flow_core as core;
pub use interaction::{Gesture, Interaction};
use std::collections::BTreeMap;
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
    fn render(&self, node: &Node, zoom: f32, _: &mut Window, cx: &mut App) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .text_size(px(Size::Sm.font_size()
                * GuiraffeTheme::global(cx).scale
                * zoom))
            .text_color(GuiraffeTheme::global(cx).surfaces().text.to_hsla())
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
    visible_counts: (usize, usize),
    meshes: Rc<RefCell<paint::MeshCache>>,
    last_error: Option<FlowError>,
    nodes: BTreeMap<core::NodeId, Entity<node::NodeCard>>,
    /// Start of the animated-edge clock (app/executor time, so tests are deterministic).
    anim_epoch: Option<std::time::Instant>,
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
            visible_counts: (0, 0),
            meshes: Rc::new(RefCell::new(paint::MeshCache::default())),
            last_error: None,
            nodes: BTreeMap::new(),
            anim_epoch: None,
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
    /// Configure structural connection rules, including self-loops and parallel edges.
    pub fn connection_policy(mut self, policy: core::ConnectionPolicy) -> Self {
        self.interaction.editor.policy = policy;
        self
    }
    /// Live node selection, independent of document serialization and undo/redo.
    pub fn selected_nodes(&self) -> &std::collections::BTreeSet<core::NodeId> {
        self.interaction.editor.selected_nodes()
    }
    /// Live edge selection, independent of document serialization and undo/redo.
    pub fn selected_edges(&self) -> &std::collections::BTreeSet<core::EdgeId> {
        self.interaction.editor.selected_edges()
    }
    pub fn connection_validator(mut self, validator: impl ConnectionValidator + 'static) -> Self {
        self.validator = Some(Rc::new(validator));
        self
    }
    pub fn graph(&self) -> &Graph {
        self.interaction.editor.graph()
    }
    /// Last paint stage wall times in microseconds: grid, edges, minimap.
    pub fn paint_timings(&self) -> (u128, u128, u128) {
        self.meshes.borrow().timings
    }
    /// Node and edge counts admitted by the last viewport query, including drag previews.
    pub fn visible_counts(&self) -> (usize, usize) {
        self.visible_counts
    }
    pub fn viewport(&self) -> Viewport {
        self.interaction.viewport()
    }
    pub fn last_error(&self) -> Option<&FlowError> {
        self.last_error.as_ref()
    }
    pub fn set_viewport(
        &mut self,
        viewport: Viewport,
        cx: &mut Context<Self>,
    ) -> Result<(), FlowError> {
        let before = self.viewport();
        let had_gesture = self.interaction.gesture.is_some();
        self.interaction.set_viewport(viewport)?;
        let viewport = self.viewport();
        if before == viewport && !had_gesture {
            return Ok(());
        }
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
        self.interaction.fit(self.size());
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
            self.meshes
                .borrow_mut()
                .sync(self.interaction.editor.scene());
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
        self.interaction.zoom_at(at, factor);
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
                let result = self.interaction.delete_selected();
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
                self.interaction.pan_to(self.viewport().pan + delta);
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
        let theme = GuiraffeTheme::global(cx).clone();
        let surfaces = theme.surfaces();
        let viewport = self.viewport();
        let size = if self.size() == WorldPoint::default() {
            WorldPoint::new(
                window.viewport_size().width.as_f32(),
                window.viewport_size().height.as_f32(),
            )
        } else {
            self.size()
        };
        if self.visible.edges.is_empty() {
            self.visible.edges = std::mem::take(&mut self.meshes.borrow_mut().edge_buffer);
        }
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
        self.visible
            .nodes
            .retain(|id| self.interaction.editor.graph().nodes.contains_key(id));
        self.visible_counts = (self.visible.nodes.len(), self.visible.edges.len());
        self.nodes
            .retain(|id, _| self.visible.nodes.binary_search(id).is_ok());
        self.visible
            .nodes
            .sort_by_key(|id| self.interaction.editor.z_key(*id));
        let mut nodes = Vec::with_capacity(self.visible.nodes.len());
        for id in &self.visible.nodes {
            let Some(node) = self.interaction.editor.graph().nodes.get(id) else {
                continue;
            };
            let revision = self.interaction.editor.scene().node_revision(*id);
            let selected = self.interaction.editor.is_selected(*id);
            let position = viewport.screen(self.interaction.preview(*id).unwrap_or(node.pos));
            let card = self.nodes.entry(*id).or_insert_with(|| {
                cx.new(|_| node::NodeCard {
                    node: {
                        let mut n = node.clone();
                        n.selected = selected;
                        n
                    },
                    revision,
                    zoom: viewport.zoom,
                    theme: theme.clone(),
                    renderer: self.renderer.clone(),
                })
            });
            if card.read(cx).revision != revision
                || card.read(cx).node.selected != selected
                || card.read(cx).zoom != viewport.zoom
                || card.read(cx).theme != theme
            {
                card.update(cx, |card, cx| {
                    if card.revision != revision {
                        card.node = node.clone();
                        card.revision = revision;
                    }
                    card.node.selected = selected;
                    card.zoom = viewport.zoom;
                    card.theme = theme.clone();
                    cx.notify();
                });
            }
            let style = StyleRefinement::default()
                .absolute()
                .left(px(position.x))
                .top(px(position.y))
                .w(px(node.size.x * viewport.zoom))
                .h(px(node.size.y * viewport.zoom));
            // GPUI 0.3.7 does not replay accessibility nodes from its paint cache.
            if window.is_a11y_active() {
                let mut frame = div().child(card.clone());
                *frame.style() = style;
                nodes.push(frame.into_any_element());
            } else {
                nodes.push(card.clone().cached(style).into_any_element());
            }
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
                let path = &self.interaction.editor.scene().paths()[id];
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
                        .bg(surfaces.surface.to_hsla())
                        .text_color(surfaces.dimmed.to_hsla())
                        .text_size(px(Size::Xs.font_size() * theme.scale))
                        .px_1()
                        .child(edge.label.clone()),
                )
            })
            .collect();
        let bounds = self.bounds.clone();
        let weak = cx.entity().downgrade();
        let meshes = self.meshes.clone();
        // Animated edges: phase from the app clock; none at all under reduced motion.
        let now = cx.background_executor().now();
        let epoch = *self.anim_epoch.get_or_insert(now);
        let phase = (!cx.reduce_motion())
            .then(|| (now.saturating_duration_since(epoch).as_millis() % 1000) as f32 / 1000.);
        let frame = paint::Frame::new(
            &self.interaction,
            &mut self.visible,
            self.options.background,
            self.options.minimap,
            &theme,
            phase,
        );
        let mut root = div()
            .id("flow")
            .test_support()
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(surfaces.body.to_hsla())
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
                        if let Some(bounds) = this.interaction.editor.scene().bounds() {
                            let world = paint::minimap_world(p, this.size(), bounds);
                            this.interaction
                                .pan_to(this.size() * 0.5 - world * this.viewport().zoom);
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
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(20.));
                if event.modifiers.control || event.modifiers.platform {
                    this.zoom(
                        this.local(event.position),
                        (delta.y.as_f32() * 0.008).clamp(-80., 80.).exp(),
                        cx,
                    );
                } else {
                    this.interaction.cancel();
                    this.interaction.pan_to(
                        this.viewport().pan + WorldPoint::new(delta.x.as_f32(), delta.y.as_f32()),
                    );
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
            && let Some(node) = self
                .interaction
                .editor
                .selected_nodes()
                .iter()
                .next()
                .and_then(|id| self.graph().nodes.get(id))
        {
            let p = viewport.screen(self.interaction.preview(node.id).unwrap_or(node.pos));
            root = root.child(
                div()
                    .absolute()
                    .left(px(p.x))
                    .top(px(p.y - 34.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        gpui_kit::component::button::Button::new("flow-delete")
                            .label("Delete selected")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this.interaction.delete_selected();
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
                    .text_color(ColorName::Red.shade(6).to_hsla())
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
