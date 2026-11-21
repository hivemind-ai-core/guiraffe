//! The GPUI viewer: a diagram element with pan, wheel zoom, fit and errors.
//!
//! Rendering happens on a background task that owns the [`Engine`] while it
//! runs; the element paints the overview or image that comes back. A newer source
//! supersedes an older render, so streaming a diagram into an agent chat
//! never shows a stale drawing.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use futures_channel::oneshot;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, CursorStyle, Div, ElementId, Entity,
    ImageSource, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Render, RenderOnce, ScrollDelta, ScrollEvent, SharedString,
    StyleRefinement, Styled as _, TestSupportExt as _, WeakEntity, Window, canvas, div, img, px,
};
use guiraffe_core::{GuiraffeTheme, Scheme};

use crate::{
    engine::{Engine, Image, RenderError, Request, StagedImage, StagedPreview},
    preview::Overview,
    sizing::zoom_raster_size,
    theme::{background_for, theme_for},
    viewport::{Point, Viewport},
};

/// How the diagram currently stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// No source set yet.
    Empty,
    /// A render is in flight.
    Drawing,
    /// The last render succeeded.
    Ready,
    /// The last render failed; the message is shown instead of a diagram.
    Failed(String),
}

struct Preview {
    overview: Option<Arc<Overview>>,
    painted: mpsc::SyncSender<()>,
    anchor: (f32, f32),
    scale: f32,
}

/// State of one [`MermaidView`].
///
/// Hosts that need to drive zooming or read the SVG can keep the entity from
/// [`MermaidView::state`] and call the methods here.
/// How long zooming must pause before the diagram is re-rasterized at the new zoom.
// Reserve a view-sized initial raster for potentially staged inputs. Actual
// preview admission is based on parser-produced node and edge counts.
const STAGED_SOURCE_BYTES: usize = 512;
const INITIAL_STAGED_WIDTH: f32 = 1_280.0;

const ZOOM_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

pub struct MermaidViewState {
    id: ElementId,
    revision: u64,
    source: String,
    dark: bool,
    engine: Option<Engine>,
    viewport: Viewport,
    published: Option<Image>,
    preview: Option<Preview>,
    status: Status,
    /// Superseded renders are dropped when they land; this is the revision
    /// the view is waiting for.
    awaiting: Option<u64>,
    cancel_render: Option<Arc<AtomicBool>>,
    /// Pending re-render after zooming settles (dropping it cancels).
    zoom_refresh: Option<gpui_kit::Task<()>>,
    density: f32,
    dragging: Option<Point>,
    message: Option<SharedString>,
}

impl MermaidViewState {
    fn new(id: ElementId, engine: Engine, cx: &mut Context<Self>) -> Self {
        cx.on_release(|state, cx| {
            if let Some(image) = state.published.take() {
                cx.drop_image(image.image, None);
            }
        })
        .detach();
        Self {
            id,
            revision: 0,
            source: String::new(),
            dark: false,
            engine: Some(engine),
            viewport: Viewport::default(),
            published: None,
            preview: None,
            status: Status::Empty,
            awaiting: None,
            cancel_render: None,
            zoom_refresh: None,
            density: 1.0,
            dragging: None,
            message: None,
        }
    }

    /// Current status.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The image last drawn, if any.
    pub fn image(&self) -> Option<&Image> {
        self.published.as_ref()
    }

    fn set_published(&mut self, next: Option<Image>, cx: &mut Context<Self>) {
        let previous = std::mem::replace(&mut self.published, next);
        if let Some(previous) = previous
            && !self
                .published
                .as_ref()
                .is_some_and(|next| Arc::ptr_eq(&next.image, &previous.image))
        {
            // RenderImage sources bypass GPUI's image cache eviction. Defer
            // until the current window has rejoined the app's window map.
            cx.defer(move |cx| cx.drop_image(previous.image, None));
        }
    }

    /// Whether the current diagram has a temporary overview.
    pub fn has_preview(&self) -> bool {
        self.preview.is_some()
    }

    /// SVG of the last successful render, for copying out.
    pub fn svg(&self) -> Option<&str> {
        if self.has_preview() {
            return None;
        }
        self.image().map(|image| image.svg.as_ref())
    }

    /// The viewport, for hosts that render their own zoom controls.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// A transient message, e.g. the result of a copy.
    pub fn message(&self) -> Option<&SharedString> {
        self.message.as_ref()
    }

    /// Set a transient status message that the footer shows.
    pub fn set_message(&mut self, message: Option<SharedString>, cx: &mut Context<Self>) {
        self.message = message;
        cx.notify();
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.viewport.zoom_in();
        self.zoom_changed(cx);
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.viewport.zoom_out();
        self.zoom_changed(cx);
    }

    /// Fit the whole diagram (also the double-click action).
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.viewport.fit();
        self.zoom_changed(cx);
    }

    /// Restore actual size.
    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.viewport.reset();
        self.zoom_changed(cx);
    }

    /// Replace the source, starting a render only when it changed.
    /// Set the diagram source and scheme; the render starts if either changed.
    ///
    /// Hosts that own the state — an agent chat streaming a fence, or a page
    /// with several diagrams — call this directly and render
    /// [`MermaidView::state`] so the element does not overwrite what they set.
    pub fn set_source(&mut self, source: SharedString, dark: bool, cx: &mut Context<Self>) {
        if source.as_ref() == self.source && dark == self.dark {
            return;
        }
        if let Some(cancelled) = &self.cancel_render {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.zoom_refresh = None;
        if self.preview.take().is_some() {
            self.set_published(None, cx);
        }
        self.source = source.into();
        self.dark = dark;
        self.revision += 1;
        self.request(cx);
    }

    /// Ask the background task for the current revision.
    fn request(&mut self, cx: &mut Context<Self>) {
        self.request_at_width(self.viewport.bounds().0, cx);
    }

    fn request_at_width(&mut self, mut width: f32, cx: &mut Context<Self>) {
        if self.source.is_empty() {
            self.status = Status::Empty;
            self.set_published(None, cx);
            self.awaiting = None;
            cx.notify();
            return;
        }
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let dark = theme.scheme == Scheme::Dark;
        self.dark = dark;
        let mermaid_theme = theme_for(theme.scheme, surfaces, theme.primary);
        let staged = self.source.len() >= STAGED_SOURCE_BYTES;
        if staged && width <= 0.0 {
            width = INITIAL_STAGED_WIDTH;
        }
        let zoom = self.viewport.zoom();
        let request =
            match Request::new(&self.source, mermaid_theme, dark, width, zoom, self.density) {
                Ok(request) => request,
                Err(error) => {
                    self.status = Status::Failed(RenderError::Source(error).to_string());
                    self.set_published(None, cx);
                    self.awaiting = None;
                    cx.notify();
                    return;
                }
            };
        // Lend the engine to the task; it comes back with the result, so
        // renders serialize without a mutex and the cache stays warm.
        let Some(engine) = self.engine.take() else {
            self.awaiting = Some(self.revision);
            self.status = Status::Drawing;
            cx.notify();
            return;
        };
        let revision = self.revision;
        self.awaiting = Some(revision);
        self.status = Status::Drawing;
        cx.notify();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancel_render = Some(cancelled.clone());
        // Start worker execution immediately so placement can overlap the
        // window's initial layout and paint.
        let (preview_sender, preview_receiver) = oneshot::channel();
        let job = cx.background_executor().spawn(async move {
            let mut engine = engine;
            let mut sender = Some(preview_sender);
            let result = engine.render_staged(&request, &cancelled, |frame| {
                if let Some(sender) = sender.take() {
                    let (painted, wait_for_paint) = mpsc::sync_channel(1);
                    if sender.send((frame, painted)).is_ok() {
                        // A hidden or undrawn view must still refine.
                        let _ = wait_for_paint.recv_timeout(std::time::Duration::from_millis(100));
                    }
                }
            });
            (engine, result)
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            if let Ok((preview, painted)) = preview_receiver.await {
                this.update(cx, |this, cx| {
                    this.publish_preview(revision, preview, painted, cx)
                })
                .ok();
            }
            let (engine, result) = job.await;
            this.update(cx, |this, cx| {
                this.engine = Some(engine);
                this.cancel_render = None;
                this.complete(revision, result, cx);
            })
            .ok();
        })
        .detach();
    }

    fn publish_preview(
        &mut self,
        revision: u64,
        frame: StagedPreview,
        painted: mpsc::SyncSender<()>,
        cx: &mut Context<Self>,
    ) {
        if self.awaiting != Some(revision) {
            return;
        }
        let (overview, natural, anchor, scale) = match frame {
            StagedPreview::Native {
                overview,
                natural,
                anchor,
                scale,
            } => {
                self.set_published(None, cx);
                (Some(Arc::new(overview)), natural, anchor, scale)
            }
            StagedPreview::Raster(frame) => {
                let Some(anchor) = frame.anchor else { return };
                let natural = (frame.image.width, frame.image.height);
                self.set_published(Some(frame.image), cx);
                (None, natural, anchor, frame.scale)
            }
        };
        self.viewport.set_natural(natural.0, natural.1);
        self.preview = Some(Preview {
            overview,
            painted,
            anchor,
            scale,
        });
        cx.notify();
    }

    fn complete(
        &mut self,
        revision: u64,
        result: Result<Option<StagedImage>, RenderError>,
        cx: &mut Context<Self>,
    ) {
        if self.awaiting != Some(revision) {
            // A newer request arrived while the worker held the engine.
            if self.awaiting.is_some() {
                self.request(cx);
            }
            return;
        }
        self.awaiting = None;
        match result {
            Ok(Some(frame)) => {
                let preview = self.preview.take();
                let natural = (frame.image.width, frame.image.height);
                match (preview, frame.anchor) {
                    (Some(preview), Some(anchor)) => {
                        self.viewport.replace_natural_aligned(
                            natural,
                            preview.anchor,
                            anchor,
                            preview.scale / frame.scale,
                        );
                    }
                    _ => self.viewport.set_natural(natural.0, natural.1),
                }
                self.set_published(Some(frame.image), cx);
                self.status = Status::Ready;
                // Input may have changed the desired density while the worker
                // was busy. The published image remains available while the
                // newest zoom settles.
                self.zoom_changed(cx);
                return;
            }
            Ok(None) => return,
            Err(error) => {
                self.preview = None;
                self.status = Status::Failed(error.to_string());
                self.set_published(None, cx);
            }
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn has_pending_zoom_refresh(&self) -> bool {
        self.zoom_refresh.is_some()
    }

    /// Zoom changed: redraw now (the current raster, scaled) and, once zooming pauses,
    /// re-rasterize at the new zoom so the diagram is sharp again.
    fn zoom_changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if self.source.is_empty() {
            return;
        }
        if self.status == Status::Ready
            && let Some(image) = self.image()
        {
            let pixels = zoom_raster_size(
                image.width,
                image.height,
                self.viewport.zoom(),
                self.density,
            );
            let current = image.image.size(0);
            if pixels == (current.width.0 as u32, current.height.0 as u32) {
                self.zoom_refresh = None;
                return;
            }
        }
        let delay = cx.background_executor().timer(ZOOM_SETTLE);
        self.zoom_refresh = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            delay.await;
            this.update(cx, |this, cx| {
                this.zoom_refresh = None;
                if this.status != Status::Ready {
                    return;
                }
                this.revision += 1;
                let width = this
                    .image()
                    .map_or(this.viewport.bounds().0, |image| image.width);
                this.request_at_width(width, cx);
            })
            .ok();
        }));
    }

    /// Record the laid-out size of the view, refitting or clamping as needed.
    fn set_bounds(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        let before = self.viewport.bounds();
        self.viewport.set_bounds(width, height);
        let after = self.viewport.bounds();
        if after == before {
            return;
        }
        // The diagram is laid out for a width: re-render (in the background) when it
        // changes, e.g. a source set before first layout was rendered at width 0.
        // A window can report several initial sizes before its first stable
        // frame. Keep a large diagram's pending preview valid through those
        // changes and let the viewport fit the raster that arrives.
        let staged_in_flight = self.awaiting.is_some() && self.source.len() >= STAGED_SOURCE_BYTES;
        if (after.0 - before.0).abs() >= 1.0 && !self.source.is_empty() && !staged_in_flight {
            self.revision += 1;
            self.request(cx);
        } else {
            cx.notify();
        }
    }

    fn on_scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let delta = match event.delta {
            ScrollDelta::Lines(delta) => delta.y,
            ScrollDelta::Pixels(delta) => f32::from(delta.y) / 40.0,
        };
        if delta == 0.0 {
            return;
        }
        let anchor = Point::new(f32::from(event.position.x), f32::from(event.position.y));
        let factor = (1.0 + delta * 0.15).clamp(0.2, 5.0);
        let zoom = self.viewport.zoom();
        self.viewport.zoom_by(factor, anchor);
        if self.viewport.zoom() != zoom {
            self.zoom_changed(cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        if event.click_count >= 2 {
            self.viewport.fit();
            self.dragging = None;
            self.zoom_changed(cx);
            return;
        }
        self.dragging = Some(Point::new(
            f32::from(event.position.x),
            f32::from(event.position.y),
        ));
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.dragging = None;
            return;
        }
        let Some(from) = self.dragging else {
            return;
        };
        let to = Point::new(f32::from(event.position.x), f32::from(event.position.y));
        let before = self.viewport.offset();
        self.viewport.pan_by(to.x - from.x, to.y - from.y);
        self.dragging = Some(to);
        if self.viewport.offset() != before {
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Context<Self>) {
        self.dragging = None;
    }

    fn status_line(&self, surfaces: guiraffe_core::Surfaces) -> Div {
        let text = match &self.status {
            Status::Empty => "No diagram".to_string(),
            Status::Drawing => "Drawing…".to_string(),
            Status::Ready => format!(
                "{:.0} × {:.0}  ·  {:.0}%{}",
                self.viewport.displayed().0,
                self.viewport.displayed().1,
                self.viewport.zoom() * 100.0,
                if self.viewport.auto_fit() {
                    "  ·  fit"
                } else {
                    ""
                }
            ),
            Status::Failed(message) => message.clone(),
        };
        div()
            .flex_shrink_0()
            .px_2()
            .py_1()
            .text_size(px(11.0))
            .text_color(surfaces.dimmed.to_hsla())
            .child(text)
    }

    fn diagram(&self, scheme: Scheme, surfaces: guiraffe_core::Surfaces) -> Div {
        let page = background_for(scheme, surfaces);
        let (width, height) = self.viewport.displayed();
        let offset = self.viewport.offset();
        let content: AnyElement = match (&self.published, &self.preview, &self.status) {
            (published, preview, _) if published.is_some() || preview.is_some() => {
                let mut diagram = div()
                    .absolute()
                    .left(px(offset.x))
                    .top(px(offset.y))
                    .w(px(width))
                    .h(px(height));
                if let Some(published) = published {
                    diagram = diagram.child(
                        img(ImageSource::Render(published.image.clone()))
                            .w(px(width))
                            .h(px(height)),
                    );
                }
                if let Some(preview) = preview {
                    let overview = preview.overview.clone();
                    let painted = preview.painted.clone();
                    diagram = diagram.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, (), window, cx| {
                                if let Some(overview) = overview {
                                    overview.paint(bounds, window, cx);
                                }
                                let _ = painted.try_send(());
                            },
                        )
                        .absolute()
                        .size_full(),
                    );
                }
                diagram.into_any_element()
            }
            (_, _, Status::Failed(message)) => {
                let error = guiraffe_core::ColorName::Red;
                div()
                    .p_3()
                    .rounded(px(theme_radius()))
                    .bg(error.shade(0).to_hsla())
                    .text_color(error.shade(8).to_hsla())
                    .text_size(px(12.0))
                    .child(message.clone())
                    .into_any_element()
            }
            _ => div()
                .text_size(px(12.0))
                .text_color(surfaces.dimmed.to_hsla())
                .child("Drawing…")
                .into_any_element(),
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(page.to_hsla())
            .child(content)
    }
}

impl Drop for MermaidViewState {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.cancel_render {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

fn theme_radius() -> f32 {
    6.0
}

impl Render for MermaidViewState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let scheme = theme.scheme;
        let surfaces = theme.surfaces();
        // Report the view's size to the viewport after layout; canvas is the
        // one hook that hands back real bounds every frame.
        let bounds_entity = cx.entity().downgrade();
        let cursor = if self.viewport.is_pannable() {
            CursorStyle::OpenHand
        } else {
            CursorStyle::Arrow
        };
        div()
            .id(self.id.clone())
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .cursor(cursor)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| this.on_mouse_down(event, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| this.on_mouse_up(event, cx)),
            )
            .on_mouse_move(
                cx.listener(|this, event: &MouseMoveEvent, _, cx| this.on_mouse_move(event, cx)),
            )
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| this.on_scroll(event, cx)))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .on_children_prepainted(move |children, _, cx| {
                        let Some(bounds) = children.first() else {
                            return;
                        };
                        let (width, height) = (bounds.size.width.into(), bounds.size.height.into());
                        if let Some(state) = bounds_entity.upgrade()
                            && state.read(cx).viewport.bounds() != (width, height)
                        {
                            state.update(cx, |this, cx| this.set_bounds(width, height, cx));
                        }
                    })
                    .child(self.diagram(scheme, surfaces)),
            )
            .child(self.status_line(surfaces))
    }
}

/// The standalone Mermaid viewer element.
///
/// ```no_run
/// # use gpui_kit::*;
/// # use guiraffe_mermaid::MermaidView;
/// # fn f() -> impl IntoElement {
/// div().h(px(320.0)).child(
///     MermaidView::new("architecture")
///         .source("flowchart LR\n  ui --> core --> db"),
/// )
/// # }
/// ```
#[derive(IntoElement)]
pub struct MermaidView {
    id: ElementId,
    /// `None` leaves the driven state's source alone, so a host that owns the
    /// state (agent chat, gallery) can stream into it without this element
    /// overwriting what it was given.
    source: Option<SharedString>,
    state: Option<Entity<MermaidViewState>>,
}

impl MermaidView {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            source: None,
            state: None,
        }
    }

    /// The diagram source, in Mermaid syntax.
    pub fn source(mut self, source: impl Into<SharedString>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Drive an existing state entity instead of creating a keyed one.
    ///
    /// Use this when the host needs to zoom, fit or read the SVG through
    /// [`MermaidViewState`]. The state's id is used for the element.
    pub fn state(mut self, state: &Entity<MermaidViewState>) -> Self {
        self.state = Some(state.clone());
        self
    }

    /// Create a state entity a host can keep and drive.
    pub fn create_state(id: impl Into<ElementId>, cx: &mut App) -> Entity<MermaidViewState> {
        let id = id.into();
        let engine = Engine::new(cx.svg_renderer());
        cx.new(|cx| MermaidViewState::new(id, engine, cx))
    }
}

impl RenderOnce for MermaidView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let MermaidView { id, source, state } = self;
        let state = match state {
            Some(state) => state,
            None => window.use_keyed_state(
                ElementId::NamedChild(std::sync::Arc::new(id.clone()), "state".into()),
                cx,
                |_, cx| MermaidViewState::new(id.clone(), Engine::new(cx.svg_renderer()), cx),
            ),
        };
        if let Some(source) = source {
            state.update(cx, |state, cx| {
                let dark = GuiraffeTheme::global(cx).scheme == Scheme::Dark;
                state.set_source(source, dark, cx);
            });
        }
        // The state fills the slot and can reuse its paint when the host redraws.
        AnyView::from(state).cached(StyleRefinement::default().size_full())
    }
}
