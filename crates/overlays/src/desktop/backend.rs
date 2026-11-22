//! The seam between [`SystemNotifications`](super::SystemNotifications) and the OS.
use std::{future::Future, pin::Pin};

use super::{CloseReason, DesktopNotification};

/// A boxed, `Send` future: backends run on the background executor.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// What a backend can report back. Apps can hide action buttons when `actions` is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Action buttons are shown and reported back.
    pub actions: bool,
    /// Body clicks are reported back ([`Response::Activated`]).
    pub activation: bool,
    /// Close events (with reasons) are reported back.
    pub close_events: bool,
    /// Replace-in-place by id is supported (otherwise an update shows a new notification).
    pub replace: bool,
    /// Programmatic close is supported.
    pub close: bool,
}

impl Capabilities {
    /// Delivery only: nothing is reported back.
    pub const NONE: Self = Self {
        actions: false,
        activation: false,
        close_events: false,
        replace: false,
        close: false,
    };
    /// Everything (a freedesktop server with the `actions` capability).
    pub const ALL: Self = Self {
        actions: true,
        activation: true,
        close_events: true,
        replace: true,
        close: true,
    };
}

/// One delivery request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub notification: DesktopNotification,
    /// The platform id of the notification this one replaces (an update), if any.
    pub replaces: Option<u32>,
}

/// How a shown notification ended, as reported by the platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// The body was clicked (freedesktop `default` action).
    Activated,
    /// An action button was pressed.
    Action(String),
    /// It closed without an action.
    Closed(CloseReason),
}

/// A delivered notification.
pub struct Shown {
    /// The platform's id, used for replacing and closing.
    pub platform_id: u32,
    /// Resolves once with the notification's fate, or `None` when the platform reports nothing
    /// (unsupported, or the connection went away). Dropping it stops listening.
    pub outcome: BoxFuture<Option<Response>>,
}

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shown")
            .field("platform_id", &self.platform_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesktopError {
    /// No notification service on this platform/session (no D-Bus daemon, sandbox, ...).
    #[error("desktop notifications are unavailable: {0}")]
    Unavailable(String),
    /// The service refused or failed the request.
    #[error("desktop notification delivery failed: {0}")]
    Delivery(String),
    /// The operation is not supported by this backend.
    #[error("not supported by the {0} notification backend")]
    Unsupported(&'static str),
    /// [`init`](super::init) / [`init_with_backend`](super::init_with_backend) was not called.
    #[error("desktop notifications are not initialized; call guiraffe_overlays::desktop::init")]
    NotInitialized,
    /// The notification is not (or no longer) open.
    #[error("unknown notification {0}")]
    Unknown(u64),
    /// The backend panicked; the panic was caught at the crate boundary.
    #[error("the notification backend panicked")]
    Panicked,
}

/// An OS notification service. Implementations must not block in `show`/`close` beyond the
/// returned futures (which run on the background executor) and must never panic.
pub trait NotificationBackend: Send + Sync + 'static {
    /// Human-readable backend name for diagnostics.
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn show(&self, delivery: Delivery) -> BoxFuture<Result<Shown, DesktopError>>;
    fn close(&self, platform_id: u32) -> BoxFuture<Result<(), DesktopError>>;
}
