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
pub mod catalog;
mod host;
pub mod ir;
pub mod slots;
pub mod template;
pub mod validation;
pub mod value;
pub mod widget;

pub use host::{BoundNode, DataUi, DataUiHandle, Logic, NoLogic};
pub use ir::{Document, Node};
pub use slots::{Patch, PatchBuf, SlotId, SlotTable};
pub use template::{CompileError, Diagnostic, EventId, MAX_DEPTH, Names, Prop, Severity, Template};
pub use value::Value;
pub use widget::{FnWidget, NodeView, Registry, Schema, Widget, widget};

pub mod bindings;
pub mod payload;
pub use bindings::{Bindings, Event, Slot, SlotValue};
pub use payload::{ItemChange, ItemContext, Payload, ScopedPayload};

mod repeat;

mod controls;

mod position;

mod viewport;
pub use viewport::{RepeatViewport, RepeatViewportError};

/// Build-generated resources for screen authoring tools.
pub mod catalogue_artifacts {
    pub const CATALOGUE_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/catalogue.json"));
    pub const SCREEN_SCHEMA: &str = include_str!(concat!(env!("OUT_DIR"), "/screen.schema.json"));
    pub const REFERENCE_MARKDOWN: &str = include_str!(concat!(env!("OUT_DIR"), "/catalogue.md"));
}

extern crate self as guiraffe_ui;

pub use guiraffe_macros::component;

/// Build a GPUI element with ordinary Rust builders and declarative children.
///
/// ```
/// use gpui_kit::{div, prelude::*};
/// use guiraffe_ui::{component, ui};
///
/// #[component]
/// fn Greeting(name: &'static str, #[prop(default)] loud: bool) -> impl IntoElement {
///     ui! { div { flex {name} if loud { "!" } } }
/// }
///
/// let element = ui! { Greeting("Ada") { loud: true } };
/// # let _ = element.into_element();
/// ```
pub use guiraffe_macros::ui;

/// Declarative UI authoring imports.
pub mod prelude {
    pub use crate::{component, ui};
}

#[doc(hidden)]
#[path = "macro_support.rs"]
pub mod __macro;
