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
//!
//! State must be `Clone + PartialEq`; use `Arc`/persistent collections inside it so cloning
//! for undo is cheap and comparisons short-circuit.

pub mod engine;
mod store;

pub use engine::{
    Coeffects, DispatchLater, Effect, Engine, EngineError, Event, EventRecord, Fx, Interceptor,
    Outcome, RedoEvent, UndoEvent,
};
pub use store::{Selected, Store, StoreStats};

#[cfg(test)]
mod gpui_tests;
