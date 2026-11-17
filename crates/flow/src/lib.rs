//! Retained GPUI flow canvas. Model and geometry live in `guiraffe-flow-core`.
mod gesture;
mod interaction;
mod node;
mod paint;
mod state;
pub use gesture::FlowGesture;
pub use state::{FlowCanvas, FlowState};
/// Compatibility name for the state/view host. New compositions can use FlowCanvas.
pub type Flow = FlowState;
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
    /// Fit once after the first nonempty layout, unless a camera is explicitly set.
    pub fit_on_init: bool,
}
impl Default for FlowOptions {
    fn default() -> Self {
        Self {
            background: Background::Dots,
            minimap: true,
            controls: true,
            toolbar: true,
            snap_grid: Some(20.),
            fit_on_init: true,
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
pub struct FlowViewport {
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
    model: Option<WeakEntity<FlowState>>,
    gesture_view: Entity<FlowGesture>,
    camera: Rc<Cell<Viewport>>,
    counts: Rc<Cell<(usize, usize)>>,
    first_fit: Rc<Cell<bool>>,
    camera_explicit: Cell<bool>,
    minimap_drag: bool,
    config: Rc<RefCell<Option<state::Config>>>,
    theme_cache: Option<Rc<GuiraffeTheme>>,
    ordered_ids: Vec<core::NodeId>,
    order_scene: Option<std::sync::Arc<core::Scene>>,
    order_selection: Option<std::sync::Arc<std::collections::BTreeSet<core::NodeId>>>,
    last_query_ids: Vec<core::NodeId>,
    #[cfg(test)]
    renders: usize,
}
struct Stamp {
    scene: std::sync::Arc<core::Scene>,
    nodes: std::sync::Arc<std::collections::BTreeSet<core::NodeId>>,
    edges: std::sync::Arc<std::collections::BTreeSet<core::EdgeId>>,
    viewport: Viewport,
    gesture: Option<Gesture>,
    error: Option<FlowError>,
}
impl EventEmitter<FlowEvent> for FlowViewport {}
impl FlowViewport {
    pub fn new(graph: Graph, cx: &mut Context<Self>) -> Result<Self, FlowError> {
        Ok(Self::from_editor(core::Editor::new(graph)?, None, cx))
    }
    fn from_editor(
        editor: core::Editor,
        model: Option<WeakEntity<FlowState>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let scene = editor.scene_shared();
        let gesture_view = cx.new(|_| FlowGesture {
            scene,
            gesture: None,
            viewport: Viewport::default(),
        });
        Self {
            interaction: Interaction::from_editor(editor),
            gesture_view,
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
            model,
            camera: Rc::new(Cell::new(Viewport::default())),
            counts: Rc::new(Cell::new((0, 0))),
            first_fit: Rc::new(Cell::new(true)),
            camera_explicit: Cell::new(false),
            minimap_drag: false,
            config: Rc::new(RefCell::new(None)),
            theme_cache: None,
            ordered_ids: Vec::new(),
            order_scene: None,
            order_selection: None,
            last_query_ids: Vec::new(),
            #[cfg(test)]
            renders: 0,
        }
    }
    pub fn options(mut self, options: FlowOptions) -> Self {
        self.first_fit.set(options.fit_on_init);
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
    fn stamp(&self) -> Stamp {
        Stamp {
            scene: self.interaction.editor.scene_shared(),
            nodes: self.interaction.editor.selected_nodes_shared(),
            edges: self.interaction.editor.selected_edges_shared(),
            viewport: self.viewport(),
            gesture: self.visible_gesture(),
            error: self.last_error.clone(),
        }
    }
    fn visible_gesture(&self) -> Option<Gesture> {
        self.interaction
            .gesture
            .as_ref()
            .filter(|g| match g {
                Gesture::Pan { .. } => false,
                Gesture::Move { delta, .. } => *delta != WorldPoint::default(),
                Gesture::Box { start, end, .. } => start.distance(*end) > 3. / self.viewport().zoom,
                Gesture::Connect { .. } => true,
            })
            .cloned()
    }
    fn publish(&mut self, before: Stamp, cx: &mut Context<Self>) {
        use std::sync::Arc;
        let document = !Arc::ptr_eq(&before.scene, &self.interaction.editor.scene_shared());
        let selection = !Arc::ptr_eq(
            &before.nodes,
            &self.interaction.editor.selected_nodes_shared(),
        ) || !Arc::ptr_eq(
            &before.edges,
            &self.interaction.editor.selected_edges_shared(),
        );
        let viewport = before.viewport != self.viewport();
        let gesture = match (&before.gesture, &self.visible_gesture()) {
            (None, None) => false,
            (Some(a), Some(b)) => !a.same(b),
            _ => true,
        };
        let error = before.error != self.last_error;
        self.camera.set(self.viewport());
        if document {
            self.meshes
                .borrow_mut()
                .sync(self.interaction.editor.scene());
            cx.emit(FlowEvent::Changed);
        }
        if selection {
            cx.emit(FlowEvent::SelectionChanged);
        }
        if viewport {
            cx.emit(FlowEvent::ViewportChanged(self.viewport()));
        }
        if error && let Some(error) = &self.last_error {
            cx.emit(FlowEvent::Error(error.clone()));
        }
        if let Some(model) = &self.model
            && (document || selection || error)
        {
            let editor = self.interaction.editor.clone();
            let _ = model.update(cx, |model, cx| {
                model.editor = editor;
                model.last_error = self.last_error.clone();
                if error && let Some(error) = &self.last_error {
                    cx.emit(FlowEvent::Error(error.clone()));
                }
                if document {
                    cx.emit(FlowEvent::Changed);
                }
                if selection {
                    cx.emit(FlowEvent::SelectionChanged);
                }
                cx.notify();
            });
        }
        if document || selection || viewport {
            self.refresh_cards(cx);
        }
        if gesture || (viewport && self.visible_gesture().is_some()) {
            let scene = self.interaction.editor.scene_shared();
            let gesture = self.visible_gesture();
            let viewport = self.viewport();
            self.gesture_view.update(cx, |leaf, cx| {
                leaf.scene = scene;
                leaf.gesture = gesture;
                leaf.viewport = viewport;
                cx.notify();
            });
        }
        if document || selection || viewport || gesture || error {
            cx.notify();
        }
    }
    fn refresh_cards(&mut self, cx: &mut Context<Self>) {
        for (id, card) in &self.nodes {
            let Some(node) = self.interaction.editor.graph().nodes.get(id) else {
                continue;
            };
            let revision = self.interaction.editor.scene().node_revision(*id);
            let selected = self.interaction.editor.is_selected(*id);
            let zoom = self.viewport().zoom;
            let old = card.read(cx);
            if old.revision != revision || old.node.selected != selected || old.zoom != zoom {
                card.update(cx, |card, cx| {
                    if card.revision != revision {
                        card.node = node.clone();
                        card.revision = revision;
                    }
                    card.node.selected = selected;
                    card.zoom = zoom;
                    cx.notify();
                });
            }
        }
    }
    pub fn set_viewport(
        &mut self,
        viewport: Viewport,
        cx: &mut Context<Self>,
    ) -> Result<(), FlowError> {
        let before = self.stamp();
        self.interaction.set_viewport(viewport)?;
        self.camera_explicit.set(true);
        self.first_fit.set(false);
        self.publish(before, cx);
        Ok(())
    }
    pub fn edit(
        &mut self,
        operation: impl FnOnce(&mut Graph) -> Result<(), FlowError>,
        cx: &mut Context<Self>,
    ) -> Result<bool, FlowError> {
        let before = self.stamp();
        self.interaction.cancel();
        let changed = self.interaction.editor.edit(operation)?;
        self.publish(before, cx);
        Ok(changed)
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let before = self.stamp();
        self.interaction.cancel();
        let result = self.interaction.editor.undo();
        self.finish(result);
        self.publish(before, cx);
    }
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        let before = self.stamp();
        self.interaction.cancel();
        let result = self.interaction.editor.redo();
        self.finish(result);
        self.publish(before, cx);
    }
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        let before = self.stamp();
        self.first_fit.set(false);
        self.interaction.fit(self.size());
        self.publish(before, cx);
    }
    fn size(&self) -> WorldPoint {
        let b = self.bounds.get();
        WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32())
    }
    fn pan_minimap(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let before = self.stamp();
        self.interaction.cancel();
        if let Some(bounds) = self.interaction.editor.scene().bounds() {
            let world = paint::minimap_world(self.local(position), self.size(), bounds);
            self.interaction
                .pan_to(self.size() * 0.5 - world * self.viewport().zoom);
            self.first_fit.set(false);
        }
        self.publish(before, cx);
    }
    fn local(&self, p: Point<Pixels>) -> WorldPoint {
        let origin = self.bounds.get().origin;
        WorldPoint::new((p.x - origin.x).as_f32(), (p.y - origin.y).as_f32())
    }
    fn finish(&mut self, result: Result<bool, FlowError>) {
        self.last_error = result.err();
    }
    fn zoom(&mut self, at: WorldPoint, factor: f32, cx: &mut Context<Self>) {
        let before = self.stamp();
        self.first_fit.set(false);
        self.interaction.zoom_at(at, factor);
        self.publish(before, cx);
    }
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let before = self.stamp();
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
                    self.publish(before, cx);
                }
                _ => {}
            }
            return;
        }
        match key {
            "escape" => {
                self.interaction.cancel();
                self.publish(before, cx);
            }
            "delete" | "backspace" => {
                self.interaction.cancel();
                let result = self.interaction.delete_selected();
                self.finish(result);
                self.publish(before, cx);
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
                self.first_fit.set(false);
                self.publish(before, cx);
            }
            _ => {}
        }
    }
}
impl Focusable for FlowViewport {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for FlowViewport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        if let Some(config) = self.config.borrow_mut().take() {
            self.first_fit
                .set(config.options.fit_on_init && !self.camera_explicit.get());
            self.interaction.grid = config.options.snap_grid;
            self.options = config.options;
            self.renderer = config.renderer;
            self.validator = config.validator;
            self.interaction.editor.policy = config.policy;
        }
        let current = GuiraffeTheme::global(cx);
        if self
            .theme_cache
            .as_ref()
            .is_none_or(|cached| cached.as_ref() != current)
        {
            self.theme_cache = Some(Rc::new(current.clone()));
        }
        let theme = self
            .theme_cache
            .clone()
            .unwrap_or_else(|| Rc::new(current.clone()));
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
        self.counts.set(self.visible_counts);
        self.nodes
            .retain(|id, _| self.visible.nodes.binary_search(id).is_ok());
        let scene = self.interaction.editor.scene_shared();
        let selection = self.interaction.editor.selected_nodes_shared();
        if self.last_query_ids != self.visible.nodes
            || self
                .order_scene
                .as_ref()
                .is_none_or(|old| !std::sync::Arc::ptr_eq(old, &scene))
            || self
                .order_selection
                .as_ref()
                .is_none_or(|old| !std::sync::Arc::ptr_eq(old, &selection))
        {
            self.last_query_ids.clone_from(&self.visible.nodes);
            self.ordered_ids.clone_from(&self.visible.nodes);
            self.ordered_ids
                .sort_unstable_by_key(|id| self.interaction.editor.z_key(*id));
            self.order_scene = Some(scene);
            self.order_selection = Some(selection);
        }
        let mut nodes = Vec::with_capacity(self.visible.nodes.len());
        for id in &self.ordered_ids {
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
                    renderer: self.renderer.clone(),
                })
            });
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
                let edge = self.graph().edges.get(id)?;
                if edge.label.is_empty() || viewport.zoom < 0.4 {
                    return None;
                }
                let path = self.interaction.editor.scene().paths().get(id)?;
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
        let first_fit = self.first_fit.clone();
        let graph_nonempty = self.interaction.editor.scene().bounds().is_some();
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
        let frame = Rc::new(RefCell::new(frame));
        let base_frame = frame.clone();
        let map_meshes = self.meshes.clone();
        let mut root = div()
            .id("flow")
            .test_support()
            // The canvas is an editor surface (keys act on the selection), so it does take
            // focus on click, like a text input (spec/00 §12.3).
            .role(gpui_kit::Role::Canvas)
            .aria_label("Flow diagram")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(surfaces.body.to_hsla())
            .child(
                canvas(
                    move |b, _, cx| {
                        let changed = bounds.get() != b;
                        bounds.set(b);
                        if first_fit.get()
                            && graph_nonempty
                            && b.size.width > px(80.)
                            && b.size.height > px(80.)
                        {
                            first_fit.set(false);
                            cx.defer(move |cx| {
                                let _ = weak.update(cx, |this, cx| this.fit(cx));
                            });
                        } else if changed {
                            cx.defer(move |cx| cx.notify(weak.entity_id()));
                        }
                    },
                    move |b, _, window, _| {
                        base_frame
                            .borrow_mut()
                            .paint(b, &mut meshes.borrow_mut(), window)
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(self.gesture_view.clone())
            .children(labels)
            .children(nodes)
            .when(self.options.minimap, |root| {
                let map = paint::minimap_rect(self.size());
                root.child(
                    div()
                        .id("flow-minimap")
                        .test_support()
                        .absolute()
                        .left(px(map.origin.x))
                        .top(px(map.origin.y))
                        .w(px(map.size.x))
                        .h(px(map.size.y))
                        .bg(surfaces.surface.to_hsla())
                        .border_1()
                        .border_color(surfaces.border.to_hsla())
                        .rounded_md()
                        .shadow_md()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, e: &MouseDownEvent, window, cx| {
                                this.focus.focus(window, cx);
                                this.interaction.cancel();
                                this.minimap_drag = true;
                                this.pan_minimap(e.position, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                            if this.minimap_drag && e.pressed_button == Some(MouseButton::Left) {
                                this.pan_minimap(e.position, cx);
                            } else {
                                this.minimap_drag = false;
                            }
                            cx.stop_propagation();
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, e: &MouseUpEvent, _, cx| {
                                if this.minimap_drag {
                                    this.pan_minimap(e.position, cx);
                                }
                                this.minimap_drag = false;
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| {
                            frame
                                .borrow()
                                .paint_minimap(b, &mut map_meshes.borrow_mut(), window);
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    let before = this.stamp();
                    this.focus.focus(window, cx);
                    let p = this.local(event.position);
                    let m = event.modifiers;
                    this.minimap_drag = false;
                    this.interaction.press(p, m.control || m.platform, m.shift);
                    this.publish(before, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.minimap_drag {
                    if event.pressed_button != Some(MouseButton::Left) {
                        this.minimap_drag = false;
                        return;
                    }
                    this.pan_minimap(event.position, cx);
                } else if this.interaction.gesture.is_some() {
                    let before = this.stamp();
                    this.interaction.motion(this.local(event.position));
                    this.publish(before, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.minimap_drag = false;
                    let before = this.stamp();
                    this.interaction.motion(this.local(event.position));
                    let result = this.interaction.release(this.validator.as_deref());
                    this.finish(result);
                    this.publish(before, cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.minimap_drag = false;
                    let before = this.stamp();
                    this.interaction.motion(this.local(event.position));
                    let result = this.interaction.release(this.validator.as_deref());
                    this.finish(result);
                    this.publish(before, cx);
                }),
            )
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                this.minimap_drag = false;
                let delta = event.delta.pixel_delta(px(20.));
                if event.modifiers.control || event.modifiers.platform {
                    this.zoom(
                        this.local(event.position),
                        (delta.y.as_f32() * 0.008).clamp(-80., 80.).exp(),
                        cx,
                    );
                } else {
                    let before = this.stamp();
                    this.interaction.cancel();
                    this.interaction.pan_to(
                        this.viewport().pan + WorldPoint::new(delta.x.as_f32(), delta.y.as_f32()),
                    );
                    this.first_fit.set(false);
                    this.publish(before, cx);
                }
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                this.zoom(this.local(event.position), 1. + event.delta, cx)
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.minimap_drag = false;
                }
                this.key(event, cx);
            }));
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
                            .tab_stop(guiraffe_core::tab_stop(None))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let before = this.stamp();
                                let result = this.interaction.delete_selected();
                                this.finish(result);
                                this.publish(before, cx);
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
                    // Scheme-aware error color, not a fixed shade.
                    .text_color(
                        guiraffe_core::GuiraffeTheme::global(cx)
                            .variant(Some(ColorName::Red), guiraffe_core::Variant::Light)
                            .foreground
                            .to_hsla(),
                    )
                    .child(error.to_string()),
            );
        }
        root
    }
}
impl FlowViewport {
    fn controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::button::Button;
        div()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .absolute()
            .top_2()
            .left_2()
            .flex()
            .gap_1()
            .child(
                Button::new("flow-zoom-in")
                    .tab_stop(guiraffe_core::tab_stop(None))
                    .accessibility_label("Zoom in")
                    .label("+")
                    .on_click(cx.listener(|this, _, _, cx| this.zoom(this.size() * 0.5, 1.2, cx))),
            )
            .child(
                Button::new("flow-zoom-out")
                    .tab_stop(guiraffe_core::tab_stop(None))
                    .accessibility_label("Zoom out")
                    .label("−")
                    .on_click(
                        cx.listener(|this, _, _, cx| this.zoom(this.size() * 0.5, 1. / 1.2, cx)),
                    ),
            )
            .child(
                Button::new("flow-fit")
                    .tab_stop(guiraffe_core::tab_stop(None))
                    .accessibility_label("Fit to view")
                    .label("Fit")
                    .on_click(cx.listener(|this, _, _, cx| this.fit(cx))),
            )
            .child(
                Button::new("flow-undo")
                    .tab_stop(guiraffe_core::tab_stop(None))
                    .label("Undo")
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                Button::new("flow-redo")
                    .tab_stop(guiraffe_core::tab_stop(None))
                    .label("Redo")
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
    }
}
