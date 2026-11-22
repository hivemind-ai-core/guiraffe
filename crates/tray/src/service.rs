//! The tray service: state, typed events, command routing.
use std::{rc::Rc, time::Instant};

use futures::{StreamExt as _, channel::mpsc};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, Entity, EventEmitter, Subscription, Task,
};
use guiraffe_commands::{Commands, InvocationSource};

use crate::{
    TrayError,
    backend::{BackendEvent, EventSink, TrayBackend, TrayState},
    click::{ClickKind, ClickTracker},
    icon::{Badge, TrayIcon},
    menu::{MenuEntry, TrayItemId, TrayMenuItem, build_menu},
};

/// What the user did with the tray icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// Primary click (left click; SNI "activate").
    Click,
    /// Second click within the double-click interval (after a [`TrayEvent::Click`]).
    DoubleClick,
    /// Middle click (SNI "secondary activate").
    MiddleClick,
    Scroll {
        delta: i32,
        horizontal: bool,
    },
    /// A menu item was chosen. Command items are also executed when routed
    /// ([`Tray::route_commands`]).
    MenuItem(TrayItemId),
    /// The icon is registered with the platform.
    Ready,
    /// The tray host (panel) appeared or went away; the icon comes back when it returns.
    HostAvailable(bool),
    /// Connecting or a later operation failed.
    Error(TrayError),
}

/// Connection status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayStatus {
    Connecting,
    Ready,
    Failed(TrayError),
    Removed,
}

/// Initial tray content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayOptions {
    /// Stable app id (used by SNI hosts to remember placement).
    pub id: String,
    pub title: String,
    pub tooltip_title: String,
    pub tooltip_body: String,
    pub icon: Option<TrayIcon>,
    pub menu: Vec<TrayMenuItem>,
    /// Double-click interval.
    pub double_click: std::time::Duration,
    pub badge_color: [u8; 4],
}

impl Default for TrayOptions {
    fn default() -> Self {
        Self {
            id: "guiraffe-app".into(),
            title: "GUIraffe".into(),
            tooltip_title: "GUIraffe".into(),
            tooltip_body: String::new(),
            icon: None,
            menu: Vec::new(),
            double_click: std::time::Duration::from_millis(400),
            // Mantine red.7
            badge_color: [224, 49, 49, 255],
        }
    }
}

struct Router {
    commands: Commands,
    window: AnyWindowHandle,
}

type Restore = Rc<dyn Fn(&mut App)>;

/// Tray state (an entity emitting [`TrayEvent`]s).
pub struct TrayService {
    state: TrayState,
    backend: Box<dyn TrayBackend>,
    status: TrayStatus,
    clicks: ClickTracker,
    origin: Instant,
    router: Option<Router>,
    /// Set by the minimize-to-tray helper: what a click does while the window is hidden.
    pub(crate) restore: Option<Restore>,
    pub(crate) hidden: bool,
    _connect: Option<Task<()>>,
    _events: Option<Task<()>>,
}

impl EventEmitter<TrayEvent> for TrayService {}

impl TrayService {
    pub fn state(&self) -> &TrayState {
        &self.state
    }
    pub fn status(&self) -> &TrayStatus {
        &self.status
    }
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }
    /// Whether the minimize-to-tray helper has the window hidden.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        if self.status == TrayStatus::Ready {
            self.backend.update(self.state.clone(), cx);
        }
        cx.notify();
    }

    fn handle(&mut self, raw: BackendEvent, cx: &mut Context<Self>) {
        let now = cx
            .background_executor()
            .now()
            .saturating_duration_since(self.origin);
        let event = match raw {
            BackendEvent::Activate => match self.clicks.activate(now) {
                ClickKind::Single => TrayEvent::Click,
                ClickKind::Double => TrayEvent::DoubleClick,
            },
            BackendEvent::DoubleActivate => {
                self.clicks.native_double();
                TrayEvent::DoubleClick
            }
            BackendEvent::SecondaryActivate => TrayEvent::MiddleClick,
            BackendEvent::Scroll { delta, horizontal } => TrayEvent::Scroll { delta, horizontal },
            BackendEvent::MenuItem(id) => TrayEvent::MenuItem(id),
            BackendEvent::HostAvailable(on) => TrayEvent::HostAvailable(on),
        };
        if event == TrayEvent::Click
            && self.hidden
            && let Some(restore) = self.restore.clone()
        {
            self.hidden = false;
            cx.defer(move |cx| restore(cx));
        }
        if let TrayEvent::MenuItem(TrayItemId::Command(id)) = &event
            && let Some(router) = &self.router
        {
            let commands = router.commands.clone();
            let window = router.window;
            let id = id.clone();
            let this = cx.entity().downgrade();
            cx.defer(move |cx| {
                let result = window.update(cx, |_, window, cx| {
                    let contexts = commands.entity().read(cx).contexts().clone();
                    commands.execute(&id, &contexts, InvocationSource::Menu, window, cx)
                });
                let error = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => Some(TrayError::Command(e.to_string())),
                    Err(_) => Some(TrayError::Command(format!(
                        "{id}: the target window is closed"
                    ))),
                };
                if let Some(error) = error {
                    let _ = this.update(cx, |_, cx| cx.emit(TrayEvent::Error(error)));
                }
            });
        }
        cx.emit(event);
    }
}

/// Handle to a tray icon. Cheap to clone; the icon is removed by [`Tray::remove`] (or when the
/// last handle and every subscription to it are dropped).
#[derive(Clone)]
pub struct Tray {
    entity: Entity<TrayService>,
}

impl Tray {
    /// Create a tray icon on the platform backend (feature `native`); without it, or on
    /// unsupported platforms, the status becomes [`TrayStatus::Failed`] with
    /// [`TrayError::Unsupported`].
    pub fn new(options: TrayOptions, cx: &mut App) -> Self {
        Self::with_backend(options, crate::platform_backend(), cx)
    }

    /// Create a tray icon on `backend` (tests use [`MockTray`](crate::MockTray)).
    pub fn with_backend(
        options: TrayOptions,
        backend: Option<Box<dyn TrayBackend>>,
        cx: &mut App,
    ) -> Self {
        let state = TrayState {
            id: options.id,
            title: options.title,
            tooltip_title: options.tooltip_title,
            tooltip_body: options.tooltip_body,
            icon: options.icon,
            badge: Badge::None,
            badge_color: options.badge_color,
            attention: false,
            menu: options.menu,
        };
        let double_click = options.double_click;
        let entity = cx.new(|cx: &mut Context<TrayService>| {
            let origin = cx.background_executor().now();
            let (backend, status) = match backend {
                Some(b) => (b, TrayStatus::Connecting),
                None => (
                    Box::new(Unsupported) as Box<dyn TrayBackend>,
                    TrayStatus::Failed(TrayError::Unsupported),
                ),
            };
            TrayService {
                state,
                backend,
                status,
                clicks: ClickTracker::new(double_click),
                origin,
                router: None,
                restore: None,
                hidden: false,
                _connect: None,
                _events: None,
            }
        });
        entity.update(cx, |tray, cx| {
            if tray.status != TrayStatus::Connecting {
                return;
            }
            let (tx, mut rx) = mpsc::unbounded();
            let connect = tray.backend.connect(tray.state.clone(), EventSink(tx), cx);
            tray._events = Some(cx.spawn(async move |this, cx| {
                while let Some(raw) = rx.next().await {
                    if this.update(cx, |tray, cx| tray.handle(raw, cx)).is_err() {
                        break;
                    }
                }
            }));
            tray._connect = Some(cx.spawn(async move |this, cx| {
                let result = connect.await;
                let _ = this.update(cx, |tray, cx| {
                    if tray.status != TrayStatus::Connecting {
                        return;
                    }
                    match result {
                        Ok(()) => {
                            tray.status = TrayStatus::Ready;
                            // Changes made while connecting.
                            tray.backend.update(tray.state.clone(), cx);
                            cx.emit(TrayEvent::Ready);
                        }
                        Err(e) => {
                            tray.status = TrayStatus::Failed(e.clone());
                            cx.emit(TrayEvent::Error(e));
                        }
                    }
                    cx.notify();
                });
            }));
        });
        Self { entity }
    }

    pub fn entity(&self) -> &Entity<TrayService> {
        &self.entity
    }
    pub fn status(&self, cx: &App) -> TrayStatus {
        self.entity.read(cx).status.clone()
    }

    fn edit(&self, cx: &mut App, f: impl FnOnce(&mut TrayState)) {
        self.entity.update(cx, |tray, cx| {
            let before = tray.state.clone();
            f(&mut tray.state);
            if tray.state != before {
                tray.push(cx);
            }
        });
    }

    pub fn set_icon(&self, icon: Option<TrayIcon>, cx: &mut App) {
        self.edit(cx, |s| s.icon = icon);
    }
    /// A dot or count over the icon (composited into RGBA icons; overlay for named icons).
    pub fn set_badge(&self, badge: Badge, cx: &mut App) {
        self.edit(cx, |s| s.badge = badge);
    }
    pub fn set_tooltip(&self, title: impl Into<String>, body: impl Into<String>, cx: &mut App) {
        let (title, body) = (title.into(), body.into());
        self.edit(cx, |s| {
            s.tooltip_title = title;
            s.tooltip_body = body;
        });
    }
    pub fn set_title(&self, title: impl Into<String>, cx: &mut App) {
        let title = title.into();
        self.edit(cx, |s| s.title = title);
    }
    /// Ask for attention (SNI `NeedsAttention`).
    pub fn set_attention(&self, attention: bool, cx: &mut App) {
        self.edit(cx, |s| s.attention = attention);
    }
    pub fn set_menu(&self, menu: Vec<TrayMenuItem>, cx: &mut App) {
        self.edit(cx, |s| s.menu = menu);
    }
    /// Build the menu from `entries` against the commands registry (its current contexts).
    /// Call again after enabling/disabling commands.
    pub fn set_menu_from_commands(&self, entries: &[MenuEntry], commands: &Commands, cx: &mut App) {
        let state = commands.entity().read(cx);
        let menu = build_menu(entries, state.catalog(), state.contexts());
        self.set_menu(menu, cx);
    }
    /// Execute command menu items through `commands` in `window` (the registry needs a window
    /// for handlers). Failures are reported as [`TrayEvent::Error`].
    pub fn route_commands(&self, commands: &Commands, window: AnyWindowHandle, cx: &mut App) {
        let commands = commands.clone();
        self.entity.update(cx, |tray, _| {
            tray.router = Some(Router { commands, window });
        });
    }
    pub fn subscribe(
        &self,
        cx: &mut App,
        f: impl Fn(&TrayEvent, &mut App) + 'static,
    ) -> Subscription {
        cx.subscribe(&self.entity, move |_, e, cx| f(e, cx))
    }
    /// Remove the icon from the panel.
    pub fn remove(&self, cx: &mut App) {
        self.entity.update(cx, |tray, cx| {
            if matches!(tray.status, TrayStatus::Ready | TrayStatus::Connecting) {
                tray.backend.shutdown(cx);
            }
            tray.status = TrayStatus::Removed;
            tray._events = None;
            tray._connect = None;
            cx.notify();
        });
    }
}

struct Unsupported;
impl TrayBackend for Unsupported {
    fn name(&self) -> &'static str {
        "unsupported"
    }
    fn connect(&mut self, _: TrayState, _: EventSink, cx: &mut App) -> Task<Result<(), TrayError>> {
        cx.background_executor()
            .spawn(async { Err(TrayError::Unsupported) })
    }
    fn update(&mut self, _: TrayState, _: &mut App) {}
    fn shutdown(&mut self, _: &mut App) {}
}
