//! Experimental test infrastructure. The Linux socket probe is opt-in.
#[cfg(all(feature = "e2e", target_os = "linux"))]
pub mod e2e;

#[cfg(feature = "inspect")]
pub mod checks;
pub mod diff;
#[cfg(feature = "inspect")]
pub mod drive;
#[cfg(feature = "inspect")]
pub mod inspect;
#[cfg(feature = "layout")]
pub mod layout;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "inspect")]
pub mod replay;
#[cfg(feature = "snapshot")]
pub mod snapshot;
#[cfg(feature = "inspect")]
pub mod transport;

/// Per-thread allocation counting for warm-repaint assertions. No feature gate:
/// it needs nothing beyond `std` and costs a TLS read only when armed.
pub mod alloc;
