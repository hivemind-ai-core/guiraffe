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
        if prefs.reduced {
            return (1.0, true);
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
                    (1.0, true)
                } else {
                    (easing.sample(secs / d), false)
                }
            }
            Timing::Spring(s) => {
                if s.is_settled(secs, 0.0, 1e-3) && secs >= s.settle_time(0.0, 1e-3) {
                    (1.0, true)
                } else {
                    (s.progress(secs), false)
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
}

/// App-wide motion preferences. Configuration (rarely changes), so a Global is appropriate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionPrefs {
    /// Respect "reduce motion": every animation completes instantly.
    pub reduced: bool,
    /// Slow-motion factor for tuning (devtools): 10.0 plays ten times slower.
    pub time_scale: f32,
    pub tokens: MotionTokens,
}

impl Default for MotionPrefs {
    fn default() -> Self {
        Self {
            reduced: false,
            time_scale: 1.0,
            tokens: MotionTokens::default(),
        }
    }
}

impl Global for MotionPrefs {}

impl MotionPrefs {
    pub fn global(cx: &App) -> MotionPrefs {
        cx.try_global::<MotionPrefs>().copied().unwrap_or_default()
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
    fn spring_timing_finishes() {
        let t = Timing::Spring(Spring::SNAPPY);
        let p = MotionPrefs::default();
        assert!(!t.sample(ms(16), &p).1);
        assert_eq!(t.sample(Duration::from_secs(3), &p), (1.0, true));
    }
}
