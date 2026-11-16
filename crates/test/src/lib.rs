//! Experimental test infrastructure. The Linux socket probe is opt-in.
#[cfg(all(feature = "e2e", target_os = "linux"))]
pub mod e2e;
