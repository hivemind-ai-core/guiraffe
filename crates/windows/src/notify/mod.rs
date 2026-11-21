//! In-app desktop notifications: themed cards in a borderless, always-on-top surface stacked in
//! a screen corner, outside the app's windows.
//!
//! - [`NotificationWindows`]: the service. `show` / `update` (same id, in place) / `dismiss`,
//!   typed [`WindowNotificationEvent`]s (shown, clicked, action, dismissed with a reason).
//! - [`WindowNotification`]: the card content (title, two-line body, kind/icon, actions,
//!   progress, timeout). [`NotificationCard`] renders it and can be used in-app too.
//! - Pure parts, tested headlessly: [`stack`] geometry, [`timer`] auto-dismiss countdown (pauses
//!   while hovered) with an injectable [`Clock`], and the [`StackModel`] (limit + FIFO queue).
//!
//! **Placement.** All cards share one surface, sized to fit them (cards have fixed heights), so
//! stacking never needs to move windows:
//! - Wayland: a `wlr-layer-shell` overlay surface anchored to the corner (the compositor keeps it
//!   on top and in place; input passes through the gaps between cards). Compositors without
//!   layer-shell (GNOME/Mutter) get a regular pop-up window whose position and stacking the
//!   compositor decides ([`PlacementUsed::CompositorPlaced`]).
//! - X11, macOS, Windows: an always-on-top pop-up window at the corner of the primary display's
//!   work area. A bottom-corner stack that changes height is re-opened at its new origin (GPUI
//!   cannot move windows). macOS and Windows are untested.
//!
//! Entry/exit animations (fade + slide from the screen edge) follow the motion tokens and are
//! instant with reduced motion. The surface never takes keyboard focus.
mod card;
pub mod model;
mod service;
pub mod stack;
pub mod timer;

pub use card::{NotificationCard, metrics as card_metrics};
pub use model::{
    CardAction, CardIcon, NotificationKind, StackModel, WindowNotification, WindowNotificationId,
};
pub use service::{
    DismissReason, NotificationStack, NotificationWindows, NotificationWindowsConfig, Placement,
    PlacementUsed, StackView, WindowNotificationEvent, WindowNotificationEventKind, init,
    init_with_clock,
};
pub use stack::{Corner, StackConfig};
pub use timer::{Clock, DismissTimer, ManualClock, SystemClock};

#[cfg(test)]
mod gpui_tests;
