//! GUIraffe system tray: a tray icon with a menu built from the commands registry, tooltip,
//! click/double-click events, a dynamic icon and badge, and a minimize-to-tray helper.
//!
//! ```
//! use guiraffe_tray::{Badge, RgbaIcon, TrayIcon, TrayOptions};
//! let options = TrayOptions {
//!     id: "my-app".into(),
//!     tooltip_title: "My app".into(),
//!     icon: Some(TrayIcon::Rgba(RgbaIcon::disc(32, [34, 139, 230, 255]))),
//!     ..Default::default()
//! };
//! // In an app: let tray = guiraffe_tray::Tray::new(options, cx);
//! //            tray.set_badge(Badge::Count(3), cx);
//! # let _ = (options, Badge::Dot);
//! ```
//!
//! **Backends** (feature `native`; without it [`Tray::new`] reports [`TrayError::Unsupported`]
//! and only [`MockTray`] / custom backends work):
//! - Linux/BSD: StatusNotifierItem via `ksni` — pure Rust over D-Bus, no GTK, independent of
//!   the display server, so it works on Wayland. Needs an SNI host: KDE Plasma, waybar/ironbar,
//!   GNOME with the AppIndicator extension, ... (`tray-icon` was not used on Linux: it needs
//!   GTK 3 and libappindicator and its own GTK main loop.)
//! - macOS / Windows: `tray-icon` (+ `muda` menus). **Untested.**
//!
//! Pure, headlessly tested parts: [`build_menu`] (commands → menu), [`ClickTracker`]
//! (double-click detection) and [`RgbaIcon`] badge compositing.

mod backend;
mod click;
mod icon;
mod menu;
mod minimize;
mod service;

#[cfg(all(
    feature = "native",
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod sni;

#[cfg(all(feature = "native", any(target_os = "macos", target_os = "windows")))]
mod native;

pub use backend::{BackendEvent, EventSink, MockTray, TrayBackend, TrayState};
pub use click::{ClickKind, ClickTracker};
pub use icon::{Badge, RgbaIcon, TrayIcon};
pub use menu::{MenuEntry, TrayItemId, TrayMenuItem, build_menu, find_item, menu_by_category};
pub use minimize::{HideMode, hide, minimize_to_tray, restore};
pub use service::{Tray, TrayEvent, TrayOptions, TrayService, TrayStatus};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrayError {
    /// No tray on this platform/build (feature `native` off, or an unsupported OS).
    #[error("system tray is not supported on this platform or build")]
    Unsupported,
    /// The platform has a tray protocol but nothing shows it (no StatusNotifierItem host).
    #[error("system tray unavailable: {0}")]
    Unavailable(String),
    #[error("system tray error: {0}")]
    Platform(String),
    #[error("invalid tray icon: {0}")]
    InvalidIcon(String),
    /// A routed menu command failed.
    #[error("tray command failed: {0}")]
    Command(String),
}

/// The platform backend for this build, if any.
pub fn platform_backend() -> Option<Box<dyn TrayBackend>> {
    #[cfg(all(
        feature = "native",
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    return Some(Box::new(sni::SniBackend::new()));
    #[cfg(all(feature = "native", any(target_os = "macos", target_os = "windows")))]
    return Some(Box::new(native::NativeBackend::new()));
    #[allow(unreachable_code)]
    None
}

#[cfg(test)]
mod gpui_tests;
