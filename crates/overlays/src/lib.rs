//! Window-local overlays built on GPUI Controls 0.9.1.
//!
//! Call [`init`] before opening windows. Kit `open_window` mounts a Root automatically;
//! with guiraffe-windows, return [`root`] from the window builder. Use [`OverlayHost`]
//! for owned layers. Managed modal storage is independent of native dialogs,
//! with Base focus/actions, Controls surface parts and shared motion transitions.
mod confirm;
mod host;
pub mod position;
pub mod search;
pub mod stack;
pub mod toast;
pub use confirm::ConfirmResult;
pub use gpui_kit::component::menu::{ContextMenu, ContextMenuExt, DropdownMenu, PopupMenu};
pub use gpui_kit::component::{hover_card::HoverCard, popover::Popover, tooltip::Tooltip};
pub use host::{ContextBuilder, Drawer, OverlayError, OverlayHost, init, root};
mod widgets;
pub use widgets::{Affix, LoadingOverlay};
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod spotlight;
pub use spotlight::{Spotlight, SpotlightAction, SpotlightEvent, SpotlightState};
#[cfg(test)]
mod gpui_tests;

/// Mantine-style name for the window-local modal manager.
pub type Modals = OverlayHost;
pub mod modals;
pub mod notifications;
pub use host::OverlayError as Error;
mod floating;
pub use floating::FloatingTooltip;

mod modal;
pub use modal::Modal;
