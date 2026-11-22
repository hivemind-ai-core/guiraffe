//! Developer tools with no runtime or dependency cost unless `devtools` is enabled.
//!
//! Apps should make this dependency optional behind their own `devtools` feature.
//! Call [`init`], create a [`Devtools`] handle per window, mount [`root`], and wrap
//! each inspected view's render output with [`Devtools::track`]. Unwrapped views are
//! deliberately not enumerated: GPUI does not expose their inherited StyleContext.
//!
//! With the `devtools` feature enabled and a per-window `Devtools` handle named
//! `tools`, embed a tracked view in a `ui!` tree.
//!
//! ```ignore
//! ui! { div { {tools.track("settings", ui! { div { "Settings" } })} } }
//! ```

#![cfg_attr(not(feature = "devtools"), allow(rustdoc::broken_intra_doc_links))]

#[cfg(feature = "devtools")]
mod model;
#[cfg(feature = "devtools")]
mod state;
#[cfg(feature = "devtools")]
mod view;
#[cfg(feature = "devtools")]
pub use model::*;
#[cfg(feature = "devtools")]
pub use state::*;
#[cfg(feature = "devtools")]
pub use view::*;

#[cfg(feature = "devtools")]
pub use serde_json::Value as DiagnosticValue;
