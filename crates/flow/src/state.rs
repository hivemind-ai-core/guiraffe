//! Model-owned document state with a separate viewport leaf and a reusable element.
use crate::{
    ConnectionValidator, DefaultNodeRenderer, FlowError, FlowEvent, FlowGesture, FlowOptions,
    FlowViewport, Graph, NodeRenderer, Viewport, WorldPoint, core, editor_for_view, paint,
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, Pixels, Render, RenderOnce, StyleRefinement, Styled, Window,
};
use std::collections::BTreeSet;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) struct Config {
    pub options: FlowOptions,
    pub renderer: Rc<dyn NodeRenderer>,
    pub default_renderer: bool,
    pub validator: Option<Rc<dyn ConnectionValidator>>,
    pub policy: core::ConnectionPolicy,
}
/// Canonical document/history/selection. Camera and pointer frames render in its
/// viewport leaf, so hosts that read the document do not depend on pointer state.
/// `Flow` remains a compatibility alias, including its lightweight Render host.
pub struct FlowState {
    pub(crate) editor: core::Editor,
    canvas: Entity<FlowViewport>,
    camera: Rc<Cell<Viewport>>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    config: Rc<RefCell<Option<Config>>>,
    meshes: Rc<RefCell<paint::MeshCache>>,
    counts: Rc<Cell<(usize, usize)>>,
    layout_job: Option<core::LayoutCancellation>,
    pub(crate) last_error: Option<FlowError>,
    #[cfg(test)]
    renders: usize,
}
impl EventEmitter<FlowEvent> for FlowState {}
impl Drop for FlowState {
    fn drop(&mut self) {
        self.abort_layout();
    }
}
impl FlowState {
    pub fn new(graph: Graph, cx: &mut Context<Self>) -> Result<Self, FlowError> {
        Ok(Self::from_editor(editor_for_view(graph)?, cx))
    }
    /// Build an entity from an editor whose graph has already been validated.
    pub fn from_editor(editor: core::Editor, cx: &mut Context<Self>) -> Self {
        let model = cx.entity().downgrade();
        let canvas = cx.new(|cx| {
            // The validated graph guarantees construction succeeds; use the
            // editor snapshot directly rather than another fallible constructor.
            FlowViewport::from_editor(editor.clone(), Some(model), cx)
        });
        let leaf = canvas.read(cx);
        Self {
            editor,
            camera: leaf.camera.clone(),
            bounds: leaf.bounds.clone(),
            config: leaf.config.clone(),
            meshes: leaf.meshes.clone(),
            counts: leaf.counts.clone(),
            layout_job: None,
            last_error: None,
            canvas,
            #[cfg(test)]
            renders: 0,
        }
    }
    fn config_mut(&mut self) -> std::cell::RefMut<'_, Option<Config>> {
        let mut config = self.config.borrow_mut();
        if config.is_none() {
            *config = Some(Config {
                options: FlowOptions::default(),
                renderer: Rc::new(DefaultNodeRenderer),
                default_renderer: true,
                validator: None,
                policy: self.editor.policy,
            });
        }
        config
    }
    pub fn options(mut self, options: FlowOptions) -> Self {
        if let Some(config) = self.config_mut().as_mut() {
            config.options = options;
        }
        self
    }
    pub fn node_renderer(mut self, renderer: impl NodeRenderer) -> Self {
        if let Some(config) = self.config_mut().as_mut() {
            config.renderer = Rc::new(renderer);
            config.default_renderer = false;
        }
        self
    }
    pub fn connection_policy(mut self, policy: core::ConnectionPolicy) -> Self {
        self.editor.policy = policy;
        if let Some(config) = self.config_mut().as_mut() {
            config.policy = policy;
        }
        self
    }
    pub fn connection_validator(mut self, validator: impl ConnectionValidator + 'static) -> Self {
        if let Some(config) = self.config_mut().as_mut() {
            config.validator = Some(Rc::new(validator));
        }
        self
    }
    pub fn graph(&self) -> &Graph {
        self.editor.graph()
    }
    pub fn selected_nodes(&self) -> &BTreeSet<core::NodeId> {
        self.editor.selected_nodes()
    }
    pub fn selected_edges(&self) -> &BTreeSet<core::EdgeId> {
        self.editor.selected_edges()
    }
    pub fn viewport(&self) -> Viewport {
        self.camera.get()
    }
    /// Subscribe to camera events on this leaf; pan/zoom events do not write the document entity.
    pub fn viewport_entity(&self) -> &Entity<FlowViewport> {
        &self.canvas
    }
    pub fn gesture_entity(&self, cx: &App) -> Entity<FlowGesture> {
        self.canvas.read(cx).gesture_view.clone()
    }
    pub fn paint_timings(&self) -> (u128, u128, u128) {
        self.meshes.borrow().timings
    }
    pub fn visible_counts(&self) -> (usize, usize) {
        self.counts.get()
    }
    pub fn last_error(&self) -> Option<&FlowError> {
        self.last_error.as_ref()
    }
    pub fn size(&self) -> WorldPoint {
        let b = self.bounds.get();
        WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32())
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.clone();
        let error = self.last_error.clone();
        self.canvas.update(cx, |leaf, cx| {
            leaf.layout_preview = None;
            leaf.interaction.cancel();
            leaf.interaction.editor = editor;
            leaf.cancel_stale_indexes();
            leaf.last_error = error;
            if leaf.gesture_view.read(cx).gesture.is_some() {
                leaf.gesture_view.update(cx, |view, cx| {
                    view.gesture = None;
                    cx.notify();
                });
            }
            leaf.meshes
                .borrow_mut()
                .sync(leaf.interaction.editor.scene());
            leaf.refresh_cards(cx);
            cx.notify();
        });
    }
    /// Run an engine on a background snapshot and publish one undoable edit.
    /// A zero duration applies the final layout without a transition.
    pub fn arrange(
        &mut self,
        engine: impl core::LayoutEngine + 'static,
        options: core::LayoutOptions,
        duration: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        self.cancel_layout(cx);
        let cancel = core::LayoutCancellation::default();
        self.layout_job = Some(cancel.clone());
        let editor = self.editor.clone();
        let base = editor.scene_shared();
        let frames = if cx.reduce_motion()
            || editor.graph().nodes.len() > 150
            || editor.graph().edges.len() > 300
        {
            0
        } else {
            (duration.as_millis() / 16).min(20) as usize
        };
        cx.spawn(async move |this, cx| {
            let worker_cancel = cancel.clone();
            let computed = cx
                .background_executor()
                .spawn(async move {
                    let mut result = engine.layout(editor.graph(), &options, &worker_cancel)?;
                    worker_cancel.check()?;
                    let prepared = editor
                        .prepare_layout(&result)
                        .map_err(core::LayoutError::Document)?;
                    let final_scene = prepared.scene();
                    let mut previews = Vec::with_capacity(frames);
                    for step in (1..frames).rev() {
                        worker_cancel.check()?;
                        let t = step as f32 / frames as f32;
                        let progress = 1. - t * t * t;
                        for (&id, position) in &mut result.positions {
                            let from = editor
                                .graph()
                                .nodes
                                .get(&id)
                                .ok_or(FlowError::MissingNode(id))
                                .map_err(core::LayoutError::Document)?
                                .pos;
                            let to = final_scene
                                .graph()
                                .nodes
                                .get(&id)
                                .ok_or(FlowError::MissingNode(id))
                                .map_err(core::LayoutError::Document)?
                                .pos;
                            *position = from + (to - from) * progress;
                        }
                        previews.push(
                            editor
                                .prepare_layout(&result)
                                .map_err(core::LayoutError::Document)?
                                .scene(),
                        );
                    }
                    Ok::<_, core::LayoutError>((prepared, previews))
                })
                .await;
            let active = |state: &Self| {
                cancel.check().is_ok()
                    && std::sync::Arc::ptr_eq(&base, &state.editor.scene_shared())
            };
            let (prepared, previews) = match computed {
                Ok(computed) => computed,
                Err(error) => {
                    let _ = this.update(cx, |state, cx| {
                        if active(state) {
                            state.layout_job = None;
                            let error = FlowError::Rejected(error.to_string());
                            state.last_error = Some(error.clone());
                            state.sync(cx);
                            cx.emit(FlowEvent::Error(error));
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            for scene in previews {
                let shown = this.update(cx, |state, cx| {
                    if !active(state) {
                        return false;
                    }
                    state.canvas.update(cx, |leaf, cx| {
                        if leaf.interaction.gesture.is_some() {
                            cancel.cancel();
                            return false;
                        }
                        leaf.layout_preview = Some(scene);
                        cx.notify();
                        true
                    })
                });
                if !matches!(shown, Ok(true)) {
                    return;
                }
                cx.background_executor()
                    .timer(duration / frames as u32)
                    .await;
            }
            let _ = this.update(cx, |state, cx| {
                if active(state) {
                    state.layout_job = None;
                    if state.canvas.read(cx).interaction.gesture.is_some() {
                        return;
                    }
                    state.last_error = None;
                    let changed = state.editor.install_prepared_layout(prepared);
                    state.sync(cx);
                    if changed {
                        cx.emit(FlowEvent::Changed);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn cancel_layout(&mut self, cx: &mut Context<Self>) {
        self.abort_layout();
        self.canvas.update(cx, |leaf, cx| {
            if leaf.layout_preview.take().is_some() {
                cx.notify();
            }
        });
    }

    pub(crate) fn abort_layout(&mut self) {
        if let Some(cancel) = self.layout_job.take() {
            cancel.cancel();
        }
    }

    pub fn edit(
        &mut self,
        operation: impl FnOnce(&mut Graph) -> Result<(), FlowError>,
        cx: &mut Context<Self>,
    ) -> Result<bool, FlowError> {
        let changed = self.editor.edit(operation)?;
        if changed {
            self.abort_layout();
            self.sync(cx);
            cx.emit(FlowEvent::Changed);
            cx.notify();
        }
        Ok(changed)
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let result = self.editor.undo();
        self.history_result(result, cx);
    }
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        let result = self.editor.redo();
        self.history_result(result, cx);
    }
    fn history_result(&mut self, result: Result<bool, FlowError>, cx: &mut Context<Self>) {
        match result {
            Ok(true) => {
                self.abort_layout();
                self.last_error = None;
                self.sync(cx);
                cx.emit(FlowEvent::Changed);
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => {
                self.last_error = Some(error.clone());
                cx.emit(FlowEvent::Error(error));
                cx.notify();
            }
        }
    }
    pub fn set_viewport(
        &mut self,
        mut viewport: Viewport,
        cx: &mut Context<Self>,
    ) -> Result<(), FlowError> {
        if !viewport.pan.finite() || !viewport.zoom.is_finite() || viewport.zoom <= 0. {
            return Err(FlowError::InvalidGeometry);
        }
        viewport.zoom = viewport.zoom.clamp(Viewport::MIN_ZOOM, Viewport::MAX_ZOOM);
        let leaf = self.canvas.read(cx);
        if viewport == self.viewport() && leaf.interaction.gesture.is_none() {
            leaf.first_fit.set(false);
            leaf.camera_explicit.set(true);
            return Ok(());
        }
        self.canvas
            .update(cx, |leaf, cx| leaf.set_viewport(viewport, cx))
    }
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        let mut viewport = self.viewport();
        if let Some(bounds) = self.editor.scene().bounds() {
            viewport.fit(bounds, self.size(), 40.);
        }
        if let Err(error) = self.set_viewport(viewport, cx) {
            self.last_error = Some(error.clone());
            cx.emit(FlowEvent::Error(error));
            cx.notify();
        }
    }
}
impl Focusable for FlowState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.canvas.read(cx).focus_handle(cx)
    }
}
fn viewport_element(canvas: Entity<FlowViewport>, window: &Window) -> AnyElement {
    if window.is_a11y_active() {
        return canvas.into_any_element();
    }
    // The surrounding host supplies this viewport's size. GPUI can replay its
    // prior paint ranges when the window draws without a Flow invalidation.
    canvas
        .cached(StyleRefinement::default().size_full())
        .into_any_element()
}

impl Render for FlowState {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        viewport_element(self.canvas.clone(), window)
    }
}
/// A control element backed by [`FlowState`], reusable in host views.
#[derive(IntoElement)]
pub struct FlowCanvas {
    state: Entity<FlowState>,
}
impl FlowCanvas {
    pub fn new(state: &Entity<FlowState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}
impl RenderOnce for FlowCanvas {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        viewport_element(self.state.read(cx).canvas.clone(), window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;
    use gpui_kit::{
        InputEvent, ScrollDelta, ScrollEvent, TestAppContext, WindowOptions, point, px,
        test::TestWindowExt,
    };
    use gpui_kit::{ParentElement, Styled, div};
    struct Sibling(Rc<Cell<usize>>);
    impl Render for Sibling {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.set(self.0.get() + 1);
            div()
        }
    }
    struct Page {
        state: Entity<FlowState>,
        sibling: Entity<Sibling>,
        renders: Rc<Cell<usize>>,
    }
    impl Render for Page {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            // Deliberately read document state, as a host toolbar or inspector would.
            let _nodes = self.state.read(cx).graph().nodes.len();
            div()
                .size_full()
                .child(FlowCanvas::new(&self.state))
                .child(self.sibling.clone())
        }
    }
    #[gpui_kit::test]
    fn unchanged_draw_reuses_flow_viewport(cx: &mut TestAppContext) {
        let (window, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    FlowState::new(
                        Graph::from_parts(
                            [
                                Node::new(1, "", WorldPoint::new(20., 80.)),
                                Node::new(2, "", WorldPoint::new(400., 80.)),
                            ],
                            [crate::core::Edge::new(1, 1, 2)],
                        )
                        .expect("graph"),
                        cx,
                    )
                    .expect("flow")
                    .options(FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                })
            })
            .expect("window")
        });
        window
            .update(cx, |_, window, cx| {
                window.render_frame(cx);
                window.render_frame(cx);
            })
            .expect("warm view");
        let view = state.read_with(cx, |state, _| state.canvas.clone());
        let renders = view.read_with(cx, |view, _| view.renders);
        for _ in 0..5 {
            window
                .update(cx, |_, window, cx| window.draw(cx).clear(cx))
                .expect("unchanged draw");
        }
        assert_eq!(view.read_with(cx, |view, _| view.renders), renders);

        state.update(cx, |state, cx| {
            state
                .set_viewport(
                    Viewport {
                        pan: WorldPoint::new(20., 0.),
                        zoom: 1.,
                    },
                    cx,
                )
                .expect("changed camera");
        });
        window
            .update(cx, |_, window, cx| window.draw(cx).clear(cx))
            .expect("changed draw");
        assert!(view.read_with(cx, |view, _| view.renders) > renders);
    }

    #[gpui_kit::test]
    fn mounted_large_flow_publishes_deferred_crossing_edges(cx: &mut TestAppContext) {
        let graph = Graph::from_parts(
            [
                Node::new(1, "", WorldPoint::new(-4_000., 80.)),
                Node::new(2, "", WorldPoint::new(4_000., 80.)),
                Node::new(3, "", WorldPoint::new(5_000., 80.)),
            ],
            std::iter::once(crate::core::Edge::new(0, 1, 2))
                .chain((1..4_096).map(|id| crate::core::Edge::new(id, 2, 3))),
        )
        .expect("graph");
        let (window, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            let state = cx.new(|cx| {
                FlowState::new(graph, cx)
                    .expect("flow")
                    .options(FlowOptions {
                        minimap: false,
                        fit_on_init: false,
                        ..Default::default()
                    })
            });
            assert!(state.read(cx).editor.scene().index_pending());
            gpui_kit::open_window(WindowOptions::default(), cx, |_, _| state).expect("window")
        });
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .expect("first frame");
        cx.run_until_parked();
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .expect("indexed frame");
        state.read_with(cx, |state, _| {
            assert!(!state.editor.scene().index_pending());
            assert!(state.editor.scene().paths().is_empty());
            assert_eq!(state.visible_counts(), (0, 1));
        });
    }

    #[gpui_kit::test]
    fn review2_2_camera_frames_isolate_document_host_and_sibling(cx: &mut TestAppContext) {
        let parent_count = Rc::new(Cell::new(0));
        let sibling_count = Rc::new(Cell::new(0));
        let (w, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            let state = cx.new(|cx| {
                FlowState::new(
                    Graph::from_parts([Node::new(1, "", WorldPoint::new(80., 120.))], [])
                        .expect("graph"),
                    cx,
                )
                .expect("state")
                .options(FlowOptions {
                    fit_on_init: false,
                    ..Default::default()
                })
            });
            let (w, _) = gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                let sibling = cx.new(|_| Sibling(sibling_count.clone()));
                cx.new(|_| Page {
                    state: state.clone(),
                    sibling,
                    renders: parent_count.clone(),
                })
            })
            .expect("window");
            (w, state)
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
        })
        .expect("initial");
        let before = (parent_count.get(), sibling_count.get());
        let leaf = state.read_with(cx, |s, _| s.canvas.clone());
        let leaf_before = leaf.read_with(cx, |l, _| l.renders);
        for _ in 0..5 {
            w.update(cx, |_, w, cx| {
                w.dispatch_event(
                    ScrollEvent {
                        position: point(px(300.), px(300.)),
                        delta: ScrollDelta::Pixels(point(px(10.), px(0.))),
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
                w.draw(cx).clear(cx);
            })
            .expect("pan");
        }
        assert_eq!(
            state.read_with(cx, |s, _| s.viewport().pan),
            WorldPoint::new(50., 0.)
        );
        assert!(leaf.read_with(cx, |l, _| l.renders) > leaf_before);
        if gpui_kit::GPUI_FAST {
            assert_eq!(
                (parent_count.get(), sibling_count.get()),
                before,
                "camera did not write model/host"
            );
        }
        assert_eq!(state.read_with(cx, |s, _| s.editor.graph().nodes.len()), 1);
    }
    #[gpui_kit::test]
    fn review2_public_flow_toolbar_real_clicks_delete_selection(cx: &mut TestAppContext) {
        let (w, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    FlowState::new(
                        Graph::from_parts([Node::new(1, "", WorldPoint::new(80., 120.))], [])
                            .expect("graph"),
                        cx,
                    )
                    .expect("state")
                    .options(FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                })
            })
            .expect("window")
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
            w.click(("flow-node", 1u64), cx);
            w.click("flow-delete", cx);
            w.render_frame(cx);
        })
        .expect("clicks");
        assert!(state.read_with(cx, |s, _| s.graph().nodes.is_empty()));
        assert!(state.read_with(cx, |s, _| s.selected_nodes().is_empty()));
    }

    #[gpui_kit::test]
    fn review2_initial_fit_preserves_explicit_camera_before_layout(cx: &mut TestAppContext) {
        let viewport = Viewport {
            pan: WorldPoint::new(5., 6.),
            zoom: 1.2,
        };
        let (w, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                let state = cx.new(|cx| {
                    FlowState::new(
                        Graph::from_parts([Node::new(1, "", WorldPoint::new(3000., 100.))], [])
                            .expect("graph"),
                        cx,
                    )
                    .expect("state")
                    .options(FlowOptions::default())
                });
                state.update(cx, |state, cx| {
                    state.set_viewport(viewport, cx).expect("camera")
                });
                state
            })
            .expect("window")
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
            w.render_frame(cx);
        })
        .expect("layout");
        assert_eq!(state.read_with(cx, |state, _| state.viewport()), viewport);
    }
    #[gpui_kit::test]
    fn review2_public_model_tracks_native_drag_and_history(cx: &mut TestAppContext) {
        let (w, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    FlowState::new(
                        Graph::from_parts([Node::new(1, "", WorldPoint::new(80., 120.))], [])
                            .expect("graph"),
                        cx,
                    )
                    .expect("state")
                    .options(FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                })
            })
            .expect("window")
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
            let p = w.find(("flow-node", 1u64)).bounds().center();
            w.drag(p, p + point(px(40.), px(20.)), cx);
        })
        .expect("drag");
        assert_eq!(
            state.read_with(cx, |s, _| s.graph().nodes.get(&1).expect("node").pos),
            WorldPoint::new(120., 140.)
        );
        state.update(cx, |state, cx| state.undo(cx));
        assert_eq!(
            state.read_with(cx, |s, _| s.graph().nodes.get(&1).expect("node").pos),
            WorldPoint::new(80., 120.)
        );
        state.update(cx, |state, cx| state.redo(cx));
        assert_eq!(
            state.read_with(cx, |s, _| s.graph().nodes.get(&1).expect("node").pos),
            WorldPoint::new(120., 140.)
        );
    }
    #[gpui_kit::test]
    fn review2_1_camera_controls_preserve_selection(cx: &mut TestAppContext) {
        let (w, state) = cx.update(|cx| {
            guiraffe_core::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    FlowState::new(
                        Graph::from_parts([Node::new(1, "", WorldPoint::new(80., 120.))], [])
                            .expect("graph"),
                        cx,
                    )
                    .expect("state")
                    .options(FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                })
            })
            .expect("window")
        });
        w.update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
            w.click(("flow-node", 1u64), cx);
            w.click("flow-zoom-in", cx);
        })
        .expect("controls");
        assert_eq!(
            state.read_with(cx, |s, _| s.selected_nodes().clone()),
            BTreeSet::from([1])
        );
        assert!(state.read_with(cx, |s, _| s.viewport().zoom) > 1.);
    }
}
