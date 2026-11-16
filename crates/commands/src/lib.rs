//! One command catalog, independent pure resolution, native keyboard routing and menus.
pub mod context;
mod error;
pub mod keybinding;
pub mod model;
pub use context::{Context as WhenContext, ContextStack, When};
pub use error::Error;
pub use keybinding::Keybinding;
pub use model::{Command, CommandId, Conflict};
