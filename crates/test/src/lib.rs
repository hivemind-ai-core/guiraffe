//! Experimental test infrastructure. The Linux socket probe is opt-in.
#[cfg(all(feature = "e2e", target_os = "linux"))]
pub mod e2e;

pub mod diff;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "snapshot")]
pub mod snapshot;
