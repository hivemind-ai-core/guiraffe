//! Animated values driven by GPUI animation frames.
//!
//! An [`Animated<T>`] lives inside the *leaf* view that renders it (conventions §6.5).
//! [`Animated::sample`] returns the current value and requests another animation frame only
//! while the value is moving, so at rest it costs nothing and nothing else re-renders.

use std::time::Instant;

use gpui_kit::{App, Div, IntoElement, ParentElement, Styled, Window, div, px};

use crate::{
    lerp::Lerp,
    presence::{Presence, PresenceFrame},
    timing::{MotionPrefs, Timing},
    transition::Frame,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Animated<T: Lerp> {
    from: T,
    to: T,
    start: Option<Instant>,
    /// Normalised start velocity (progress/s) carried over from an interrupted spring.
    v0: f32,
    timing: Timing,
}

impl<T: Lerp + PartialEq> Animated<T> {
    pub fn new(value: T, timing: Timing) -> Self {
        Self {
            from: value.clone(),
            to: value,
            start: None,
            v0: 0.0,
            timing,
        }
    }

    pub fn target(&self) -> &T {
        &self.to
    }

    pub fn set_timing(&mut self, timing: Timing) {
        self.timing = timing;
    }

    /// Value at `now` and whether it is still moving (pure; for tests and custom drivers).
    pub fn value_at(&self, now: Instant, prefs: &MotionPrefs) -> (T, bool) {
        let (v, _, moving) = self.sample_full(now, prefs);
        (v, moving)
    }

    fn sample_full(&self, now: Instant, prefs: &MotionPrefs) -> (T, f32, bool) {
        let Some(start) = self.start else {
            return (self.to.clone(), 0.0, false);
        };
        let (p, vel, done) =
            self.timing
                .sample_with_velocity(now.saturating_duration_since(start), self.v0, prefs);
        if done {
            (self.to.clone(), 0.0, false)
        } else {
            (self.from.lerp(&self.to, p), vel, true)
        }
    }

    /// Retarget from the current value. Returns false (and does nothing) if `target` is
    /// already the target, so callers can skip `notify`.
    pub fn set_at(&mut self, target: T, now: Instant, prefs: &MotionPrefs) -> bool {
        if target == self.to {
            return false;
        }
        let (current, vel, moving) = self.sample_full(now, prefs);
        // Keep velocity continuous for springs: re-express the old velocity on the new segment.
        let ratio = if moving {
            self.from.span_ratio(&self.to, &current, &target)
        } else {
            0.0
        };
        self.v0 = if matches!(self.timing, Timing::Spring(_)) && ratio.is_finite() {
            vel * ratio
        } else {
            0.0
        };
        self.from = current;
        self.to = target;
        self.start = Some(now);
        true
    }

    /// Jump to `value` without animating.
    pub fn jump(&mut self, value: T) {
        self.from = value.clone();
        self.to = value;
        self.start = None;
        self.v0 = 0.0;
    }

    /// Retarget using the app clock and preferences.
    pub fn set(&mut self, target: T, cx: &App) -> bool {
        self.set_at(
            target,
            cx.background_executor().now(),
            &MotionPrefs::global(cx),
        )
    }

    /// Current value for rendering; schedules the next frame while moving.
    pub fn sample(&mut self, window: &mut Window, cx: &App) -> T {
        let (v, moving) = self.value_at(cx.background_executor().now(), &MotionPrefs::global(cx));
        if moving {
            window.request_animation_frame();
        } else if self.start.is_some() {
            // Settle so later samples are a cheap clone.
            self.start = None;
            self.v0 = 0.0;
            self.from = self.to.clone();
        }
        v
    }
}

/// Render-time helper for [`Presence`]: returns the visibility to render (or `None` when
/// unmounted) and schedules frames while animating.
pub fn sample_presence(presence: &mut Presence, window: &mut Window, cx: &App) -> Option<f32> {
    match presence.frame(cx.background_executor().now(), &MotionPrefs::global(cx)) {
        PresenceFrame::Unmounted => None,
        PresenceFrame::Mounted {
            visibility,
            animating,
        } => {
            if animating {
                window.request_animation_frame();
            }
            Some(visibility)
        }
    }
}

/// Toggle a presence with the app clock. Returns whether the target changed.
pub fn set_presence(presence: &mut Presence, visible: bool, cx: &App) -> bool {
    presence.set_visible(
        visible,
        cx.background_executor().now(),
        &MotionPrefs::global(cx),
    )
}

/// Wrap `child` so it renders at transition frame `f`.
///
/// The offset lives on a wrapper (`position: relative` on the *wrapper*), so the child's own
/// positioning (absolute overlays, insets) is never altered and nothing jumps when the
/// offset returns to zero.
pub fn framed(child: impl IntoElement, f: Frame) -> Div {
    div()
        .relative()
        .left(px(f.offset_x))
        .top(px(f.offset_y))
        .opacity(f.opacity.clamp(0.0, 1.0))
        .child(child)
}
