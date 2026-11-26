//! GUIraffe state: event-driven application state for GPUI, inspired by ClojureScript's
//! re-frame and adapted to Rust and GPUI's entity model.
//!
//! - **Events** are typed values ([`Event`]) dispatched to a [`Store`] and processed
//!   run-to-completion in FIFO order. Dispatches in one tick are batched.
//! - **Handlers are pure**: they read state and coeffects (time, injected values) and describe
//!   their outcome through [`Fx`]: a new state, more events, and **effects as data**, which
//!   effect handlers registered on the store execute. Test handlers with [`Engine`] alone.
//! - **Interceptors** observe every event (logging, devtools, validation); undo/redo is built in.
//! - **Selections** ([`Store::select`], [`Store::select_memo`]) notify a view only when its
//!   selected value changes; expensive derivations recompute only when their input changes.
//! - **Lenses and binding** (provisional): [`Lens`] paths with per-path change keys,
//!   [`KeyedList`] for collections with stable item identity, and bound controls and lists
//!   ([`bound`], [`bound_list`]) that `ui!` and `store!` generate. See docs/guides/stores.md.
//!
//! State must be `Clone + PartialEq`; use `Arc`/persistent collections inside it so cloning
//! for undo is cheap and comparisons short-circuit.

pub mod bind;
pub mod engine;
pub mod history;
mod keyed;
pub mod lens;
pub mod list;
mod store;

pub use bind::{
    Bind, BindSource, Bindings, Bound, BoundView, Derived, Lenses, Set, Sourced, Watch, bound,
    bound_at, derived,
};
pub use engine::{
    Coeffects, DispatchLater, Effect, Engine, EngineError, Event, EventRecord, Fx, Interceptor,
    Outcome, RedoEvent, UndoEvent,
};
pub use guiraffe_core::protocol::Source;
pub use guiraffe_macros::{Lens, store};
pub use history::{BoundedHistory, HistoryLimits};
pub use keyed::KeyedList;
pub use lens::{ByKey, Compose, Field, Identity, Lens, Lensed, ListLens};
pub use list::{BoundList, ListMode, ListView, RowView, RowViews, bound_list};
pub use store::{Selected, Store, StoreStats, WeakStore};

/// Conventions §4: every crate has an idempotent `init`. The state crate keeps no globals
/// (stores are explicit values, one per app or per window), so this is a no-op today.
pub fn init(_cx: &mut gpui_kit::App) {}

#[cfg(test)]
mod gpui_tests;
