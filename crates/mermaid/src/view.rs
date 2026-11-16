//! The GPUI viewer: a diagram element with pan, wheel zoom, fit and errors.
//!
//! Rendering happens on a background task that owns the [`Engine`] while it
//! runs; the element only paints the image that comes back. A newer source
//! supersedes an older render, so streaming a diagram into an agent chat
//! never shows a stale drawing.

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, CursorStyle, Div, ElementId, Entity, ImageSource,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Render, RenderOnce, ScrollDelta, ScrollEvent, SharedString,
    Styled as _, TestSupportExt as _, WeakEntity, Window, div, img, px,
};
use guiraffe_core::{GuiraffeTheme, Scheme};

use crate::{
    engine::{Engine, Image, RenderError, Request},
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

#[derive(Clone)]
struct Published {
    image: Image,
}

/// State of one [`MermaidView`].
///
/// Hosts that need to drive zooming or read the SVG can keep the entity from
/// [`MermaidView::state`] and call the methods here.
pub struct MermaidViewState {
    id: ElementId,
    revision: u64,
    source: String,
    dark: bool,
    engine: Option<Engine>,
    viewport: Viewport,
    published: Option<Published>,
    status: Status,
    /// Superseded renders are dropped when they land; this is the revision
    /// the view is waiting for.
    awaiting: Option<u64>,
    density: f32,
    dragging: Option<Point>,
    message: Option<SharedString>,
}

impl MermaidViewState {
    fn new(id: ElementId, engine: Engine) -> Self {
        Self {
            id,
            revision: 0,
            source: String::new(),
            dark: false,
            engine: Some(engine),
            viewport: Viewport::default(),
            published: None,
            status: Status::Empty,
            awaiting: None,
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
        self.published.as_ref().map(|published| &published.image)
    }

    /// SVG of the last successful render, for copying out.
    pub fn svg(&self) -> Option<&str> {
        self.published
            .as_ref()
            .map(|published| published.image.svg.as_ref())
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
        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.viewport.zoom_out();
        cx.notify();
    }

    /// Fit the whole diagram (also the double-click action).
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.viewport.fit();
        cx.notify();
    }

    /// Restore actual size.
    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.viewport.reset();
        cx.notify();
    }

    /// Replace the source, starting a render only when it changed.
    /// Set the diagram source and scheme; the render starts if either changed.
    ///
    /// Hosts that own the state — an agent chat streaming a fence, or a page
    /// with several diagrams — call this directly and render
    /// [`MermaidView::state`] so the element does not overwrite what they set.
    pub fn set_source(&mut self, source: SharedString, dark: bool, cx: &mut Context<Self>) {
        let source: String = source.into();
        if source == self.source && dark == self.dark {
            return;
        }
        self.source = source;
        self.dark = dark;
        self.revision += 1;
        self.request(cx);
    }

    /// Ask the background task for the current revision.
    fn request(&mut self, cx: &mut Context<Self>) {
        if self.source.is_empty() {
            self.status = Status::Empty;
            self.published = None;
            self.awaiting = None;
            cx.notify();
            return;
        }
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let dark = theme.scheme == Scheme::Dark;
        self.dark = dark;
        let mermaid_theme = theme_for(theme.scheme, surfaces, theme.primary);
        let width = self.viewport.bounds().0;
        let zoom = self.viewport.zoom();
        let request =
            match Request::new(&self.source, mermaid_theme, dark, width, zoom, self.density) {
                Ok(request) => request,
                Err(error) => {
                    self.status = Status::Failed(RenderError::Source(error).to_string());
                    self.published = None;
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
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let (engine, result) = cx
                .background_executor()
                .spawn(async move {
                    let mut engine = engine;
                    let result = engine.render(&request);
                    (engine, result)
                })
                .await;
            this.update(cx, |this, cx| {
                this.engine = Some(engine);
                this.complete(revision, result, cx);
            })
            .ok();
        })
        .detach();
    }

    fn complete(
        &mut self,
        revision: u64,
        result: Result<Image, RenderError>,
        cx: &mut Context<Self>,
    ) {
        if self.awaiting != Some(revision) {
            // Superseded while this render held the engine: the view has moved
            // on, and nobody else could have started the newer render, so
            // start it now.
            self.request(cx);
            return;
        }
        self.awaiting = None;
        match result {
            Ok(image) => {
                self.viewport.set_natural(image.width, image.height);
                self.published = Some(Published { image });
                self.status = Status::Ready;
            }
            Err(error) => {
                self.status = Status::Failed(error.to_string());
                self.published = None;
            }
        }
        cx.notify();
    }

    /// Record the laid-out size of the view, refitting or clamping as needed.
    fn set_bounds(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        let before = self.viewport.bounds();
        self.viewport.set_bounds(width, height);
        if self.viewport.bounds() != before {
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
            cx.notify();
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        if event.click_count >= 2 {
            self.viewport.fit();
            self.dragging = None;
            cx.notify();
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
        let content: AnyElement = match (&self.published, &self.status) {
            (Some(published), _) => div()
                .absolute()
                .left(px(offset.x))
                .top(px(offset.y))
                .w(px(width))
                .h(px(height))
                .child(
                    img(ImageSource::Render(published.image.image.clone()))
                        .w(px(width))
                        .h(px(height)),
                )
                .into_any_element(),
            (None, Status::Failed(message)) => {
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
            (None, _) => div()
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
                    .flex_1()
                    .min_h_0()
                    .on_children_prepainted(move |children, _, cx| {
                        let Some(bounds) = children.first() else {
                            return;
                        };
                        let (width, height) = (bounds.size.width.into(), bounds.size.height.into());
                        bounds_entity
                            .update(cx, |this, cx| this.set_bounds(width, height, cx))
                            .ok();
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
        cx.new(|_| MermaidViewState::new(id, engine))
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
                |_, cx| MermaidViewState::new(id.clone(), Engine::new(cx.svg_renderer())),
            ),
        };
        if let Some(source) = source {
            state.update(cx, |state, cx| {
                let dark = GuiraffeTheme::global(cx).scheme == Scheme::Dark;
                state.set_source(source, dark, cx);
            });
        }
        // The state paints itself; wrapping keeps this element's own size
        // under the host's layout.
        div().size_full().child(state)
    }
}
