//! GUIraffe data-driven UI (spec/10): documents (typically designed in Mocket) compiled once
//! into templates, instantiated by a [`DataUi`] host that can live anywhere in a static UI.
//!
//! - **Pre-created:** compile resolves widget kinds to shared `dyn Widget`s and binding/event
//!   names to indices. Rendering never parses or looks names up.
//! - **Bindings by id:** documents reference view-data *slots* and *events* by name; logic
//!   resolves them once in [`Logic::bind`].
//! - **Minimal data motion:** logic answers events with slot [`Patch`]es; unchanged values are
//!   no-ops and don't re-render. The boundary is message-shaped (future wasm logic).

pub mod builtins;
mod host;
pub mod ir;
pub mod slots;
pub mod template;
pub mod value;
pub mod widget;

pub use host::{BoundNode, DataUi, DataUiHandle, Logic, NoLogic};
pub use ir::{Document, Node};
pub use slots::{Patch, PatchBuf, SlotId, SlotTable};
pub use template::{CompileError, Diagnostic, EventId, MAX_DEPTH, Names, Prop, Severity, Template};
pub use value::Value;
pub use widget::{FnWidget, NodeView, Registry, Schema, Widget, widget};
