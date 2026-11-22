//! Timing (tween or spring), motion tokens and the reduced-motion preference.

use std::time::Duration;

use gpui_kit::{App, Global};

use crate::{easing::Easing, spring::Spring};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Timing {
    Tween { duration: Duration, easing: Easing },
    Spring(Spring),
}

impl Timing {
    pub const fn tween(duration: Duration, easing: Easing) -> Self {
        Timing::Tween { duration, easing }
    }

    /// Eased progress at `elapsed`, and whether the motion is finished.
    pub fn sample(&self, elapsed: Duration, prefs: &MotionPrefs) -> (f32, bool) {
        let (p, _, done) = self.sample_with_velocity(elapsed, 0.0, prefs);
        (p, done)
    }

    /// Like [`Timing::sample`], for a spring starting with normalised velocity `v0`
    /// (progress units per second). Returns `(progress, velocity, done)`; tweens report 0 velocity.
    pub fn sample_with_velocity(
        &self,
        elapsed: Duration,
        v0: f32,
        prefs: &MotionPrefs,
    ) -> (f32, f32, bool) {
        const EPS: f32 = 1e-4;
        if prefs.reduced {
            return (1.0, 0.0, true);
        }
        let scale = if prefs.time_scale.is_finite() && prefs.time_scale > 0.0 {
            prefs.time_scale
        } else {
            1.0
        };
        let secs = elapsed.as_secs_f32() / scale;
        match *self {
            Timing::Tween { duration, easing } => {
                let d = duration.as_secs_f32();
                if d <= 0.0 || secs >= d {
                    (1.0, 0.0, true)
                } else {
                    (easing.sample(secs / d), 0.0, false)
                }
            }
            Timing::Spring(s) => {
                // Displacement-space velocity is the negated progress velocity.
                if secs >= s.settle_time(-v0, EPS) {
                    (1.0, 0.0, true)
                } else {
                    let (x, v) = s.state(secs, -v0);
                    (1.0 - x, -v, false)
                }
            }
        }
    }
}

/// Theme-level motion tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionTokens {
    pub fast: Duration,
    pub normal: Duration,
    pub slow: Duration,
    pub easing: Easing,
    pub enter_easing: Easing,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            fast: Duration::from_millis(100),
            normal: Duration::from_millis(200),
            slow: Duration::from_millis(300),
            easing: Easing::Ease,
            enter_easing: Easing::Emphasized,
        }
    }
}

impl MotionTokens {
    pub fn normal(&self) -> Timing {
        Timing::tween(self.normal, self.easing)
    }
    pub fn fast(&self) -> Timing {
        Timing::tween(self.fast, self.easing)
    }
    pub fn enter(&self) -> Timing {
        Timing::tween(self.normal, self.enter_easing)
    }
    pub fn slow(&self) -> Timing {
        Timing::tween(self.slow, self.easing)
    }
}

/// A transition preset paired with its timing (conventions §8: `.transition(Transition)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub kind: crate::transition::TransitionKind,
    pub enter: Timing,
    pub exit: Timing,
}

impl Transition {
    pub fn new(kind: crate::transition::TransitionKind, timing: Timing) -> Self {
        Self {
            kind,
            enter: timing,
            exit: timing,
        }
    }
    /// The theme default: a fade with the standard enter/exit timings.
    pub fn themed(kind: crate::transition::TransitionKind, tokens: &MotionTokens) -> Self {
        Self {
            kind,
            enter: tokens.enter(),
            exit: tokens.fast(),
        }
    }
}

/// App-wide motion preferences. Configuration (rarely changes), so a Global is appropriate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionPrefs {
    /// Respect "reduce motion": every animation completes instantly.
    pub reduced: bool,
    /// Slow-motion factor for tuning (devtools): 10.0 plays ten times slower.
    pub time_scale: f32,
    pub tokens: MotionTokens,
    /// Opt-in layout animation: containers that support it interpolate size and
    /// position changes instead of jumping. Off by default.
    pub layout: LayoutMotion,
}

impl Default for MotionPrefs {
    fn default() -> Self {
        Self {
            reduced: false,
            time_scale: 1.0,
            tokens: MotionTokens::default(),
            layout: LayoutMotion::default(),
        }
    }
}

/// App-wide default for layout animation. Containers override it per instance
/// with `.enabled(bool)` / `.timing(Timing)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMotion {
    /// Whether layout changes animate by default. Off unless the app opts in.
    pub enabled: bool,
    /// How sizes and positions move. A spring, so retargeting keeps velocity.
    pub timing: Timing,
}

impl Default for LayoutMotion {
    fn default() -> Self {
        Self {
            enabled: false,
            timing: Timing::Spring(Spring::SNAPPY),
        }
    }
}

impl MotionPrefs {
    /// Whether a container with override `enabled` animates layout under these preferences.
    /// Reduced motion always wins: it makes every container a plain pass-through.
    pub fn layout_enabled(&self, enabled: Option<bool>) -> bool {
        !self.reduced && enabled.unwrap_or(self.layout.enabled)
    }
}

impl Global for MotionPrefs {}

impl MotionPrefs {
    /// Effective preferences: ours, with the platform/app reduce-motion setting folded in.
    pub fn global(cx: &App) -> MotionPrefs {
        let mut p = cx.try_global::<MotionPrefs>().copied().unwrap_or_default();
        p.reduced |= cx.reduce_motion();
        p
    }
    pub fn set(prefs: MotionPrefs, cx: &mut App) {
        cx.set_global(prefs);
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn tween_progress_and_completion() {
        let t = Timing::tween(ms(200), Easing::Linear);
        let p = MotionPrefs::default();
        assert_eq!(t.sample(ms(0), &p), (0.0, false));
        assert_eq!(t.sample(ms(100), &p), (0.5, false));
        assert_eq!(t.sample(ms(200), &p), (1.0, true));
        assert_eq!(t.sample(ms(999), &p), (1.0, true));
        assert_eq!(
            Timing::tween(ms(0), Easing::Ease).sample(ms(0), &p),
            (1.0, true)
        );
    }

    #[test]
    fn reduced_motion_and_time_scale() {
        let t = Timing::tween(ms(200), Easing::Linear);
        assert_eq!(
            t.sample(
                ms(0),
                &MotionPrefs {
                    reduced: true,
                    ..Default::default()
                }
            ),
            (1.0, true)
        );
        let slow = MotionPrefs {
            time_scale: 10.0,
            ..Default::default()
        };
        assert_eq!(t.sample(ms(1000), &slow), (0.5, false));
        let bad = MotionPrefs {
            time_scale: f32::NAN,
            ..Default::default()
        };
        assert_eq!(t.sample(ms(100), &bad), (0.5, false));
    }

    #[test]
    fn spring_with_velocity_and_zero_damping_finishes() {
        let p = MotionPrefs::default();
        let undamped = Timing::Spring(Spring {
            stiffness: 200.0,
            damping: 0.0,
            mass: 1.0,
        });
        assert!(
            undamped.sample(Duration::from_secs(11), &p).1,
            "capped, never animates forever"
        );
        let (_, v, _) = Timing::Spring(Spring::SNAPPY).sample_with_velocity(ms(0), 5.0, &p);
        assert!((v - 5.0).abs() < 1e-3, "initial velocity is honoured: {v}");
    }

    #[test]
    fn spring_timing_finishes() {
        let t = Timing::Spring(Spring::SNAPPY);
        let p = MotionPrefs::default();
        assert!(!t.sample(ms(16), &p).1);
        assert_eq!(t.sample(Duration::from_secs(3), &p), (1.0, true));
    }
}
