//! System (OS) notifications, behind the `desktop` feature.
//!
//! Two layers:
//!
//! - [`DesktopNotification`]: the notification data (title, body, icon, urgency, timeout,
//!   action buttons). [`DesktopNotification::show`] / [`DesktopNotification::show_async`] are
//!   fire-and-forget delivery that only report delivery errors.
//! - [`SystemNotifications`]: the notification service. It assigns a [`NotificationId`], keeps
//!   the notification addressable for [`update`](SystemNotifications::update) (replace in place)
//!   and [`close`](SystemNotifications::close), and reports what happens to it as typed
//!   [`DesktopEvent`]s: shown, activated (body click), action button pressed, closed (with a
//!   [`CloseReason`]) or failed.
//!
//! The service talks to the OS through a [`NotificationBackend`]. [`init`] installs the platform
//! backend ([`PlatformBackend`]): freedesktop notifications over D-Bus on Linux/BSD,
//! `NSUserNotificationCenter` on macOS and toast notifications on Windows, all through
//! `notify-rust`. Tests install [`mock::MockBackend`] with [`init_with_backend`] so no
//! notification daemon is needed. Nothing here touches the network.
//!
//! Platform support (see `KNOWN_ISSUES.md`): actions, close reasons and replace-by-id work on
//! freedesktop servers that advertise them (`actions` capability). On macOS and Windows the
//! notification is delivered, but no response is reported back ([`Capabilities::NONE`] apart from
//! delivery); those backends are untested.
//!
//! ```
//! use guiraffe_overlays::desktop::{DesktopNotification, Urgency};
//! let note = DesktopNotification::new("Build finished", "3 warnings")
//!     .urgency(Urgency::Low)
//!     .icon_name("dialog-information")
//!     .action("open", "Open log");
//! assert_eq!(note.actions[0].key, "open");
//! ```
//!
//! Linux notification bodies are markup: bodies are escaped (see [`escape_markup`]) so untrusted
//! text is shown verbatim.

mod backend;
pub mod mock;
mod platform;
mod service;

use std::{path::PathBuf, time::Duration};

pub use backend::{
    BoxFuture, Capabilities, Delivery, DesktopError, NotificationBackend, Response, Shown,
};
pub use platform::PlatformBackend;
pub use service::{
    DesktopEvent, DesktopEventKind, NotificationCenter, SystemNotifications, init,
    init_with_backend,
};

/// How urgent a notification is. Freedesktop servers style and sort by it; Windows maps
/// [`Urgency::Critical`] to a high-priority toast. macOS ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

/// When a notification expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Timeout {
    /// The server's default (typically 5–10 s; critical notifications often never expire).
    #[default]
    Default,
    /// Stays until the user dismisses it.
    Never,
    /// Expires after this long. Clamped to `u32::MAX` milliseconds.
    After(Duration),
}

impl Timeout {
    /// Milliseconds in freedesktop terms: `-1` server default, `0` never.
    pub fn as_millis_i32(self) -> i32 {
        match self {
            Self::Default => -1,
            Self::Never => 0,
            Self::After(d) => i32::try_from(d.as_millis()).unwrap_or(i32::MAX).max(1),
        }
    }
}

/// The notification's icon.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum NotificationIcon {
    /// The application's default icon (or none).
    #[default]
    None,
    /// A freedesktop icon-theme name, e.g. `"dialog-information"` (Linux/BSD only).
    Named(String),
    /// An image file. Used as the icon on Linux and as the content image on macOS/Windows.
    Path(PathBuf),
}

/// A button on a notification. `key` comes back in [`DesktopEventKind::Action`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NotificationAction {
    /// Stable identifier reported back when pressed. `"default"` is reserved for the body click.
    pub key: String,
    /// Button label shown to the user.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotification {
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon: NotificationIcon,
    pub urgency: Urgency,
    pub timeout: Timeout,
    pub actions: Vec<NotificationAction>,
    /// Freedesktop category hint, e.g. `"transfer.complete"`.
    pub category: Option<String>,
    /// Transient notifications skip the server's history (freedesktop `transient` hint).
    pub transient: bool,
}

impl DesktopNotification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            app_name: "GUIraffe".into(),
            title: title.into(),
            body: body.into(),
            icon: NotificationIcon::None,
            urgency: Urgency::Normal,
            timeout: Timeout::Default,
            actions: Vec::new(),
            category: None,
            transient: false,
        }
    }
    pub fn app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }
    pub fn icon(mut self, icon: NotificationIcon) -> Self {
        self.icon = icon;
        self
    }
    pub fn icon_name(self, name: impl Into<String>) -> Self {
        self.icon(NotificationIcon::Named(name.into()))
    }
    pub fn icon_path(self, path: impl Into<PathBuf>) -> Self {
        self.icon(NotificationIcon::Path(path.into()))
    }
    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }
    pub fn timeout(mut self, timeout: Timeout) -> Self {
        self.timeout = timeout;
        self
    }
    /// Add a button. Keys must be unique and not `"default"`; a duplicate or reserved key is
    /// ignored (with a debug assertion).
    pub fn action(mut self, key: impl Into<String>, label: impl Into<String>) -> Self {
        let key = key.into();
        let valid =
            !key.is_empty() && key != "default" && !self.actions.iter().any(|a| a.key == key);
        debug_assert!(
            valid,
            "invalid or duplicate notification action key {key:?}"
        );
        if valid {
            self.actions.push(NotificationAction {
                key,
                label: label.into(),
            });
        }
        self
    }
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
    pub fn transient(mut self, transient: bool) -> Self {
        self.transient = transient;
        self
    }

    /// Fire-and-forget delivery off the foreground thread; await the task for delivery errors.
    /// Use [`SystemNotifications`] to receive clicks, actions and close events.
    pub fn show_async(&self, cx: &gpui_kit::App) -> gpui_kit::Task<Result<(), DesktopError>> {
        let note = self.clone();
        deliver_async(cx, move || note.show())
    }

    /// Blocking fire-and-forget delivery; call it from a background thread.
    pub fn show(&self) -> Result<(), DesktopError> {
        platform::show_blocking(self)
    }
}

/// Typed notification identity, stable across [`SystemNotifications::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NotificationId(pub u64);

/// Why a notification closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloseReason {
    /// It timed out.
    Expired,
    /// The user dismissed it.
    Dismissed,
    /// The application closed it ([`SystemNotifications::close`]).
    App,
    /// Some other server-specific reason code.
    Other(u32),
}

/// Escape freedesktop body markup (`&`, `<`, `>`), so untrusted text is shown verbatim.
pub fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn deliver_async(
    cx: &gpui_kit::App,
    delivery: impl FnOnce() -> Result<(), DesktopError> + Send + 'static,
) -> gpui_kit::Task<Result<(), DesktopError>> {
    cx.background_executor().spawn(async move { delivery() })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_body_is_plain_text() {
        assert_eq!(
            escape_markup("<b>A & B</b>"),
            "&lt;b&gt;A &amp; B&lt;/b&gt;"
        );
    }
    #[test]
    fn defaults() {
        let n = DesktopNotification::new("Hello", "Body");
        assert_eq!(n.urgency, Urgency::Normal);
        assert_eq!(n.timeout, Timeout::Default);
        assert!(n.actions.is_empty());
    }
    #[test]
    fn timeout_maps_to_freedesktop_millis() {
        assert_eq!(Timeout::Default.as_millis_i32(), -1);
        assert_eq!(Timeout::Never.as_millis_i32(), 0);
        assert_eq!(
            Timeout::After(Duration::from_millis(1500)).as_millis_i32(),
            1500
        );
        // Zero would mean "never" on the wire; a zero duration still expires.
        assert_eq!(Timeout::After(Duration::ZERO).as_millis_i32(), 1);
        assert_eq!(
            Timeout::After(Duration::from_secs(u64::MAX)).as_millis_i32(),
            i32::MAX
        );
    }
    #[test]
    fn builder_collects_actions_in_order() {
        let n = DesktopNotification::new("t", "b")
            .action("a", "A")
            .action("b", "B")
            .icon_name("mail")
            .category("email.arrived")
            .transient(true);
        let keys: Vec<_> = n.actions.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(keys, ["a", "b"]);
        assert_eq!(n.icon, NotificationIcon::Named("mail".into()));
        assert_eq!(n.category.as_deref(), Some("email.arrived"));
        assert!(n.transient);
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[gpui_kit::test]
    async fn review_11_delivery_is_deferred_to_background(cx: &mut gpui_kit::TestAppContext) {
        let calls = Arc::new(AtomicUsize::new(0));
        let delivery = calls.clone();
        let task = cx.update(|cx| {
            deliver_async(cx, move || {
                delivery.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "delivery must not execute inline on the foreground"
        );
        assert_eq!(task.await, Ok(()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
