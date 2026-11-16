//! Animated values driven by GPUI animation frames.
//!
//! An [`Animated<T>`] lives inside the *leaf* view that renders it (conventions §6.5).
//! [`Animated::sample`] returns the current value and requests another animation frame only
//! while the value is moving, so at rest it costs nothing and nothing else re-renders.

use std::time::Instant;

use gpui_kit::{App, Styled, Window, px};

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
    timing: Timing,
}

impl<T: Lerp + PartialEq> Animated<T> {
    pub fn new(value: T, timing: Timing) -> Self {
        Self {
            from: value.clone(),
            to: value,
            start: None,
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
        let Some(start) = self.start else {
            return (self.to.clone(), false);
        };
        let (p, done) = self
            .timing
            .sample(now.saturating_duration_since(start), prefs);
        if done {
            (self.to.clone(), false)
        } else {
            (self.from.lerp(&self.to, p), true)
        }
    }

    /// Retarget from the current value. Returns false (and does nothing) if `target` is
    /// already the target, so callers can skip `notify`.
    pub fn set_at(&mut self, target: T, now: Instant, prefs: &MotionPrefs) -> bool {
        if target == self.to {
            return false;
        }
        let (current, _) = self.value_at(now, prefs);
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

/// Apply a transition [`Frame`] to any styled element.
pub trait FrameStyle: Styled + Sized {
    fn frame(self, f: Frame) -> Self {
        let el = self.opacity(f.opacity);
        if f.offset_x == 0.0 && f.offset_y == 0.0 {
            el
        } else {
            el.relative().left(px(f.offset_x)).top(px(f.offset_y))
        }
    }
}

impl<E: Styled> FrameStyle for E {}

/// Timing presets from the current theme tokens.
pub fn tokens(cx: &App) -> crate::timing::MotionTokens {
    MotionPrefs::global(cx).tokens
}
