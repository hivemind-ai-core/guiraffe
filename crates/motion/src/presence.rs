//! Presence: keep an element mounted while it animates out. Pure state machine; time is passed in.

use std::time::{Duration, Instant};

use crate::timing::{MotionPrefs, Timing};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Hidden,
    /// Animating toward visible, from visibility `from` at `start`.
    Entering {
        start: Instant,
        from: f32,
    },
    Shown,
    Exiting {
        start: Instant,
        from: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Presence {
    phase: Phase,
    enter: Timing,
    exit: Timing,
}

/// What to render at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PresenceFrame {
    /// Not mounted: render nothing.
    Unmounted,
    /// Mounted with `visibility` ∈ [0,1] (may overshoot with springs); `animating` means
    /// another frame is needed.
    Mounted { visibility: f32, animating: bool },
}

impl Presence {
    pub fn new(visible: bool, enter: Timing, exit: Timing) -> Self {
        Self {
            phase: if visible { Phase::Shown } else { Phase::Hidden },
            enter,
            exit,
        }
    }

    pub fn is_visible_target(&self) -> bool {
        matches!(self.phase, Phase::Entering { .. } | Phase::Shown)
    }

    /// Change the target. Returns whether anything changed (so callers notify only then).
    /// Reversing mid-animation continues from the current visibility (no jump).
    pub fn set_visible(&mut self, visible: bool, now: Instant, prefs: &MotionPrefs) -> bool {
        if visible == self.is_visible_target() {
            return false;
        }
        let current = match self.frame(now, prefs) {
            PresenceFrame::Unmounted => 0.0,
            PresenceFrame::Mounted { visibility, .. } => visibility.clamp(0.0, 1.0),
        };
        self.phase = if visible {
            Phase::Entering {
                start: now,
                from: current,
            }
        } else {
            Phase::Exiting {
                start: now,
                from: current,
            }
        };
        true
    }

    fn elapsed(now: Instant, start: Instant) -> Duration {
        now.saturating_duration_since(start)
    }

    /// Sample the presence at `now`, settling finished phases.
    pub fn frame(&mut self, now: Instant, prefs: &MotionPrefs) -> PresenceFrame {
        match self.phase {
            Phase::Hidden => PresenceFrame::Unmounted,
            Phase::Shown => PresenceFrame::Mounted {
                visibility: 1.0,
                animating: false,
            },
            Phase::Entering { start, from } => {
                let (p, done) =
                    scaled(self.enter, 1.0 - from).sample(Self::elapsed(now, start), prefs);
                if done {
                    self.phase = Phase::Shown;
                    PresenceFrame::Mounted {
                        visibility: 1.0,
                        animating: false,
                    }
                } else {
                    PresenceFrame::Mounted {
                        visibility: from + (1.0 - from) * p,
                        animating: true,
                    }
                }
            }
            Phase::Exiting { start, from } => {
                let (p, done) = scaled(self.exit, from).sample(Self::elapsed(now, start), prefs);
                if done {
                    self.phase = Phase::Hidden;
                    PresenceFrame::Unmounted
                } else {
                    PresenceFrame::Mounted {
                        // A spring may overshoot past hidden; never render below fully hidden.
                        visibility: (from * (1.0 - p)).max(0.0),
                        animating: true,
                    }
                }
            }
        }
    }
}

/// Tweens covering only part of the distance take a proportional part of the duration
/// (reversing at 5 % visibility exits in 5 % of the time). Springs are physical already.
fn scaled(t: Timing, remaining: f32) -> Timing {
    match t {
        Timing::Tween { duration, easing } => {
            let r = if remaining.is_finite() {
                remaining.clamp(0.0, 1.0)
            } else {
                1.0
            };
            Timing::Tween {
                duration: duration.mul_f32(r),
                easing,
            }
        }
        spring => spring,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;

    fn lin(ms: u64) -> Timing {
        Timing::tween(Duration::from_millis(ms), Easing::Linear)
    }
    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn enter_then_exit_unmounts_only_after_exit() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut pr = Presence::new(false, lin(100), lin(200));
        assert_eq!(pr.frame(t0, &p), PresenceFrame::Unmounted);
        assert!(pr.set_visible(true, t0, &p));
        assert!(!pr.set_visible(true, t0, &p), "no-op returns false");
        assert_eq!(
            pr.frame(at(t0, 50), &p),
            PresenceFrame::Mounted {
                visibility: 0.5,
                animating: true
            }
        );
        assert_eq!(
            pr.frame(at(t0, 100), &p),
            PresenceFrame::Mounted {
                visibility: 1.0,
                animating: false
            }
        );
        assert!(pr.set_visible(false, at(t0, 100), &p));
        assert_eq!(
            pr.frame(at(t0, 200), &p),
            PresenceFrame::Mounted {
                visibility: 0.5,
                animating: true
            }
        );
        assert_eq!(pr.frame(at(t0, 300), &p), PresenceFrame::Unmounted);
    }

    #[test]
    fn reversing_mid_flight_continues_without_jump() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut pr = Presence::new(false, lin(100), lin(100));
        pr.set_visible(true, t0, &p);
        let PresenceFrame::Mounted { visibility: v1, .. } = pr.frame(at(t0, 30), &p) else {
            panic!()
        };
        pr.set_visible(false, at(t0, 30), &p);
        let PresenceFrame::Mounted { visibility: v2, .. } = pr.frame(at(t0, 30), &p) else {
            panic!()
        };
        assert!((v1 - v2).abs() < 1e-6, "{v1} vs {v2}");
        // Exit from 0.3 takes 30% of 100ms and unmounts.
        assert!(matches!(
            pr.frame(at(t0, 45), &p),
            PresenceFrame::Mounted { .. }
        ));
        assert_eq!(pr.frame(at(t0, 60), &p), PresenceFrame::Unmounted);
    }

    #[test]
    fn spring_presence_never_goes_below_hidden_and_toggling_in_one_frame_is_stable() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let spring = Timing::Spring(crate::spring::Spring::BOUNCY);
        let mut pr = Presence::new(true, spring, spring);
        pr.set_visible(false, t0, &p);
        for i in 0..300 {
            if let PresenceFrame::Mounted { visibility, .. } = pr.frame(at(t0, i * 5), &p) {
                assert!(visibility >= 0.0);
            }
        }
        let mut pr = Presence::new(false, lin(100), lin(100));
        for _ in 0..10 {
            pr.set_visible(true, t0, &p);
            pr.set_visible(false, t0, &p);
        }
        assert_eq!(
            pr.frame(t0, &p),
            PresenceFrame::Unmounted,
            "net hidden, no residue"
        );
    }

    #[test]
    fn reduced_motion_is_instant() {
        let p = MotionPrefs {
            reduced: true,
            ..Default::default()
        };
        let t0 = Instant::now();
        let mut pr = Presence::new(false, lin(100), lin(100));
        pr.set_visible(true, t0, &p);
        assert_eq!(
            pr.frame(t0, &p),
            PresenceFrame::Mounted {
                visibility: 1.0,
                animating: false
            }
        );
        pr.set_visible(false, t0, &p);
        assert_eq!(pr.frame(t0, &p), PresenceFrame::Unmounted);
    }

    #[test]
    fn clock_going_backwards_is_safe() {
        let p = MotionPrefs::default();
        let t0 = Instant::now() + Duration::from_secs(1);
        let mut pr = Presence::new(false, lin(100), lin(100));
        pr.set_visible(true, t0, &p);
        assert_eq!(
            pr.frame(t0 - Duration::from_millis(500), &p),
            PresenceFrame::Mounted {
                visibility: 0.0,
                animating: true
            }
        );
    }
}
