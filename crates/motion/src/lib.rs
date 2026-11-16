//! GUIraffe motion: light, fluid animation for GPUI.
//!
//! - Pure, deterministic core: [`Easing`], [`Spring`], [`Timing`], [`Lerp`], [`TransitionKind`], [`Presence`].
//! - GPUI drivers: [`Animated`] and [`sample_presence`] request animation frames only while
//!   moving, from within the leaf view that renders the value.
//! - [`MotionPrefs`]: reduced motion and a slow-motion time scale, applied everywhere.
//!
//! ```ignore
//! struct Badge { glow: Animated<f32> }
//! // on hover: if self.glow.set(1.0, cx) { cx.notify() }
//! // in render: let g = self.glow.sample(window, cx); div().opacity(0.5 + g * 0.5)
//! ```

pub mod animated;
pub mod easing;
pub mod lerp;
pub mod presence;
pub mod spring;
pub mod timing;
pub mod transition;

pub use animated::{Animated, framed, sample_presence, set_presence};
pub use easing::Easing;
pub use lerp::Lerp;
pub use presence::{Presence, PresenceFrame};
pub use spring::Spring;
pub use timing::{MotionPrefs, MotionTokens, Timing, Transition};
pub use transition::{Frame, TransitionKind};

/// Install default motion preferences (idempotent; keeps existing preferences).
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<MotionPrefs>() {
        cx.set_global(MotionPrefs::default());
    }
}

#[cfg(test)]
mod gpui_tests;
