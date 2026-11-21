//! The seam between [`Tray`](crate::Tray) and the platform tray.
use std::sync::{Arc, Mutex, MutexGuard};

use futures::channel::mpsc::UnboundedSender;
use gpui_kit::{App, Task};

use crate::{
    TrayError,
    icon::{Badge, TrayIcon},
    menu::{TrayItemId, TrayMenuItem},
};

/// Everything a backend shows. Backends receive a full snapshot on every change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    /// Stable application id (StatusNotifierItem `Id`).
    pub id: String,
    pub title: String,
    pub tooltip_title: String,
    pub tooltip_body: String,
    pub icon: Option<TrayIcon>,
    pub badge: Badge,
    /// Badge color, RGBA.
    pub badge_color: [u8; 4],
    /// Ask for attention (SNI `NeedsAttention`; ignored elsewhere).
    pub attention: bool,
    pub menu: Vec<TrayMenuItem>,
}

/// Raw input from the platform tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// Primary activation (left click).
    Activate,
    /// The platform detected a double click itself (Windows).
    DoubleActivate,
    /// Secondary activation (middle click on SNI).
    SecondaryActivate,
    Scroll {
        delta: i32,
        horizontal: bool,
    },
    MenuItem(TrayItemId),
    /// The tray host appeared (true) or went away (false).
    HostAvailable(bool),
}

/// Where backends send [`BackendEvent`]s; `Send`, usable from any thread.
#[derive(Clone)]
pub struct EventSink(pub(crate) UnboundedSender<BackendEvent>);

impl EventSink {
    /// Returns false once the tray is gone.
    pub fn send(&self, event: BackendEvent) -> bool {
        self.0.unbounded_send(event).is_ok()
    }
}

/// A platform tray implementation.
pub trait TrayBackend: 'static {
    fn name(&self) -> &'static str;
    /// Create the platform tray icon. May finish asynchronously; the returned task resolves when
    /// the icon is registered (or failed).
    fn connect(
        &mut self,
        state: TrayState,
        events: EventSink,
        cx: &mut App,
    ) -> Task<Result<(), TrayError>>;
    /// Show a new snapshot. Called only after `connect` succeeded.
    fn update(&mut self, state: TrayState, cx: &mut App);
    /// Remove the icon. Called once; the backend is dropped afterwards.
    fn shutdown(&mut self, cx: &mut App);
}

#[derive(Default)]
struct MockState {
    states: Vec<TrayState>,
    sink: Option<EventSink>,
    shut_down: bool,
    fail: Option<TrayError>,
}

/// An in-memory backend for tests: records snapshots and lets the test play the tray host.
#[derive(Clone, Default)]
pub struct MockTray(Arc<Mutex<MockState>>);

impl MockTray {
    pub fn new() -> Self {
        Self::default()
    }
    /// A backend whose `connect` fails with `error`.
    pub fn failing(error: TrayError) -> Self {
        let mock = Self::default();
        mock.lock().fail = Some(error);
        mock
    }
    fn lock(&self) -> MutexGuard<'_, MockState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Every snapshot shown so far (connect included).
    pub fn states(&self) -> Vec<TrayState> {
        self.lock().states.clone()
    }
    pub fn last(&self) -> Option<TrayState> {
        self.lock().states.last().cloned()
    }
    pub fn is_shut_down(&self) -> bool {
        self.lock().shut_down
    }
    /// Play the host: send a raw event. False before connect or after shutdown.
    pub fn send(&self, event: BackendEvent) -> bool {
        let sink = self.lock().sink.clone();
        sink.is_some_and(|s| s.send(event))
    }
}

impl TrayBackend for MockTray {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn connect(
        &mut self,
        state: TrayState,
        events: EventSink,
        cx: &mut App,
    ) -> Task<Result<(), TrayError>> {
        let mut s = self.lock();
        let result = match s.fail.take() {
            Some(e) => Err(e),
            None => {
                s.states.push(state);
                s.sink = Some(events);
                Ok(())
            }
        };
        cx.background_executor().spawn(async move { result })
    }
    fn update(&mut self, state: TrayState, _: &mut App) {
        self.lock().states.push(state);
    }
    fn shutdown(&mut self, _: &mut App) {
        let mut s = self.lock();
        s.shut_down = true;
        s.sink = None;
    }
}
