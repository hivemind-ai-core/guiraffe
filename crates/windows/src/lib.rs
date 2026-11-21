//! GUIraffe windows: rock-solid multi-window support for GPUI.
//!
//! - [`WindowManager`]: keyed registry with singleton reuse, cascading, close guards,
//!   a typed cross-window event bus, debounced atomic layout persistence and a quit policy.
//! - [`placement`]: pure restore-onto-current-displays logic.
//! - [`layout_store`]: versioned layout file that sets corrupt files aside.
//! - [`notify`]: in-app desktop notifications in borderless always-on-top windows.

pub mod layout_store;
mod manager;
pub mod notify;
pub mod placement;

pub use manager::{
    CloseDecision, ListenerId, OpenMode, QuitPolicy, WindowConfig, WindowKey, WindowManager,
};

#[cfg(test)]
mod tests;
