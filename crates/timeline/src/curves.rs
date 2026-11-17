//! Keyframe curves for dope sheets and curve editors: keys with constant, linear or bezier
//! interpolation (bezier uses per-key tangent handles in (time, value) space).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Interp {
    Constant,
    Linear,
    Bezier,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub t: f64,
    pub v: f64,
    /// Interpolation to the next key.
    pub interp: Interp,
    /// Handles relative to the key: incoming (dt ≤ 0) and outgoing (dt ≥ 0).
    pub in_handle: (f64, f64),
    pub out_handle: (f64, f64),
}

impl Key {
    pub fn new(t: f64, v: f64, interp: Interp) -> Key {
        Key {
            t,
            v,
            interp,
            in_handle: (0.0, 0.0),
            out_handle: (0.0, 0.0),
        }
    }
}

/// A curve: keys sorted by time, unique times.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Curve {
    keys: Vec<Key>,
}

impl Curve {
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Insert or replace the key at `key.t`. Returns its index.
    pub fn set(&mut self, key: Key) -> Option<usize> {
        if !(key.t.is_finite()
            && key.v.is_finite()
            && key.in_handle.0.is_finite()
            && key.in_handle.1.is_finite()
            && key.out_handle.0.is_finite()
            && key.out_handle.1.is_finite())
        {
            return None;
        }
        let i = self.keys.partition_point(|k| k.t < key.t);
        if self.keys.get(i).is_some_and(|k| k.t == key.t) {
            self.keys[i] = key;
        } else {
            self.keys.insert(i, key);
        }
        Some(i)
    }

    /// Move key `i` in time, keeping order: it cannot pass its neighbours.
    pub fn move_key(&mut self, i: usize, t: f64, v: f64) -> bool {
        if i >= self.keys.len() || !(t.is_finite() && v.is_finite()) {
            return false;
        }
        const EPS: f64 = 1e-6;
        let lo = if i == 0 {
            f64::NEG_INFINITY
        } else {
            self.keys[i - 1].t + EPS
        };
        let hi = self
            .keys
            .get(i + 1)
            .map(|k| k.t - EPS)
            .unwrap_or(f64::INFINITY);
        let k = &mut self.keys[i];
        let before = (k.t, k.v);
        // Keys closer than 2*EPS leave no room: stay put in time rather than panic.
        k.t = if lo <= hi { t.clamp(lo, hi) } else { k.t };
        k.v = v;
        (k.t, k.v) != before
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.keys.len() {
            false
        } else {
            self.keys.remove(index);
            true
        }
    }
    pub fn set_interpolation(&mut self, index: usize, interp: Interp) -> bool {
        let Some(key) = self.keys.get_mut(index) else {
            return false;
        };
        if key.interp == interp {
            return false;
        }
        key.interp = interp;
        true
    }
    pub fn set_tangent(
        &mut self,
        index: usize,
        incoming: bool,
        handle: (f64, f64),
        mode: TangentMode,
    ) -> bool {
        if !handle.0.is_finite() || !handle.1.is_finite() {
            return false;
        }
        let Some(key) = self.keys.get_mut(index) else {
            return false;
        };
        let old = *key;
        let mut handle = (
            if incoming {
                handle.0.min(0.)
            } else {
                handle.0.max(0.)
            },
            handle.1,
        );
        if mode == TangentMode::Flat {
            handle.1 = 0.;
        }
        let opposite = if incoming {
            key.out_handle
        } else {
            key.in_handle
        };
        let other = match mode {
            TangentMode::Mirrored => (-handle.0, -handle.1),
            TangentMode::Aligned => {
                let length = handle.0.hypot(handle.1);
                if length > 0. {
                    let scale = opposite.0.hypot(opposite.1) / length;
                    (-handle.0 * scale, -handle.1 * scale)
                } else {
                    opposite
                }
            }
            TangentMode::Flat => (opposite.0, 0.),
            TangentMode::Free => opposite,
        };
        if incoming {
            key.in_handle = handle;
            key.out_handle = other;
        } else {
            key.out_handle = handle;
            key.in_handle = other;
        }
        *key != old
    }
    /// Insert a key sampled from the existing curve. Cubic segments split using
    /// de Casteljau, preserving their shape; constant/linear retain their mode.
    pub fn insert_at(&mut self, t: f64) -> Option<usize> {
        if !t.is_finite() {
            return None;
        }
        let index = self.keys.partition_point(|k| k.t < t);
        if self.keys.get(index).is_some_and(|k| k.t == t) {
            return Some(index);
        }
        let v = self.eval(t);
        if index == 0 || index == self.keys.len() {
            return self.set(Key::new(t, v, Interp::Linear));
        }
        let (a, b) = (self.keys[index - 1], self.keys[index]);
        let mut key = Key::new(t, v, a.interp);
        if a.interp == Interp::Bezier {
            let span = b.t - a.t;
            let p = [
                (a.t, a.v),
                (a.t + a.out_handle.0.clamp(0., span), a.v + a.out_handle.1),
                (b.t + b.in_handle.0.clamp(-span, 0.), b.v + b.in_handle.1),
                (b.t, b.v),
            ];
            let lerp = |a: (f64, f64), b: (f64, f64), u: f64| {
                (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u)
            };
            let split = |u: f64| {
                let q = [
                    lerp(p[0], p[1], u),
                    lerp(p[1], p[2], u),
                    lerp(p[2], p[3], u),
                ];
                let r = [lerp(q[0], q[1], u), lerp(q[1], q[2], u)];
                (q, r, lerp(r[0], r[1], u))
            };
            let (mut lo, mut hi) = (0., 1.);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.;
                if split(mid).2.0 < t {
                    lo = mid
                } else {
                    hi = mid
                }
            }
            let (q, r, centre) = split((lo + hi) / 2.);
            key.v = centre.1;
            key.in_handle = (r[0].0 - t, r[0].1 - key.v);
            key.out_handle = (r[1].0 - t, r[1].1 - key.v);
            self.keys[index - 1].out_handle = (q[0].0 - a.t, q[0].1 - a.v);
            self.keys[index].in_handle = (q[2].0 - b.t, q[2].1 - b.v);
        }
        self.keys.insert(index, key);
        Some(index)
    }
    pub fn split_segment(&mut self, index: usize, t: f64) -> Option<usize> {
        let a = self.keys.get(index)?;
        let b = self.keys.get(index + 1)?;
        if t <= a.t || t >= b.t {
            return None;
        }
        self.insert_at(t)
    }
    /// Remove a segment's end key, joining its neighbours with the preceding mode.
    pub fn remove_segment(&mut self, index: usize) -> bool {
        self.remove(index.saturating_add(1))
    }
    /// Value at time `t` (held before the first and after the last key; 0 if empty).
    pub fn eval(&self, t: f64) -> f64 {
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else {
            return 0.0;
        };
        if t.is_nan() || t <= first.t {
            return first.v;
        }
        if t >= last.t {
            return last.v;
        }
        let i = self.keys.partition_point(|k| k.t <= t).saturating_sub(1);
        let (a, b) = (self.keys[i], self.keys[i + 1]);
        match a.interp {
            Interp::Constant => a.v,
            Interp::Linear => a.v + (b.v - a.v) * (t - a.t) / (b.t - a.t),
            Interp::Bezier => bezier_segment(a, b, t),
        }
    }
}

/// Evaluate the cubic bezier from `a` to `b` at time `t`. Handles are clamped into the
/// segment in time so the curve is a function of time (no loops), then x(s) = t is solved.
fn bezier_segment(a: Key, b: Key, t: f64) -> f64 {
    let span = b.t - a.t;
    let p0 = (a.t, a.v);
    let p1 = (a.t + a.out_handle.0.clamp(0.0, span), a.v + a.out_handle.1);
    let p2 = (b.t + b.in_handle.0.clamp(-span, 0.0), b.v + b.in_handle.1);
    let p3 = (b.t, b.v);
    let bez = |s: f64, c: [f64; 4]| {
        let u = 1.0 - s;
        u * u * u * c[0] + 3.0 * u * u * s * c[1] + 3.0 * u * s * s * c[2] + s * s * s * c[3]
    };
    let xs = [p0.0, p1.0, p2.0, p3.0];
    let ys = [p0.1, p1.1, p2.1, p3.1];
    // x(s) is monotonic with clamped handles: bisection always converges.
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, xs) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez((lo + hi) / 2.0, ys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_modes() {
        let mut c = Curve::default();
        assert_eq!(c.eval(1.0), 0.0, "empty curve");
        c.set(Key::new(0.0, 0.0, Interp::Linear));
        c.set(Key::new(2.0, 10.0, Interp::Constant));
        c.set(Key::new(4.0, 20.0, Interp::Linear));
        assert_eq!(c.eval(1.0), 5.0);
        assert_eq!(c.eval(3.0), 10.0, "constant holds");
        assert_eq!((c.eval(-1.0), c.eval(9.0)), (0.0, 20.0), "held outside");
        assert!(c.set(Key::new(f64::NAN, 1.0, Interp::Linear)).is_none());
    }

    #[test]
    fn bezier_hits_keys_and_respects_handles() {
        let mut c = Curve::default();
        let mut a = Key::new(0.0, 0.0, Interp::Bezier);
        a.out_handle = (0.5, 0.0);
        let mut b = Key::new(1.0, 1.0, Interp::Linear);
        b.in_handle = (-0.5, 0.0);
        c.set(a);
        c.set(b);
        assert!(c.eval(0.0).abs() < 1e-9 && (c.eval(1.0) - 1.0).abs() < 1e-9);
        assert!((c.eval(0.5) - 0.5).abs() < 1e-6, "symmetric ease: midpoint");
        assert!(c.eval(0.1) < 0.1, "eases in");
        // Wild handles beyond the segment are clamped: still a function of time.
        let mut w = c.clone();
        w.keys[0].out_handle = (50.0, 3.0);
        let mut prev = f64::NEG_INFINITY;
        for i in 0..=100 {
            let v = w.eval(i as f64 / 100.0);
            assert!(v.is_finite());
            prev = prev.max(v);
        }
    }

    #[test]
    fn moving_keys_keeps_order() {
        let mut c = Curve::default();
        for t in [0.0, 1.0, 2.0] {
            c.set(Key::new(t, t, Interp::Linear));
        }
        assert!(c.move_key(1, 5.0, 7.0));
        assert!(c.keys()[1].t < c.keys()[2].t, "cannot pass the next key");
        assert!(!c.move_key(9, 0.0, 0.0));
        assert_eq!(
            c.set(Key::new(2.0, 9.0, Interp::Linear)),
            Some(2),
            "same time replaces"
        );
        assert_eq!(c.keys().len(), 3);
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;

    #[test]
    fn crowded_keys_do_not_panic_on_move() {
        let mut c = Curve::default();
        for t in [0.0, 1e-7, 2e-7] {
            c.set(Key::new(t, 0.0, Interp::Linear));
        }
        assert!(c.move_key(1, 5.0, 1.0), "value still changes");
        assert_eq!(c.keys()[1].t, 1e-7);
    }

    #[test]
    fn eval_nan_is_finite() {
        let mut c = Curve::default();
        c.set(Key::new(0.0, 2.0, Interp::Linear));
        c.set(Key::new(1.0, 3.0, Interp::Linear));
        assert_eq!(c.eval(f64::NAN), 2.0);
    }
}

/// How moving one tangent influences its opposite handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TangentMode {
    #[default]
    Free,
    Aligned,
    Mirrored,
    Flat,
}
#[cfg(test)]
mod editing_tests {
    use super::*;
    #[test]
    fn splitting_preserves_a_cubic_and_modes_are_editable() {
        let mut c = Curve::default();
        let mut a = Key::new(0., 0., Interp::Bezier);
        a.out_handle = (0.5, 1.);
        let mut b = Key::new(1., 1., Interp::Linear);
        b.in_handle = (-0.2, -0.8);
        c.set(a);
        c.set(b);
        let before: Vec<_> = (0..100).map(|i| c.eval(i as f64 / 100.)).collect();
        assert_eq!(c.split_segment(0, 0.37), Some(1));
        for (i, v) in before.into_iter().enumerate() {
            assert!((c.eval(i as f64 / 100.) - v).abs() < 1e-9);
        }
        assert!(c.set_tangent(1, false, (0.1, 0.2), TangentMode::Mirrored));
        assert_eq!(c.keys()[1].in_handle, (-0.1, -0.2));
        assert!(c.set_interpolation(1, Interp::Constant));
        assert!(c.remove_segment(0));
        assert!(!c.remove(99));
    }
}
