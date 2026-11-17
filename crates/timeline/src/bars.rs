//! Quantized bar-curve model. Pointer-segment drawing fills every intervening bar.
use crate::{Curve, Interp, Key};
use std::rc::Rc;
#[derive(Clone, Debug, PartialEq)]
pub struct BarCurve {
    values: Rc<Vec<f64>>,
    min: f64,
    max: f64,
    steps: Option<usize>,
    pub bipolar: bool,
}
impl BarCurve {
    pub fn new(count: usize, min: f64, max: f64) -> Self {
        let (min, max) = if min.is_finite() && max.is_finite() && min < max {
            (min, max)
        } else {
            (0., 1.)
        };
        Self {
            values: Rc::new(vec![0_f64.clamp(min, max); count]),
            min,
            max,
            steps: None,
            bipolar: false,
        }
    }
    pub(crate) fn shared_values(&self) -> Rc<Vec<f64>> {
        self.values.clone()
    }
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    pub fn range(&self) -> (f64, f64) {
        (self.min, self.max)
    }
    pub fn set_steps(&mut self, steps: Option<usize>) {
        self.steps = steps.filter(|n| *n > 0);
        for i in 0..self.values.len() {
            let value = self.quantize(self.values[i]);
            Rc::make_mut(&mut self.values)[i] = value;
        }
    }
    fn quantize(&self, v: f64) -> f64 {
        let v = v.clamp(self.min, self.max);
        if let Some(n) = self.steps {
            let step = (self.max - self.min) / n as f64;
            (self.min + ((v - self.min) / step).round() * step).clamp(self.min, self.max)
        } else {
            v
        }
    }
    pub fn set(&mut self, index: usize, value: f64) -> bool {
        if !value.is_finite() || index >= self.values.len() {
            return false;
        }
        let v = self.quantize(value);
        if self.values[index] == v {
            return false;
        }
        Rc::make_mut(&mut self.values)[index] = v;
        true
    }
    /// Draw from fractional bar coordinate/value to another. Coordinate 0 is bar 0's
    /// centre; coordinates outside the model clamp to its ends. Returns changed span.
    pub fn draw(&mut self, from: (f64, f64), to: (f64, f64)) -> Option<std::ops::Range<usize>> {
        if self.values.is_empty() || ![from.0, from.1, to.0, to.1].iter().all(|v| v.is_finite()) {
            return None;
        }
        let max = (self.values.len() - 1) as f64;
        let a = from.0.round().clamp(0., max) as usize;
        let b = to.0.round().clamp(0., max) as usize;
        let mut changed = None;
        for i in a.min(b)..=a.max(b) {
            let f = if a == b {
                1.
            } else {
                (i as f64 - a as f64) / (b as f64 - a as f64)
            };
            let v = from.1 + (to.1 - from.1) * f;
            if self.set(i, v) {
                let span = changed.get_or_insert(i..i + 1);
                span.start = span.start.min(i);
                span.end = span.end.max(i + 1);
            }
        }
        changed
    }
    pub fn from_curve(
        curve: &Curve,
        count: usize,
        start: f64,
        end: f64,
        min: f64,
        max: f64,
    ) -> Self {
        let mut bars = Self::new(count, min, max);
        if !(start.is_finite() && end.is_finite() && end > start) {
            return bars;
        }
        for i in 0..count {
            let t = start + (end - start) * i as f64 / count.saturating_sub(1).max(1) as f64;
            bars.set(i, curve.eval(t));
        }
        bars
    }
    pub fn to_curve(&self, start: f64, end: f64) -> Curve {
        let mut curve = Curve::default();
        if !(start.is_finite() && end.is_finite() && end > start) {
            return curve;
        }
        for (i, v) in self.values.iter().enumerate() {
            curve.set(Key::new(
                start
                    + (end - start) * i as f64 / self.values.len().saturating_sub(1).max(1) as f64,
                *v,
                Interp::Linear,
            ));
        }
        curve
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_single_quantization_and_export() {
        assert!(BarCurve::new(0, 0., 1.).draw((0., 0.), (1., 1.)).is_none());
        let mut b = BarCurve::new(1, -1., 1.);
        b.set_steps(Some(4));
        assert!(b.set(0, 0.3));
        assert_eq!(b.values(), &[0.5]);
        assert!(!b.set(0, f64::NAN));
        let c = b.to_curve(0., 1.);
        assert_eq!(c.eval(0.), 0.5);
        assert_eq!(
            BarCurve::from_curve(&c, 1, 0., 1., -1., 1.).values(),
            b.values()
        );
    }
    #[test]
    fn generated_draw_paths_have_no_gaps_and_are_direction_invariant() {
        for n in 2..80 {
            for a in 0..n {
                let b = (a * 13 + 7) % n;
                let mut forward = BarCurve::new(n, -2., 2.);
                let mut reverse = forward.clone();
                forward.draw((a as f64, -1.), (b as f64, 1.));
                reverse.draw((b as f64, 1.), (a as f64, -1.));
                if a != b {
                    for i in a.min(b)..=a.max(b) {
                        let expected = -1. + 2. * (i as f64 - a as f64) / (b as f64 - a as f64);
                        assert!((forward.values()[i] - expected).abs() < 1e-12);
                        assert!((forward.values()[i] - reverse.values()[i]).abs() < 1e-12);
                    }
                }
            }
        }
    }
    #[test]
    fn fast_strokes_clamp_and_cover_every_bar() {
        let mut b = BarCurve::new(100, 0., 1.);
        assert_eq!(b.draw((-100., -1.), (200., 2.)), Some(34..100));
        assert_eq!(b.values()[0], 0.);
        assert_eq!(b.values()[99], 1.);
        assert!(b.draw((f64::NAN, 0.), (2., 1.)).is_none());
    }
}
