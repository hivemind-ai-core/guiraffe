//! Window-local overlays built on GPUI Controls 0.9.1.
//!
//! Call [`init`] before opening windows. Kit `open_window` mounts a Root automatically;
//! with guiraffe-windows, return [`root`] from the window builder. Use [`OverlayHost`]
//! for owned layers. Managed modal storage is independent of native dialogs,
//! with Base focus/actions, Controls surface parts and shared motion transitions.
//!
//! A Rust `ui!` example. Create shared entities before rendering when the host owns their state.
//!
//! ```ignore
//! ui! { Affix { position: ToastPosition::TopRight; offset: Size::Sm LoadingOverlay { visible: false } } }
//! ```
mod confirm;
mod data_ui;
mod document_services;
mod host;
mod kit_surfaces;
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
mod spotlight;
pub use spotlight::Spotlight;
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

use guiraffe_core::widget_spec;
mod widget_specs;
