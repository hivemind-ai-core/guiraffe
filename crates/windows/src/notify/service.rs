//! The notification-window service: one borderless always-on-top surface per app, anchored to
//! a screen corner, holding the card stack.
use std::{collections::HashMap, rc::Rc, time::Duration};

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, Global, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Task,
    Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowHandle, WindowKind,
    WindowOptions, div, point, px, size,
};
use guiraffe_motion::{Animated, MotionPrefs, Timing};

use super::{
    card::{NotificationCard, metrics},
    model::{Pushed, StackModel, WindowNotification, WindowNotificationId},
    stack::{StackConfig, StackLayout, layer_anchor, resize_keeps_corner, surface_origin},
    timer::Clock,
};
use crate::{WindowManager, placement::Rect};

/// How the notification surface is placed on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// Layer-shell on Wayland, a positioned pop-up window elsewhere.
    #[default]
    Auto,
    /// A Wayland `wlr-layer-shell` overlay surface anchored to the corner. Falls back to
    /// [`Placement::Positioned`] when the compositor lacks the protocol.
    LayerShell,
    /// A borderless pop-up window positioned at the corner of the primary display's work area.
    /// On Wayland without layer-shell the compositor chooses the position.
    Positioned,
}

/// The placement actually in use (after `Auto` resolution and fallbacks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementUsed {
    LayerShell,
    Positioned,
    /// Positioned was requested on Wayland or layer-shell was unavailable: the window opened but
    /// the compositor decides where (and whether it stays on top).
    CompositorPlaced(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationWindowsConfig {
    pub stack: StackConfig,
    pub placement: Placement,
    /// Layer-shell namespace / window app id, for compositor rules.
    pub namespace: String,
    /// Pause every card's countdown while the pointer is over the stack.
    pub pause_on_hover: bool,
}

impl Default for NotificationWindowsConfig {
    fn default() -> Self {
        Self {
            stack: StackConfig::default(),
            placement: Placement::Auto,
            namespace: "guiraffe-notifications".into(),
            pause_on_hover: true,
        }
    }
}

/// Why a card went away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DismissReason {
    /// Its countdown ran out.
    Timeout,
    /// The close button.
    Closed,
    /// The card body was clicked (`dismiss_on_click`).
    Clicked,
    /// An action button was pressed.
    Action,
    /// [`NotificationWindows::dismiss`] / [`NotificationWindows::clear`].
    App,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowNotificationEventKind {
    /// Became visible (also when promoted from the queue).
    Shown,
    /// The body was clicked.
    Clicked,
    /// An action button with this key was pressed.
    Action(String),
    /// Removed. Terminal.
    Dismissed(DismissReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowNotificationEvent {
    pub id: WindowNotificationId,
    pub kind: WindowNotificationEventKind,
}

type Handler = Rc<dyn Fn(&WindowNotificationEvent, &mut App)>;

/// The clock used by the service: the GPUI executor's, so test executors can advance it.
struct ExecutorClock {
    executor: gpui_kit::BackgroundExecutor,
    origin: std::time::Instant,
}
impl Clock for ExecutorClock {
    fn now(&self) -> Duration {
        self.executor.now().saturating_duration_since(self.origin)
    }
}

struct Surface {
    handle: WindowHandle<StackView>,
    size: (f32, f32),
    layer: bool,
}

/// Service state (an entity; subscribe for [`WindowNotificationEvent`]s).
pub struct NotificationStack {
    config: NotificationWindowsConfig,
    clock: Rc<dyn Clock>,
    model: StackModel,
    next_id: u64,
    handlers: HashMap<WindowNotificationId, Handler>,
    surface: Option<Surface>,
    placement_used: Option<PlacementUsed>,
    last_error: Option<String>,
    timer: Option<Task<()>>,
    leaving: HashMap<WindowNotificationId, Task<()>>,
    open_pending: bool,
}

impl EventEmitter<WindowNotificationEvent> for NotificationStack {}

impl NotificationStack {
    pub fn model(&self) -> &StackModel {
        &self.model
    }
    pub fn config(&self) -> &NotificationWindowsConfig {
        &self.config
    }
    /// The surface's window, while any card is visible.
    pub fn window(&self) -> Option<WindowHandle<StackView>> {
        self.surface.as_ref().map(|s| s.handle)
    }
    pub fn placement_used(&self) -> Option<&PlacementUsed> {
        self.placement_used.as_ref()
    }
    /// The last error opening or resizing the surface.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
    /// The current geometry (surface size and card slots).
    pub fn layout(&self) -> StackLayout {
        self.model.layout(&self.config.stack, &metrics())
    }

    fn now(&self) -> Duration {
        self.clock.now()
    }

    fn emit(
        &mut self,
        id: WindowNotificationId,
        kind: WindowNotificationEventKind,
        cx: &mut Context<Self>,
    ) {
        let terminal = matches!(kind, WindowNotificationEventKind::Dismissed(_));
        let event = WindowNotificationEvent { id, kind };
        let handler = if terminal {
            self.handlers.remove(&id)
        } else {
            self.handlers.get(&id).cloned()
        };
        if let Some(handler) = handler {
            let event = event.clone();
            cx.defer(move |cx| handler(&event, cx));
        }
        cx.emit(event);
    }

    fn push(
        &mut self,
        id: WindowNotificationId,
        content: WindowNotification,
        cx: &mut Context<Self>,
    ) {
        let now = self.now();
        if self.model.push(id, content, now) == Pushed::Shown {
            self.emit(id, WindowNotificationEventKind::Shown, cx);
        }
        // A revived card must not be removed by its pending exit.
        self.leaving.remove(&id);
        self.changed(cx);
    }

    /// Start removing `id`; it leaves after the exit animation (instantly with reduced motion).
    fn leave(
        &mut self,
        id: WindowNotificationId,
        reason: DismissReason,
        cx: &mut Context<Self>,
    ) -> bool {
        let visible = self.model.visible().iter().any(|c| c.id == id);
        if !self.model.begin_leave(id) {
            return false;
        }
        self.emit(id, WindowNotificationEventKind::Dismissed(reason), cx);
        if visible {
            let exit = exit_duration(cx);
            if exit.is_zero() {
                self.finish_leave(id, cx);
            } else {
                let timer = cx.background_executor().timer(exit);
                let task = cx.spawn(async move |this, cx| {
                    timer.await;
                    let _ = this.update(cx, |stack, cx| {
                        if let Some(task) = stack.leaving.remove(&id) {
                            // We are this task: let it run out.
                            task.detach();
                        }
                        stack.finish_leave(id, cx);
                    });
                });
                self.leaving.insert(id, task);
            }
        }
        self.changed(cx);
        true
    }

    fn finish_leave(&mut self, id: WindowNotificationId, cx: &mut Context<Self>) {
        let now = self.now();
        for promoted in self.model.finish_leave(id, now) {
            self.emit(promoted, WindowNotificationEventKind::Shown, cx);
        }
        self.changed(cx);
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if !self.config.pause_on_hover {
            return;
        }
        let now = self.now();
        if self.model.set_paused(hovered, now) {
            self.reschedule(cx);
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = self.now();
        for id in self.model.expired(now) {
            self.leave(id, DismissReason::Timeout, cx);
        }
        self.reschedule(cx);
    }

    fn reschedule(&mut self, cx: &mut Context<Self>) {
        let now = self.now();
        self.timer = self.model.next_deadline(now).map(|deadline| {
            let timer = cx.background_executor().timer(deadline.saturating_sub(now));
            cx.spawn(async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |stack, cx| {
                    // Keep this (running) task alive until it returns.
                    if let Some(task) = stack.timer.take() {
                        task.detach();
                    }
                    stack.tick(cx);
                });
            })
        });
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.reschedule(cx);
        self.sync_surface(cx);
        cx.notify();
    }

    /// Open, resize, move or close the surface to fit the visible cards.
    fn sync_surface(&mut self, cx: &mut Context<Self>) {
        let layout = self.layout();
        if layout.is_empty() {
            if let Some(surface) = self.surface.take() {
                close_window(surface.handle, cx);
            }
            return;
        }
        let new_size = (layout.width, layout.height);
        let inputs = input_region(&layout);
        if let Some(surface) = &mut self.surface {
            if same_size(surface.size, new_size) {
                set_input_region(surface.handle, inputs, cx);
                return;
            }
            let keeps = surface.layer
                || resize_keeps_corner(self.config.stack.corner, surface.size, new_size);
            if keeps {
                surface.size = new_size;
                let handle = surface.handle;
                let region = inputs.clone();
                let resized = handle.update(cx, move |_, window, _| {
                    window.resize(size(px(new_size.0), px(new_size.1)));
                    window.set_input_region(Some(&region));
                });
                if resized.is_ok() {
                    return;
                }
            }
            // The origin must move: platforms without window moves need a new window.
            if let Some(old) = self.surface.take() {
                close_window(old.handle, cx);
            }
        }
        // Opening renders the new window, which reads this entity: do it outside the update.
        if !self.open_pending {
            self.open_pending = true;
            let this = cx.entity();
            cx.defer(move |cx| open_deferred(this, cx));
        }
    }
}

fn open_deferred(this: Entity<NotificationStack>, cx: &mut App) {
    let (config, layout, already) = {
        let s = this.read(cx);
        (s.config.clone(), s.layout(), s.surface.is_some())
    };
    if already || layout.is_empty() {
        this.update(cx, |s, _| s.open_pending = false);
        return;
    }
    let inputs = input_region(&layout);
    let opened = open_surface(&config, this.clone(), (layout.width, layout.height), cx);
    if let Ok((surface, _)) = &opened {
        // The surface never keeps the app alive or blocks "quit on last window".
        WindowManager::mark_auxiliary(surface.handle.into(), cx);
        set_input_region(surface.handle, inputs, cx);
    }
    this.update(cx, |s, cx| {
        s.open_pending = false;
        match opened {
            Ok((surface, used)) => {
                s.surface = Some(surface);
                s.placement_used = Some(used);
                s.last_error = None;
            }
            Err(e) => s.last_error = Some(format!("{e:#}")),
        }
        // Cards may have changed meanwhile.
        s.sync_surface(cx);
    });
}

fn open_surface(
    config: &NotificationWindowsConfig,
    stack: Entity<NotificationStack>,
    (w, h): (f32, f32),
    cx: &mut App,
) -> anyhow::Result<(Surface, PlacementUsed)> {
    let wayland = cfg!(target_os = "linux") && cx.compositor_name() == "Wayland";
    let want_layer = match config.placement {
        Placement::Auto => wayland,
        Placement::LayerShell => true,
        Placement::Positioned => false,
    };
    let used;
    if want_layer {
        match open_layer(config, w, h, stack.clone(), cx) {
            Ok(handle) => {
                return Ok((
                    Surface {
                        handle,
                        size: (w, h),
                        layer: true,
                    },
                    PlacementUsed::LayerShell,
                ));
            }
            Err(e) => {
                used = PlacementUsed::CompositorPlaced(format!(
                    "layer-shell unavailable ({e:#}); the compositor positions the window"
                ));
            }
        }
    } else {
        used = if wayland {
            PlacementUsed::CompositorPlaced(
                "Wayland clients cannot position their windows; the compositor places it".into(),
            )
        } else {
            PlacementUsed::Positioned
        };
    }
    {
        let area = cx
            .primary_display()
            .map(|d| {
                let b = d.visible_bounds();
                Rect::new(
                    b.origin.x.into(),
                    b.origin.y.into(),
                    b.size.width.into(),
                    b.size.height.into(),
                )
            })
            .unwrap_or(Rect::new(0.0, 0.0, 1280.0, 800.0));
        let (x, y) = surface_origin(&config.stack, area, w, h);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(x), px(y)),
                size(px(w), px(h)),
            ))),
            kind: WindowKind::PopUp,
            ..surface_options(config)
        };
        let handle = cx.open_window(options, move |_, cx| cx.new(StackView::new(stack)))?;
        Ok((
            Surface {
                handle,
                size: (w, h),
                layer: false,
            },
            used,
        ))
    }
}

#[cfg(target_os = "linux")]
fn open_layer(
    config: &NotificationWindowsConfig,
    w: f32,
    h: f32,
    stack: Entity<NotificationStack>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<StackView>> {
    use gpui_kit::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
    let ([top, right, bottom, left], [mt, mr, mb, ml]) = layer_anchor(&config.stack);
    let mut anchor = Anchor::empty();
    for (on, edge) in [
        (top, Anchor::TOP),
        (right, Anchor::RIGHT),
        (bottom, Anchor::BOTTOM),
        (left, Anchor::LEFT),
    ] {
        if on {
            anchor |= edge;
        }
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(w), px(h)),
        ))),
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: config.namespace.clone(),
            layer: Layer::Overlay,
            anchor,
            exclusive_zone: None,
            exclusive_edge: None,
            margin: Some((px(mt), px(mr), px(mb), px(ml))),
            keyboard_interactivity: KeyboardInteractivity::None,
        }),
        ..surface_options(config)
    };
    cx.open_window(options, move |_, cx| cx.new(StackView::new(stack)))
}

#[cfg(not(target_os = "linux"))]
fn open_layer(
    _: &NotificationWindowsConfig,
    _: f32,
    _: f32,
    _: Entity<NotificationStack>,
    _: &mut App,
) -> anyhow::Result<WindowHandle<StackView>> {
    anyhow::bail!("layer-shell is Wayland-only")
}

fn surface_options(config: &NotificationWindowsConfig) -> WindowOptions {
    WindowOptions {
        titlebar: None,
        focus: false,
        show: true,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        app_id: Some(config.namespace.clone()),
        ..Default::default()
    }
}

fn exit_duration(cx: &App) -> Duration {
    let prefs = MotionPrefs::global(cx);
    if prefs.reduced {
        return Duration::ZERO;
    }
    let base = match prefs.tokens.fast() {
        Timing::Tween { duration, .. } => duration,
        Timing::Spring(_) => Duration::from_millis(250),
    };
    base.mul_f32(prefs.time_scale.max(0.0))
}

fn same_size(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}

fn input_region(layout: &StackLayout) -> Vec<Bounds<gpui_kit::Pixels>> {
    layout
        .slots
        .iter()
        .map(|r| Bounds::new(point(px(r.x), px(r.y)), size(px(r.w), px(r.h))))
        .collect()
}

fn set_input_region(
    handle: WindowHandle<StackView>,
    rects: Vec<Bounds<gpui_kit::Pixels>>,
    cx: &mut App,
) {
    // Wayland only (elsewhere a no-op): clicks between cards reach the windows below.
    let _ = handle.update(cx, move |_, window, _| {
        window.set_input_region(Some(&rects))
    });
}

fn close_window(handle: WindowHandle<StackView>, cx: &mut App) {
    // Deferred: the window may be the one dispatching the click that closed the last card.
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    });
}

/// The surface's root view: renders the visible cards in their slots, animating entry and exit.
pub struct StackView {
    stack: Entity<NotificationStack>,
    /// Per-card visibility 0 → 1 (enter) and → 0 (exit).
    presence: HashMap<WindowNotificationId, (u64, Animated<f32>)>,
    _observe: Subscription,
}

impl StackView {
    fn new(stack: Entity<NotificationStack>) -> impl FnOnce(&mut Context<Self>) -> Self {
        move |cx| {
            let observe = cx.observe(&stack, |_, _, cx| cx.notify());
            Self {
                stack,
                presence: HashMap::new(),
                _observe: observe,
            }
        }
    }
}

impl Render for StackView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stack = self.stack.read(cx);
        let layout = stack.layout();
        let sign = stack.config.stack.corner.slide_sign();
        let cards: Vec<_> = stack
            .model
            .visible()
            .iter()
            .map(|c| (c.id, c.revision, c.leaving, c.content.clone()))
            .collect();
        let prefs = MotionPrefs::global(cx);
        let enter = prefs.tokens.enter();
        let exit = prefs.tokens.fast();
        self.presence
            .retain(|id, _| cards.iter().any(|c| c.0 == *id));
        let mut children = Vec::with_capacity(cards.len());
        let slide = metrics().base * 0.3;
        for ((id, revision, leaving, content), slot) in cards.into_iter().zip(layout.slots) {
            let entry = self.presence.entry(id).or_insert_with(|| {
                let mut a = Animated::new(0.0, enter);
                a.set(1.0, cx);
                (revision, a)
            });
            entry.0 = revision;
            let target = if leaving { 0.0 } else { 1.0 };
            if *entry.1.target() != target {
                entry.1.set_timing(if leaving { exit } else { enter });
                entry.1.set(target, cx);
            }
            let v = entry.1.sample(window, cx).clamp(0.0, 1.0);
            let handle = self.stack.clone();
            let on_click = {
                let handle = handle.clone();
                move |_: &mut Window, cx: &mut App| {
                    handle.update(cx, |s, cx| {
                        s.emit(id, WindowNotificationEventKind::Clicked, cx);
                        s.leave(id, DismissReason::Clicked, cx);
                    });
                }
            };
            let on_action = {
                let handle = handle.clone();
                move |key: &str, _: &mut Window, cx: &mut App| {
                    let key = key.to_string();
                    handle.update(cx, |s, cx| {
                        s.emit(id, WindowNotificationEventKind::Action(key), cx);
                        s.leave(id, DismissReason::Action, cx);
                    });
                }
            };
            let on_close = move |_: &mut Window, cx: &mut App| {
                handle.update(cx, |s, cx| {
                    s.leave(id, DismissReason::Closed, cx);
                });
            };
            children.push(
                div()
                    .absolute()
                    .top(px(slot.y))
                    .left(px(slot.x + (1.0 - v) * slide * sign))
                    .w(px(slot.w))
                    .h(px(slot.h))
                    .opacity(v)
                    .child(
                        NotificationCard::new(("notification", id.0), content)
                            .test_id(format!("notification-{}", id.0))
                            .on_click(on_click)
                            .on_action(on_action)
                            .on_close(on_close),
                    ),
            );
        }
        let hover = self.stack.clone();
        div()
            .id("guiraffe-notification-stack")
            .debug_selector(|| "notification-stack".into())
            .size_full()
            .relative()
            .on_hover(move |hovered, _, cx| {
                let hovered = *hovered;
                hover.update(cx, |s, cx| s.set_hovered(hovered, cx));
            })
            .children(children)
    }
}

#[derive(Clone)]
struct Installed(Entity<NotificationStack>);
impl Global for Installed {}

/// Handle to the notification-window service. Cheap to clone.
///
/// ```ignore
/// guiraffe_windows::notify::init(NotificationWindowsConfig::default(), cx);
/// let service = NotificationWindows::global(cx);
/// let id = service.show(WindowNotification::new("Exported").body("report.pdf"), cx);
/// ```
#[derive(Clone)]
pub struct NotificationWindows {
    stack: Entity<NotificationStack>,
}

/// Install the service with the GPUI executor's clock. Re-initializing replaces the config and
/// keeps visible cards.
pub fn init(config: NotificationWindowsConfig, cx: &mut App) -> NotificationWindows {
    let clock = Rc::new(ExecutorClock {
        executor: cx.background_executor().clone(),
        origin: cx.background_executor().now(),
    });
    init_with_clock(config, clock, cx)
}

/// Install the service with an explicit clock (tests).
pub fn init_with_clock(
    config: NotificationWindowsConfig,
    clock: Rc<dyn Clock>,
    cx: &mut App,
) -> NotificationWindows {
    if let Some(Installed(stack)) = cx.try_global::<Installed>().cloned() {
        stack.update(cx, |s, cx| {
            let now = s.now();
            s.config = config;
            for id in s.model.set_max_visible(s.config.stack.max_visible, now) {
                s.emit(id, WindowNotificationEventKind::Shown, cx);
            }
            // Corner or size may have changed: rebuild the surface.
            if let Some(surface) = s.surface.take() {
                close_window(surface.handle, cx);
            }
            s.changed(cx);
        });
        return NotificationWindows { stack };
    }
    let max = config.stack.max_visible;
    let stack = cx.new(|_| NotificationStack {
        config,
        clock,
        model: StackModel::new(max),
        next_id: 0,
        handlers: HashMap::new(),
        surface: None,
        placement_used: None,
        last_error: None,
        timer: None,
        leaving: HashMap::new(),
        open_pending: false,
    });
    cx.set_global(Installed(stack.clone()));
    NotificationWindows { stack }
}

impl NotificationWindows {
    /// The installed service, installing the default one if needed.
    pub fn global(cx: &mut App) -> Self {
        match cx.try_global::<Installed>() {
            Some(Installed(stack)) => Self {
                stack: stack.clone(),
            },
            None => init(NotificationWindowsConfig::default(), cx),
        }
    }
    pub fn entity(&self) -> &Entity<NotificationStack> {
        &self.stack
    }
    /// Show a card (or queue it when `max_visible` are showing).
    pub fn show(&self, content: WindowNotification, cx: &mut App) -> WindowNotificationId {
        self.stack.update(cx, |s, cx| {
            s.next_id += 1;
            let id = WindowNotificationId(s.next_id);
            s.push(id, content, cx);
            id
        })
    }
    /// Show a card and call `handler` with its events.
    pub fn show_with(
        &self,
        content: WindowNotification,
        handler: impl Fn(&WindowNotificationEvent, &mut App) + 'static,
        cx: &mut App,
    ) -> WindowNotificationId {
        self.stack.update(cx, |s, cx| {
            s.next_id += 1;
            let id = WindowNotificationId(s.next_id);
            s.handlers.insert(id, Rc::new(handler));
            s.push(id, content, cx);
            id
        })
    }
    /// Replace a card's content in place (restarting its countdown). Updating an id that is
    /// gone shows it again.
    pub fn update(&self, id: WindowNotificationId, content: WindowNotification, cx: &mut App) {
        self.stack.update(cx, |s, cx| {
            s.next_id = s.next_id.max(id.0);
            s.push(id, content, cx)
        });
    }
    /// Dismiss a card. Returns false if it isn't showing or queued.
    pub fn dismiss(&self, id: WindowNotificationId, cx: &mut App) -> bool {
        self.stack
            .update(cx, |s, cx| s.leave(id, DismissReason::App, cx))
    }
    /// Dismiss every card, queued ones included.
    pub fn clear(&self, cx: &mut App) {
        self.stack.update(cx, |s, cx| {
            let ids: Vec<_> = s
                .model
                .queued()
                .map(|c| c.id)
                .chain(s.model.visible().iter().map(|c| c.id))
                .collect();
            for id in ids {
                s.leave(id, DismissReason::App, cx);
            }
        });
    }
    pub fn subscribe(
        &self,
        cx: &mut App,
        f: impl Fn(&WindowNotificationEvent, &mut App) + 'static,
    ) -> Subscription {
        cx.subscribe(&self.stack, move |_, e, cx| f(e, cx))
    }
}
