//! The OS backend, through `notify-rust`.
//!
//! - Linux/BSD: freedesktop notifications over D-Bus (zbus, async). Actions, body clicks, close
//!   reasons, replace-by-id and programmatic close are all supported. Tested on Linux.
//! - macOS (`NSUserNotificationCenter`) and Windows (toast): delivery only. **Untested.**
//! - Elsewhere: every call returns [`DesktopError::Unavailable`].
use super::{
    DesktopNotification,
    backend::{BoxFuture, Capabilities, Delivery, DesktopError, NotificationBackend, Shown},
};

/// The platform's notification service.
#[derive(Default)]
pub struct PlatformBackend {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    live: std::sync::Arc<xdg::Live>,
}

impl PlatformBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod xdg {
    use std::{
        collections::HashMap,
        panic::AssertUnwindSafe,
        sync::{Arc, Mutex},
    };

    use futures::FutureExt as _;
    use notify_rust::{NotificationHandle, NotificationResponse};

    use super::super::{
        CloseReason, DesktopNotification, NotificationIcon, Urgency,
        backend::{DesktopError, Response},
        escape_markup,
    };

    /// Handles of shown notifications, so they can be closed. Each handle keeps the D-Bus
    /// connection that sent it alive: some servers deliver signals only to the sender.
    #[derive(Default)]
    pub struct Live(Mutex<HashMap<u32, Arc<NotificationHandle>>>);

    impl Live {
        fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Arc<NotificationHandle>>> {
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }
        pub fn insert(&self, id: u32, handle: Arc<NotificationHandle>) {
            self.lock().insert(id, handle);
        }
        pub fn take(&self, id: u32) -> Option<Arc<NotificationHandle>> {
            self.lock().remove(&id)
        }
    }

    pub fn build(note: &DesktopNotification, replaces: Option<u32>) -> notify_rust::Notification {
        let mut n = notify_rust::Notification::new();
        n.appname(&note.app_name)
            .summary(&note.title)
            .body(&escape_markup(&note.body))
            .timeout(note.timeout.as_millis_i32());
        match &note.icon {
            NotificationIcon::None => {}
            NotificationIcon::Named(name) => {
                n.icon(name);
            }
            NotificationIcon::Path(path) => {
                n.icon(&path.to_string_lossy());
            }
        }
        n.urgency(match note.urgency {
            Urgency::Low => notify_rust::Urgency::Low,
            Urgency::Normal => notify_rust::Urgency::Normal,
            Urgency::Critical => notify_rust::Urgency::Critical,
        });
        // The body click is the freedesktop "default" action; servers need it declared.
        n.action("default", "");
        for action in &note.actions {
            n.action(&action.key, &action.label);
        }
        if let Some(category) = &note.category {
            n.hint(notify_rust::Hint::Category(category.clone()));
        }
        if note.transient {
            n.hint(notify_rust::Hint::Transient(true));
        }
        if let Some(id) = replaces {
            n.id(id);
        }
        n
    }

    pub fn response(response: &NotificationResponse) -> Option<Response> {
        Some(match response {
            NotificationResponse::Default => Response::Activated,
            NotificationResponse::Action(key) if key == "default" => Response::Activated,
            NotificationResponse::Action(key) => Response::Action(key.clone()),
            NotificationResponse::Reply(_) => return None,
            NotificationResponse::Closed(reason) => Response::Closed(match reason {
                notify_rust::CloseReason::Expired => CloseReason::Expired,
                notify_rust::CloseReason::Dismissed => CloseReason::Dismissed,
                notify_rust::CloseReason::CloseAction => CloseReason::App,
                notify_rust::CloseReason::Other(code) => CloseReason::Other(*code),
            }),
        })
    }

    pub fn error(e: notify_rust::error::Error) -> DesktopError {
        let text = e.to_string();
        // A missing session bus or notification daemon is "unavailable", not a failed request.
        if text.contains("ServiceUnknown")
            || text.contains("org.freedesktop.Notifications")
            || text.contains("DBUS_SESSION_BUS_ADDRESS")
            || text.contains("session bus")
        {
            DesktopError::Unavailable(text)
        } else {
            DesktopError::Delivery(text)
        }
    }

    pub async fn show(
        live: Arc<Live>,
        note: DesktopNotification,
        replaces: Option<u32>,
    ) -> Result<super::Shown, DesktopError> {
        let request = build(&note, replaces);
        let handle = AssertUnwindSafe(request.show_async())
            .catch_unwind()
            .await
            .map_err(|_| DesktopError::Panicked)?
            .map_err(error)?;
        let platform_id = handle.id();
        let handle = Arc::new(handle);
        live.insert(platform_id, handle.clone());
        let outcome = async move {
            let mut out = None;
            // notify-rust unwraps D-Bus errors inside its signal loop: keep panics in here.
            let listened = AssertUnwindSafe(handle.wait_for_action_async(|r| out = response(r)))
                .catch_unwind()
                .await;
            // Only forget the handle if it is still ours (an update may have replaced it).
            let mut map = live.lock();
            if map
                .get(&platform_id)
                .is_some_and(|h| Arc::ptr_eq(h, &handle))
            {
                map.remove(&platform_id);
            }
            listened.ok().and(out)
        };
        Ok(super::Shown {
            platform_id,
            outcome: Box::pin(outcome),
        })
    }

    pub async fn close(live: Arc<Live>, platform_id: u32) -> Result<(), DesktopError> {
        let handle = live
            .take(platform_id)
            .ok_or(DesktopError::Unknown(platform_id.into()))?;
        AssertUnwindSafe(handle.close_async())
            .catch_unwind()
            .await
            .map_err(|_| DesktopError::Panicked)
    }

    pub fn show_blocking(note: &DesktopNotification) -> Result<(), DesktopError> {
        let mut request = build(note, None);
        // Fire-and-forget: nobody listens, so offer no buttons.
        request.actions.clear();
        std::panic::catch_unwind(AssertUnwindSafe(|| request.show()))
            .map_err(|_| DesktopError::Panicked)?
            .map(|_| ())
            .map_err(error)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod basic {
    //! macOS / Windows: delivery only. UNTESTED (no hardware in CI); see KNOWN_ISSUES.md.
    use std::panic::AssertUnwindSafe;

    use super::super::{DesktopNotification, NotificationIcon, backend::DesktopError};

    pub fn show_blocking(note: &DesktopNotification) -> Result<(), DesktopError> {
        let mut n = notify_rust::Notification::new();
        n.appname(&note.app_name)
            .summary(&note.title)
            .body(&note.body)
            .timeout(note.timeout.as_millis_i32());
        if let NotificationIcon::Path(path) = &note.icon {
            n.image_path(&path.to_string_lossy());
        }
        #[cfg(target_os = "windows")]
        n.urgency(match note.urgency {
            super::super::Urgency::Low => notify_rust::Urgency::Low,
            super::super::Urgency::Normal => notify_rust::Urgency::Normal,
            super::super::Urgency::Critical => notify_rust::Urgency::Critical,
        });
        std::panic::catch_unwind(AssertUnwindSafe(|| n.show()))
            .map_err(|_| DesktopError::Panicked)?
            .map(|_| ())
            .map_err(|e| DesktopError::Delivery(e.to_string()))
    }
}

pub(super) fn show_blocking(note: &DesktopNotification) -> Result<(), DesktopError> {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    return xdg::show_blocking(note);
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return basic::show_blocking(note);
    #[allow(unreachable_code)]
    {
        let _ = note;
        Err(DesktopError::Unavailable(
            "no system notification service on this platform; use OverlayHost::notify".into(),
        ))
    }
}

impl NotificationBackend for PlatformBackend {
    fn name(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(all(
            unix,
            not(any(target_os = "ios", target_os = "android"))
        )) {
            "freedesktop"
        } else {
            "unsupported"
        }
    }

    fn capabilities(&self) -> Capabilities {
        if cfg!(all(
            unix,
            not(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "android"
            ))
        )) {
            Capabilities::ALL
        } else {
            Capabilities::NONE
        }
    }

    fn show(&self, delivery: Delivery) -> BoxFuture<Result<Shown, DesktopError>> {
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ))]
        {
            Box::pin(xdg::show(
                self.live.clone(),
                delivery.notification,
                delivery.replaces,
            ))
        }
        #[cfg(not(all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        )))]
        {
            Box::pin(async move {
                show_blocking(&delivery.notification)?;
                Ok(Shown {
                    platform_id: 0,
                    outcome: Box::pin(async { None }),
                })
            })
        }
    }

    fn close(&self, platform_id: u32) -> BoxFuture<Result<(), DesktopError>> {
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ))]
        {
            Box::pin(xdg::close(self.live.clone(), platform_id))
        }
        #[cfg(not(all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        )))]
        {
            let _ = platform_id;
            let name = self.name();
            Box::pin(async move { Err(DesktopError::Unsupported(name)) })
        }
    }
}
