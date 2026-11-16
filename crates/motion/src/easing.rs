//! Easing curves. `Easing::sample(t)` maps linear progress `t ∈ [0, 1]` to eased progress.

/// A timing curve. `CubicBezier` uses CSS semantics (control points P1, P2; P0=(0,0), P3=(1,1)).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Easing {
    Linear,
    /// CSS `ease` (Mantine's default).
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Material "emphasized decelerate", for things entering the screen.
    Emphasized,
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    fn control_points(self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Easing::Linear => None,
            Easing::Ease => Some((0.25, 0.1, 0.25, 1.0)),
            Easing::EaseIn => Some((0.42, 0.0, 1.0, 1.0)),
            Easing::EaseOut => Some((0.0, 0.0, 0.58, 1.0)),
            Easing::EaseInOut => Some((0.42, 0.0, 0.58, 1.0)),
            Easing::Emphasized => Some((0.05, 0.7, 0.1, 1.0)),
            Easing::CubicBezier(a, b, c, d) => Some((a, b, c, d)),
        }
    }

    /// Eased progress for linear progress `t`. `t` is clamped to `[0, 1]`; NaN maps to 0.
    /// Endpoints are exact: `sample(0) == 0`, `sample(1) == 1`.
    pub fn sample(self, t: f32) -> f32 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        if t == 0.0 || t == 1.0 {
            return t;
        }
        match self.control_points() {
            None => t,
            Some((x1, y1, x2, y2)) => {
                // x control points must be within [0,1] for x(s) to be monotonic (CSS rule).
                let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0));
                let s = solve_bezier_x(t, x1, x2);
                bezier(s, y1, y2)
            }
        }
    }
}

/// One coordinate of a cubic bezier with endpoints 0 and 1.
fn bezier(s: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s
}

fn bezier_deriv(s: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * p1 + 6.0 * u * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
}

/// Parameter `s` with `x(s) = x`: Newton steps, falling back to bisection (always converges).
fn solve_bezier_x(x: f32, x1: f32, x2: f32) -> f32 {
    let mut s = x;
    for _ in 0..8 {
        let err = bezier(s, x1, x2) - x;
        if err.abs() < 1e-6 {
            return s;
        }
        let d = bezier_deriv(s, x1, x2);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - err / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    s = x;
    for _ in 0..40 {
        let v = bezier(s, x1, x2);
        if (v - x).abs() < 1e-6 {
            break;
        }
        if v < x {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Easing; 7] = [
        Easing::Linear,
        Easing::Ease,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::Emphasized,
        Easing::CubicBezier(0.68, -0.6, 0.32, 1.6),
    ];

    #[test]
    fn endpoints_exact_and_inputs_clamped() {
        for e in ALL {
            assert_eq!(e.sample(0.0), 0.0, "{e:?}");
            assert_eq!(e.sample(1.0), 1.0, "{e:?}");
            assert_eq!(e.sample(-3.0), 0.0);
            assert_eq!(e.sample(7.0), 1.0);
            assert_eq!(e.sample(f32::NAN), 0.0);
        }
    }

    #[test]
    fn standard_curves_are_monotonic() {
        for e in &ALL[..6] {
            let mut prev = 0.0;
            for i in 1..=200 {
                let v = e.sample(i as f32 / 200.0);
                assert!(v + 1e-5 >= prev, "{e:?} at {i}: {v} < {prev}");
                prev = v;
            }
        }
    }

    #[test]
    fn known_values() {
        // Reference: CSS ease-in-out is symmetric around 0.5.
        assert!((Easing::EaseInOut.sample(0.5) - 0.5).abs() < 1e-3);
        // Ease-out is ahead of linear, ease-in behind.
        assert!(Easing::EaseOut.sample(0.3) > 0.3);
        assert!(Easing::EaseIn.sample(0.3) < 0.3);
        // CSS `ease` at 0.5 ≈ 0.8024 (browser reference value).
        assert!(
            (Easing::Ease.sample(0.5) - 0.8024).abs() < 2e-3,
            "{}",
            Easing::Ease.sample(0.5)
        );
    }

    #[test]
    fn overshooting_bezier_allowed_in_y() {
        let back = Easing::CubicBezier(0.68, -0.6, 0.32, 1.6);
        let max = (1..100)
            .map(|i| back.sample(i as f32 / 100.0))
            .fold(f32::MIN, f32::max);
        let min = (1..100)
            .map(|i| back.sample(i as f32 / 100.0))
            .fold(f32::MAX, f32::min);
        assert!(max > 1.0 && min < 0.0);
    }

    #[test]
    fn out_of_range_x_controls_do_not_break_solver() {
        let weird = Easing::CubicBezier(-5.0, 0.0, 9.0, 1.0);
        for i in 0..=20 {
            assert!(weird.sample(i as f32 / 20.0).is_finite());
        }
    }
}
