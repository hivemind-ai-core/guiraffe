//! One command catalog, independent pure resolution, native keyboard routing and menus.
//!
//! Pure metadata and resolution work without creating a GPUI application:
//! ```
//! use guiraffe_commands::{Command, CommandId, ContextStack, Keybinding, Registry};
//!
//! let mut catalog = Registry::default();
//! let id = CommandId::parse("file.save")?;
//! let binding = Keybinding::parse("Control+S")?;
//! catalog.register(Command::new(id.clone(), "Save", ())
//!     .category("File").keybinding(binding.clone()))?;
//! assert_eq!(catalog.resolve_binding(&binding, &ContextStack::default())
//!     .map(|command| &command.id), Some(&id));
//! # Ok::<(), guiraffe_commands::Error>(())
//! ```
//!
//! For native applications, create a [`Commands`] service per window, register
//! handlers, and place [`CommandScope`] around focused content. [`Menu`] consumes
//! the same registry; Ctrl+Shift+P opens the overlays Spotlight command bar, with
//! categories as groups and shortcuts as keycap chips.
//! See the crate README and `examples/commands.rs` for host context management.
//!
//! A `ui!` view can mount a menu using the window's `Commands` service.
//!
//! ```ignore
//! ui! { Menu("file-menu", &commands) { show_categories: true; show_keybindings: true } }
//! ```
use guiraffe_core::widget_spec;
mod document;
mod widget_specs;

pub mod context;
pub mod control;
pub use control::ControlAction;
mod error;
pub mod keybinding;
pub mod model;
pub mod params;
pub mod ribbon;
pub use context::{Context as WhenContext, ContextStack, When};
pub use error::Error;
pub use keybinding::{Keybinding, Platform};
pub use model::{Bindings, Command, CommandId, Conflict, PrefixConflict, Registry};
pub use params::{Choice, Choices, Draft, Param, ParamKind};
pub use ribbon::{CommandRibbon, RibbonButton, RibbonState, ribbon_scope};

mod menu;
mod service;
pub use menu::{CommandScope, Menu, MenuProps};
pub use service::{
    Commands, ConflictReport, Handler, InvocationSource, PALETTE_SHORTCUT, Payload, RegistryState,
    execute, init, register, registry, unregister,
};
#[cfg(test)]
mod gpui_tests;
