//! The shared time axis: a view window over time (seconds as `f64`), zoom about a point,
//! pan, and "nice" ruler ticks. Every timeline editor (Gantt, tracks, dope sheet, curves)
//! uses this one model, so they zoom, scroll and snap identically.

/// How ruler labels read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeFormat {
    /// `1:02.5` style.
    Seconds,
    /// Frame numbers at a frame rate.
    Frames(f64),
    /// Calendar days (time in seconds since the epoch, UTC).
    Days,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    /// Time at the left edge of the view.
    pub start: f64,
    /// Seconds per pixel.
    pub scale: f64,
    /// View width in pixels.
    pub width: f64,
    pub min_scale: f64,
    pub max_scale: f64,
}

impl Axis {
    pub fn new(start: f64, end: f64, width: f64) -> Axis {
        let width = if width.is_finite() && width > 0.0 {
            width
        } else {
            1.0
        };
        let span = if end.is_finite() && start.is_finite() && end > start {
            end - start
        } else {
            1.0
        };
        let mut a = Axis {
            start: if start.is_finite() { start } else { 0.0 },
            scale: span / width,
            width,
            min_scale: 1e-6,
            max_scale: 1e9,
        };
        a.scale = a.scale.clamp(a.min_scale, a.max_scale);
        a
    }

    pub fn end(&self) -> f64 {
        self.start + self.width * self.scale
    }

    pub fn x(&self, t: f64) -> f64 {
        (t - self.start) / self.scale
    }

    pub fn t(&self, x: f64) -> f64 {
        self.start + x * self.scale
    }

    /// Zoom by `factor` (> 1 zooms in) keeping the time under pixel `x` fixed.
    pub fn zoom_at(&mut self, x: f64, factor: f64) -> bool {
        if !factor.is_finite() || factor <= 0.0 || !x.is_finite() {
            return false;
        }
        let anchor = self.t(x);
        let scale = (self.scale / factor).clamp(self.min_scale, self.max_scale);
        if scale == self.scale {
            return false;
        }
        self.scale = scale;
        self.start = anchor - x * scale;
        true
    }

    pub fn pan_px(&mut self, dx: f64) {
        if dx.is_finite() {
            self.start += dx * self.scale;
        }
    }

    pub fn set_width(&mut self, width: f64) {
        if width.is_finite() && width > 0.0 {
            self.width = width;
        }
    }

    /// Fit `[a, b]` with `margin` fraction of padding on each side.
    pub fn fit(&mut self, a: f64, b: f64, margin: f64) {
        if !(a.is_finite() && b.is_finite()) || b <= a {
            return;
        }
        let pad = (b - a) * margin.clamp(0.0, 0.45);
        self.scale = ((b - a + 2.0 * pad) / self.width).clamp(self.min_scale, self.max_scale);
        self.start = a - pad;
    }

    /// Major tick step (seconds) giving ticks roughly every `min_px` pixels, chosen from a
    /// "nice" ladder for the format (1/2/5 decades; frames; minutes/hours; days/weeks).
    pub fn tick_step(&self, min_px: f64, format: TimeFormat) -> f64 {
        let target = (min_px.max(1.0) * self.scale).max(f64::MIN_POSITIVE);
        let nice_125 = |t: f64| {
            let p = 10f64.powf(t.log10().floor());
            [1.0, 2.0, 5.0, 10.0]
                .iter()
                .map(|m| m * p)
                .find(|s| *s >= t)
                .unwrap_or(10.0 * p)
        };
        match format {
            TimeFormat::Frames(fps) if fps > 0.0 => {
                let frames = (target * fps).max(1.0);
                nice_125(frames) / fps
            }
            TimeFormat::Days => {
                const DAY: f64 = 86_400.0;
                [
                    3600.0,
                    6.0 * 3600.0,
                    DAY,
                    7.0 * DAY,
                    30.0 * DAY,
                    365.0 * DAY,
                ]
                .into_iter()
                .find(|s| *s >= target)
                .unwrap_or_else(|| nice_125(target / (365.0 * DAY)) * 365.0 * DAY)
            }
            _ => {
                if target >= 60.0 {
                    [60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0]
                        .into_iter()
                        .find(|s| *s >= target)
                        .unwrap_or_else(|| nice_125(target / 3600.0) * 3600.0)
                } else {
                    nice_125(target)
                }
            }
        }
    }

    /// Tick times within the view for `step` (aligned to multiples of the step).
    pub fn ticks(&self, step: f64) -> Vec<f64> {
        if !(step.is_finite() && step > 0.0) {
            return Vec::new();
        }
        let (first, last) = ((self.start / step).ceil(), (self.end() / step).floor());
        if !(first.is_finite() && last.is_finite()) || last - first > 10_000.0 {
            return Vec::new();
        }
        let (first, last) = (first as i64, last as i64);
        (first..=last).map(|i| i as f64 * step).collect()
    }
}

/// Snap `t` to the nearest of `targets` or grid multiples of `grid`, if within `threshold`
/// (seconds). Returns the snapped time and whether it snapped.
pub fn snap(t: f64, grid: Option<f64>, targets: &[f64], threshold: f64) -> (f64, bool) {
    let mut best = (t, f64::INFINITY);
    if let Some(g) = grid.filter(|g| g.is_finite() && *g > 0.0) {
        let c = (t / g).round() * g;
        best = (c, (c - t).abs());
    }
    for &c in targets {
        let d = (c - t).abs();
        if d < best.1 {
            best = (c, d);
        }
    }
    if best.1 <= threshold {
        (best.0, true)
    } else {
        (t, false)
    }
}

/// `1:02.500`-style label for seconds.
pub fn label(t: f64, format: TimeFormat) -> String {
    match format {
        TimeFormat::Frames(fps) if fps > 0.0 => format!("{}", (t * fps).round() as i64),
        TimeFormat::Days => {
            let days = (t / 86_400.0).floor() as i64;
            let (y, m, d) = civil_from_days(days);
            format!("{y:04}-{m:02}-{d:02}")
        }
        _ => {
            let neg = t < 0.0;
            let t = (t.abs() * 1000.0).round() / 1000.0; // ms precision: 59.9996 -> 1:00
            let m = (t / 60.0).floor();
            let s = t - m * 60.0;
            let body = if s.fract().abs() < 1e-9 {
                format!("{}:{:02}", m as i64, s as i64)
            } else {
                format!("{}:{:06.3}", m as i64, s)
            };
            if neg { format!("-{body}") } else { body }
        }
    }
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_round_trips_and_zoom_keeps_anchor() {
        let mut a = Axis::new(10.0, 20.0, 1000.0);
        assert_eq!(a.x(10.0), 0.0);
        assert!((a.t(a.x(13.7)) - 13.7).abs() < 1e-9);
        let before = a.t(250.0);
        assert!(a.zoom_at(250.0, 4.0));
        assert!((a.t(250.0) - before).abs() < 1e-9, "anchor stays put");
        assert!(!a.zoom_at(250.0, 0.0) && !a.zoom_at(f64::NAN, 2.0));
        a.pan_px(100.0);
        assert!((a.t(250.0) - before - 100.0 * a.scale).abs() < 1e-9);
    }

    #[test]
    fn zoom_clamps_and_degenerate_inputs_are_safe() {
        let mut a = Axis::new(0.0, 1.0, 100.0);
        for _ in 0..200 {
            a.zoom_at(50.0, 10.0);
        }
        assert_eq!(a.scale, a.min_scale);
        let bad = Axis::new(f64::NAN, -5.0, 0.0);
        assert!(bad.scale.is_finite() && bad.width > 0.0 && bad.start.is_finite());
        let mut f = Axis::new(0.0, 1.0, 1000.0);
        f.fit(100.0, 200.0, 0.1);
        assert!((f.start - 90.0).abs() < 1e-9 && (f.end() - 210.0).abs() < 1e-6);
    }

    #[test]
    fn ticks_are_nice_and_aligned() {
        let a = Axis::new(0.0, 10.0, 1000.0); // 0.01 s/px
        let step = a.tick_step(80.0, TimeFormat::Seconds);
        assert_eq!(step, 1.0);
        assert_eq!(a.ticks(step), (0..=10).map(f64::from).collect::<Vec<_>>());
        let f = Axis::new(0.0, 10.0, 1000.0);
        let fs = f.tick_step(60.0, TimeFormat::Frames(24.0));
        assert!(
            ((fs * 24.0) - (fs * 24.0).round()).abs() < 1e-9,
            "whole frames"
        );
        let d = Axis::new(0.0, 90.0 * 86_400.0, 1000.0);
        assert_eq!(
            d.tick_step(60.0, TimeFormat::Days),
            7.0 * 86_400.0,
            "weekly ticks over a quarter"
        );
        let m = Axis::new(0.0, 7200.0, 1000.0);
        assert_eq!(
            m.tick_step(80.0, TimeFormat::Seconds),
            600.0,
            "10-minute ticks over two hours"
        );
        assert!(
            a.ticks(f64::NAN).is_empty() && a.ticks(1e-9).is_empty(),
            "pathological steps produce no ticks"
        );
    }

    #[test]
    fn snapping_prefers_the_closest_target_within_threshold() {
        assert_eq!(snap(1.04, Some(0.5), &[], 0.1), (1.0, true));
        assert_eq!(
            snap(1.04, Some(0.5), &[1.05], 0.1),
            (1.05, true),
            "explicit target closer than grid"
        );
        assert_eq!(snap(1.3, Some(0.5), &[], 0.1), (1.3, false));
        assert_eq!(snap(2.0, None, &[], 1.0), (2.0, false));
    }

    #[test]
    fn labels() {
        assert_eq!(label(62.5, TimeFormat::Seconds), "1:02.500");
        assert_eq!(label(60.0, TimeFormat::Seconds), "1:00");
        assert_eq!(label(-1.0, TimeFormat::Seconds), "-0:01");
        assert_eq!(label(2.0, TimeFormat::Frames(30.0)), "60");
        assert_eq!(label(0.0, TimeFormat::Days), "1970-01-01");
        assert_eq!(label(19_723.0 * 86_400.0, TimeFormat::Days), "2024-01-01");
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;

    #[test]
    fn tiny_step_on_huge_range_returns_empty() {
        assert!(Axis::new(-1e10, 1e10, 1000.).ticks(1e-9).is_empty());
    }

    #[test]
    fn labels_round_to_ms() {
        assert_eq!(label(59.9996, TimeFormat::Seconds), "1:00");
    }
}
