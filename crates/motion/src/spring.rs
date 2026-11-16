//! Damped harmonic springs, solved analytically (exact for any time step, frame-rate independent).

/// Spring parameters. Displacement starts at `1.0` (from) and settles at `0.0` (to); callers
/// map that to their own range with [`Spring::progress`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl Spring {
    /// Smooth, no overshoot (critically damped).
    pub const GENTLE: Spring = Spring {
        stiffness: 170.0,
        damping: 26.077,
        mass: 1.0,
    };
    /// Quick with a hint of overshoot.
    pub const SNAPPY: Spring = Spring {
        stiffness: 400.0,
        damping: 32.0,
        mass: 1.0,
    };
    /// Playful overshoot.
    pub const BOUNCY: Spring = Spring {
        stiffness: 300.0,
        damping: 14.0,
        mass: 1.0,
    };

    fn sanitized(self) -> (f32, f32, f32) {
        let pos = |v: f32, d: f32| if v.is_finite() && v > 0.0 { v } else { d };
        let damping = if self.damping.is_finite() && self.damping >= 0.0 {
            self.damping
        } else {
            26.0
        };
        (pos(self.stiffness, 170.0), damping, pos(self.mass, 1.0))
    }

    /// Displacement and velocity at time `t` seconds, for start displacement 1 and start velocity `v0`
    /// (in displacement units per second; negative moves toward the target).
    pub fn state(self, t: f32, v0: f32) -> (f32, f32) {
        let (k, c, m) = self.sanitized();
        let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
        let w0 = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());
        let x0 = 1.0f32;
        if (zeta - 1.0).abs() < 1e-3 {
            // Critically damped.
            let b = v0 + w0 * x0;
            let e = (-w0 * t).exp();
            let x = (x0 + b * t) * e;
            let v = (b - w0 * (x0 + b * t)) * e;
            (x, v)
        } else if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let a = x0;
            let b = (v0 + zeta * w0 * x0) / wd;
            let e = (-zeta * w0 * t).exp();
            let (s, co) = (wd * t).sin_cos();
            let x = e * (a * co + b * s);
            let v = e * ((-zeta * w0) * (a * co + b * s) + (-a * wd * s + b * wd * co));
            (x, v)
        } else {
            let r = (zeta * zeta - 1.0).sqrt();
            let r1 = -w0 * (zeta - r);
            let r2 = -w0 * (zeta + r);
            let c2 = (v0 - r1 * x0) / (r2 - r1);
            let c1 = x0 - c2;
            let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        }
    }

    /// Progress from 0 (start) toward 1 (target) at `t` seconds, starting at rest. May overshoot 1.
    pub fn progress(self, t: f32) -> f32 {
        1.0 - self.state(t, 0.0).0
    }

    /// Hard cap on any spring's duration, so a weakly damped spring can never animate forever.
    pub const MAX_DURATION: f32 = 10.0;

    /// Time (seconds) after which the displacement envelope stays below `epsilon`, starting
    /// from displacement 1 with velocity `v0`. Computed analytically in O(1) and capped at
    /// [`Spring::MAX_DURATION`] (zero damping never settles, so it hits the cap).
    pub fn settle_time(self, v0: f32, epsilon: f32) -> f32 {
        let (k, c, m) = self.sanitized();
        let eps = if epsilon.is_finite() && epsilon > 0.0 {
            epsilon
        } else {
            1e-4
        };
        let v0 = if v0.is_finite() { v0 } else { 0.0 };
        let w0 = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());
        let t = if zeta <= 1e-6 {
            Self::MAX_DURATION
        } else if (zeta - 1.0).abs() < 1e-3 {
            // |x(t)| <= (1 + |b| t) e^{-w0 t}; solve by fixed-point iteration (monotone, converges).
            let b = (v0 + w0).abs();
            let mut t = (1.0f32 / eps).ln() / w0;
            for _ in 0..16 {
                t = ((1.0 + b * t) / eps).ln() / w0;
            }
            t
        } else if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let amp = (1.0 + ((v0 + zeta * w0) / wd).powi(2)).sqrt();
            (amp / eps).ln() / (zeta * w0)
        } else {
            let r = (zeta * zeta - 1.0).sqrt();
            let r1 = -w0 * (zeta - r); // slow root (closest to 0)
            let r2 = -w0 * (zeta + r);
            let c2 = (v0 - r1) / (r2 - r1);
            let c1 = 1.0 - c2;
            ((c1.abs() + c2.abs()) / eps).ln() / -r1
        };
        if t.is_finite() {
            t.clamp(0.0, Self::MAX_DURATION)
        } else {
            Self::MAX_DURATION
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_from_and_settles_at_target() {
        for s in [Spring::GENTLE, Spring::SNAPPY, Spring::BOUNCY] {
            assert!(s.progress(0.0).abs() < 1e-6);
            assert!((s.progress(5.0) - 1.0).abs() < 1e-3, "{s:?}");
        }
    }

    #[test]
    fn gentle_never_overshoots_bouncy_does() {
        let max = |s: Spring| {
            (0..2000)
                .map(|i| s.progress(i as f32 / 1000.0))
                .fold(f32::MIN, f32::max)
        };
        assert!(max(Spring::GENTLE) <= 1.0 + 1e-4);
        assert!(max(Spring::BOUNCY) > 1.05);
    }

    #[test]
    fn velocity_matches_numeric_derivative() {
        for s in [
            Spring::GENTLE,
            Spring::SNAPPY,
            Spring {
                stiffness: 100.0,
                damping: 40.0,
                mass: 1.0,
            },
        ] {
            for &t in &[0.01f32, 0.1, 0.3] {
                let h = 1e-3;
                let numeric = (s.state(t + h, 2.0).0 - s.state(t - h, 2.0).0) / (2.0 * h);
                let (_, v) = s.state(t, 2.0);
                assert!(
                    (numeric - v).abs() < 0.05 * v.abs().max(1.0),
                    "{s:?} t={t}: {numeric} vs {v}"
                );
            }
        }
    }

    #[test]
    fn settle_time_bounds_the_motion() {
        // After the analytic settle time, displacement stays within epsilon (checked densely).
        for s in [
            Spring::GENTLE,
            Spring::SNAPPY,
            Spring::BOUNCY,
            Spring {
                stiffness: 100.0,
                damping: 60.0,
                mass: 1.0,
            },
        ] {
            for v0 in [0.0, -8.0, 8.0] {
                let eps = 1e-3;
                let t = s.settle_time(v0, eps);
                for i in 0..2000 {
                    let x = s.state(t + i as f32 / 200.0, v0).0;
                    assert!(x.abs() <= eps * 1.01, "{s:?} v0={v0} t={t}+{i}: {x}");
                }
            }
        }
    }

    #[test]
    fn undamped_spring_is_capped() {
        let s = Spring {
            stiffness: 200.0,
            damping: 0.0,
            mass: 1.0,
        };
        assert_eq!(s.settle_time(0.0, 1e-3), Spring::MAX_DURATION);
    }

    #[test]
    fn settle_time_is_reasonable_and_bounded() {
        // (analytic, so it is cheap enough to call every frame)
        let g = Spring::GENTLE.settle_time(0.0, 1e-3);
        assert!(g > 0.2 && g < 1.5, "{g}");
        assert!(Spring::BOUNCY.settle_time(0.0, 1e-3) > g);
        let weird = Spring {
            stiffness: f32::NAN,
            damping: -1.0,
            mass: 0.0,
        };
        assert!(weird.progress(0.5).is_finite());
        assert!(weird.settle_time(0.0, 1e-3) <= 10.0);
    }

    #[test]
    fn overdamped_is_monotonic() {
        let s = Spring {
            stiffness: 100.0,
            damping: 60.0,
            mass: 1.0,
        };
        let mut prev = 0.0;
        for i in 1..1000 {
            let p = s.progress(i as f32 / 200.0);
            assert!(p + 1e-6 >= prev);
            prev = p;
        }
    }
}
