//! The seam between [`Tray`](crate::Tray) and the platform tray.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard},
};

use futures::{StreamExt as _, channel::mpsc};
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
pub struct EventSink {
    inbox: Arc<Mutex<Inbox>>,
}

enum PendingEvent {
    Scroll { delta: i64, horizontal: bool },
    Ordered(BackendEvent),
}

struct Inbox {
    queue: VecDeque<PendingEvent>,
    wake: mpsc::Sender<()>,
    closed: bool,
}

pub(crate) struct EventReceiver {
    inbox: Arc<Mutex<Inbox>>,
    wake: mpsc::Receiver<()>,
}

impl Inbox {
    fn close(&mut self) {
        self.closed = true;
        self.queue = VecDeque::new();
        self.wake.close_channel();
    }
}

impl EventSink {
    pub(crate) fn channel() -> (Self, EventReceiver) {
        let (wake, receiver) = mpsc::channel(0);
        let inbox = Arc::new(Mutex::new(Inbox {
            queue: VecDeque::new(),
            wake,
            closed: false,
        }));
        (
            Self {
                inbox: inbox.clone(),
            },
            EventReceiver {
                inbox,
                wake: receiver,
            },
        )
    }

    pub(crate) fn close(&self) {
        self.inbox
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .close();
    }

    /// Returns false once the tray is gone. Adjacent scroll bursts coalesce by axis.
    pub fn send(&self, event: BackendEvent) -> bool {
        let mut inbox = self.inbox.lock().unwrap_or_else(|error| error.into_inner());
        if inbox.closed {
            return false;
        }
        let needs_wake = inbox.queue.is_empty();
        let pending = match event {
            BackendEvent::Scroll { delta, horizontal } => {
                for pending in inbox.queue.iter_mut().rev() {
                    match pending {
                        PendingEvent::Scroll {
                            delta: total,
                            horizontal: axis,
                        } if *axis == horizontal => {
                            *total = total.saturating_add(i64::from(delta));
                            return true;
                        }
                        PendingEvent::Scroll { .. } => {}
                        PendingEvent::Ordered(_) => break,
                    }
                }
                PendingEvent::Scroll {
                    delta: i64::from(delta),
                    horizontal,
                }
            }
            ordered => PendingEvent::Ordered(ordered),
        };
        inbox.queue.push_back(pending);
        if needs_wake {
            let _ = inbox.wake.try_send(());
        }
        true
    }
}

impl EventReceiver {
    pub(crate) fn try_recv(&self) -> Option<BackendEvent> {
        let mut inbox = self.inbox.lock().unwrap_or_else(|error| error.into_inner());
        let event = inbox.queue.pop_front()?;
        Some(match event {
            PendingEvent::Ordered(event) => event,
            PendingEvent::Scroll { delta, horizontal } => BackendEvent::Scroll {
                delta: delta.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                horizontal,
            },
        })
    }

    pub(crate) async fn recv(&mut self) -> Option<BackendEvent> {
        loop {
            if let Some(event) = self.try_recv() {
                return Some(event);
            }
            self.wake.next().await?;
        }
    }
}

impl Drop for EventReceiver {
    fn drop(&mut self) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|error| error.into_inner());
        inbox.close();
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

#[cfg(test)]
mod inbox_tests {
    use super::*;

    #[test]
    fn scroll_churn_coalesces_and_activation_order_survives_until_teardown() {
        let (sink, receiver) = EventSink::channel();
        let scroll = |horizontal, delta| BackendEvent::Scroll { horizontal, delta };
        for _ in 0..100_000 {
            assert!(sink.send(scroll(false, 2)));
            assert!(sink.send(scroll(true, -1)));
        }
        assert_eq!(sink.inbox.lock().expect("tray inbox lock").queue.len(), 2);
        assert!(sink.send(BackendEvent::Activate));
        assert!(sink.send(scroll(false, 5)));
        assert!(sink.send(BackendEvent::SecondaryActivate));
        assert_eq!(receiver.try_recv(), Some(scroll(false, 200_000)));
        assert_eq!(receiver.try_recv(), Some(scroll(true, -100_000)));
        assert_eq!(receiver.try_recv(), Some(BackendEvent::Activate));
        assert_eq!(receiver.try_recv(), Some(scroll(false, 5)));
        assert_eq!(receiver.try_recv(), Some(BackendEvent::SecondaryActivate));
        assert!(receiver.try_recv().is_none());

        for _ in 0..1_000 {
            assert!(sink.send(BackendEvent::MenuItem(TrayItemId::Custom("queued".into()))));
        }
        drop(receiver);
        assert_eq!(
            sink.inbox.lock().expect("tray inbox lock").queue.capacity(),
            0
        );
        assert!(!sink.send(BackendEvent::Activate));
    }
}
