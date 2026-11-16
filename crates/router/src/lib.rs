//! GUIraffe router: typed, TanStack-style routing for GPUI apps.
//!
//! - [`routes!`] declares a typed route enum with path patterns (`/projects/:id`, `/docs/*rest`);
//!   unbound fields are typed **search params**. Parsing and formatting round-trip.
//! - [`History`] is a pure per-window history (push/replace/back/forward, capped).
//! - [`Router`] is built on `guiraffe-state`: navigation is events on a store, so interceptors
//!   (logging, devtools) and change-only selections work for routes. Guards can block
//!   navigation (pair with window close guards for "unsaved changes").
//!
//! Layouts are plain Rust: `match router.current(cx) { AppRoute::Project { .. } => ... }`
//! inside a layout view, which re-renders only when its selected part of the route changes.

pub mod history;
pub mod matcher;
pub mod path;
pub mod route;
mod router;

pub use history::History;
pub use path::Location;
pub use route::Route;
pub use router::{Guard, GuardId, GuardOutcome, Nav, Router, RouterState};

/// Idempotent init (conventions §4); the router keeps no globals.
pub fn init(_cx: &mut gpui_kit::App) {}

#[cfg(test)]
mod tests;
