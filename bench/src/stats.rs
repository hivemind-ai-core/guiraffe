//! Deterministic percentile bootstrap statistics. Seeds are recorded by the harness.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Percentile confidence interval, in the same units as the estimator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
}

/// Median, unscaled median absolute deviation, and 95% percentile bootstrap CI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub median: f64,
    pub mad: f64,
    pub ci95: Interval,
}

fn validate(values: &[f64]) -> Result<()> {
    ensure!(!values.is_empty(), "samples must not be empty");
    ensure!(
        values.iter().all(|v| v.is_finite()),
        "samples must be finite"
    );
    Ok(())
}

fn sorted_median(values: &[f64]) -> f64 {
    let mid = values.len() / 2;
    match (values.get(mid), values.get(mid.saturating_sub(1))) {
        (Some(upper), Some(lower)) if values.len().is_multiple_of(2) => {
            if lower.signum() == upper.signum() {
                lower + (upper - lower) / 2.0
            } else {
                lower / 2.0 + upper / 2.0
            }
        }
        (Some(value), _) => *value,
        _ => 0.0, // Private helper: callers validate nonempty inputs.
    }
}

/// Median of finite, nonempty samples, without modifying the input.
pub fn median(values: &[f64]) -> Result<f64> {
    validate(values)?;
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Ok(sorted_median(&sorted))
}

struct Rng(u64);
impl Rng {
    fn index(&mut self, len: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        // Multiply-high avoids modulo bias for the small sample counts used here.
        ((self.0 as u128 * len as u128) >> 64) as usize
    }
}

fn bootstrap(
    len: usize,
    resamples: usize,
    seed: u64,
    mut estimator: impl FnMut(&[usize]) -> Result<f64>,
) -> Result<Interval> {
    ensure!(
        resamples >= 100,
        "at least 100 bootstrap resamples required"
    );
    let mut rng = Rng(if seed == 0 { 1 } else { seed });
    let mut indices = vec![0; len];
    let mut estimates = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        for index in &mut indices {
            *index = rng.index(len);
        }
        estimates.push(estimator(&indices)?);
    }
    estimates.sort_by(f64::total_cmp);
    let low = estimates.get((resamples - 1) * 25 / 1000);
    let high = estimates.get((resamples - 1) * 975 / 1000);
    match (low, high) {
        (Some(low), Some(high)) => Ok(Interval {
            low: *low,
            high: *high,
        }),
        _ => anyhow::bail!("bootstrap produced no estimates"),
    }
}

/// Compute summary statistics using a reproducible percentile bootstrap.
pub fn summarize(values: &[f64], resamples: usize, seed: u64) -> Result<Summary> {
    let center = median(values)?;
    let deviations: Vec<_> = values.iter().map(|v| (v - center).abs()).collect();
    let mad = median(&deviations)?;
    let ci95 = bootstrap(values.len(), resamples, seed, |indices| {
        let sample: Vec<_> = indices
            .iter()
            .filter_map(|i| values.get(*i).copied())
            .collect();
        median(&sample)
    })?;
    Ok(Summary {
        median: center,
        mad,
        ci95,
    })
}

/// Paired bootstrap CI for the relative change in medians (0.10 means +10%).
/// Each index is one interleaved A/B round; resampling preserves that pairing.
pub fn relative_change(old: &[f64], new: &[f64], resamples: usize, seed: u64) -> Result<Interval> {
    validate(old)?;
    validate(new)?;
    ensure!(
        old.len() == new.len(),
        "paired samples must have equal lengths"
    );
    ensure!(
        old.iter().chain(new).all(|v| *v > 0.0),
        "timings must be positive"
    );
    bootstrap(old.len(), resamples, seed, |indices| {
        let a: Vec<_> = indices
            .iter()
            .filter_map(|i| old.get(*i).copied())
            .collect();
        let b: Vec<_> = indices
            .iter()
            .filter_map(|i| new.get(*i).copied())
            .collect();
        let change = median(&b)? / median(&a)? - 1.0;
        ensure!(change.is_finite(), "relative change overflow");
        Ok(change)
    })
}

/// A slowdown is established only when the entire CI lies beyond the threshold.
pub fn regressed(ci: Interval, threshold: f64) -> bool {
    ci.low > threshold
}

/// Statistical classification, using symmetric slowdown and speedup thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Regression,
    Improvement,
    Uncertain,
}
impl Change {
    /// Human-readable comparison table status.
    pub fn label(self) -> &'static str {
        match self {
            Self::Regression => "REGRESSION",
            Self::Improvement => "improvement",
            Self::Uncertain => "pass",
        }
    }
}
/// Classify only changes whose entire confidence interval exceeds a threshold.
pub fn classify(ci: Interval, threshold: f64) -> Change {
    if regressed(ci, threshold) {
        Change::Regression
    } else if ci.high < -threshold {
        Change::Improvement
    } else {
        Change::Uncertain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medians_cover_odd_even_single_unsorted_negative() -> Result<()> {
        for (values, expected) in [
            (vec![3., 1., 2.], 2.),
            (vec![4., 1., 3., 2.], 2.5),
            (vec![7.], 7.),
            (vec![-9., -3., -6.], -6.),
            (vec![f64::MAX, f64::MAX], f64::MAX),
        ] {
            let original = values.clone();
            assert_eq!(median(&values)?, expected);
            assert_eq!(values, original);
        }
        Ok(())
    }

    #[test]
    fn invalid_inputs_rejected() {
        for values in [
            vec![],
            vec![f64::NAN],
            vec![f64::INFINITY],
            vec![f64::NEG_INFINITY],
        ] {
            assert!(median(&values).is_err());
            assert!(summarize(&values, 100, 1).is_err());
        }
        assert!(summarize(&[1.], 99, 1).is_err());
        assert!(relative_change(&[1.], &[1., 2.], 100, 1).is_err());
        assert!(relative_change(&[0.], &[1.], 100, 1).is_err());
        assert!(relative_change(&[1.], &[-1.], 100, 1).is_err());
        assert!(relative_change(&[f64::MIN_POSITIVE], &[f64::MAX], 100, 1).is_err());
    }

    #[test]
    fn mad_is_unscaled_and_robust_to_outlier() -> Result<()> {
        let result = summarize(&[1., 2., 3., 4., 1000.], 2000, 42)?;
        assert_eq!(result.median, 3.);
        assert_eq!(result.mad, 1.);
        assert!(result.ci95.low <= result.median && result.ci95.high >= result.median);
        Ok(())
    }

    #[test]
    fn constants_and_singleton_have_exact_intervals() -> Result<()> {
        for values in [vec![5.], vec![5.; 20]] {
            assert_eq!(
                summarize(&values, 100, 0)?,
                Summary {
                    median: 5.,
                    mad: 0.,
                    ci95: Interval { low: 5., high: 5. }
                }
            );
        }
        Ok(())
    }

    #[test]
    fn bootstrap_is_reproducible_and_scale_equivariant() -> Result<()> {
        let data = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10.];
        let result = summarize(&data, 2000, 17)?;
        assert_eq!(result, summarize(&data, 2000, 17)?);
        let scaled = summarize(&data.map(|v| v * 2.), 2000, 17)?;
        assert_eq!(scaled.median, result.median * 2.);
        assert_eq!(scaled.mad, result.mad * 2.);
        assert_eq!(scaled.ci95.low, result.ci95.low * 2.);
        assert_eq!(scaled.ci95.high, result.ci95.high * 2.);
        Ok(())
    }

    #[test]
    fn paired_ratios_preserve_common_host_noise() -> Result<()> {
        let old = [1., 100., 4., 7., 9., 22., 5., 3., 2., 30.];
        let new = old.map(|v| v * 1.25);
        let ci = relative_change(&old, &new, 2000, 42)?;
        assert!((ci.low - 0.25).abs() < 1e-12);
        assert!((ci.high - 0.25).abs() < 1e-12);
        assert!(regressed(ci, 0.10));
        assert!(!regressed(ci, 0.30));
        Ok(())
    }

    #[test]
    fn uncertain_and_boundary_results_do_not_fail() -> Result<()> {
        assert!(!regressed(
            Interval {
                low: 0.1,
                high: 0.5
            },
            0.1
        ));
        assert!(!regressed(
            Interval {
                low: -0.2,
                high: 0.8
            },
            0.1
        ));
        assert!(!regressed(
            relative_change(&[10.; 10], &[9.; 10], 1000, 1)?,
            0.02
        ));
        assert!(regressed(
            relative_change(&[100.; 10], &[103.; 10], 1000, 1)?,
            0.02
        ));
        Ok(())
    }
    #[test]
    fn even_medians_handle_extreme_and_subnormal_values() -> Result<()> {
        let tiny = f64::from_bits(1);
        assert_eq!(median(&[tiny, tiny])?, tiny);
        assert_eq!(median(&[-f64::MAX, f64::MAX])?, 0.);
        assert_eq!(median(&[-f64::MAX, -f64::MAX])?, -f64::MAX);
        Ok(())
    }

    #[test]
    fn bootstrap_percentiles_on_two_points_are_exact() -> Result<()> {
        let result = summarize(&[1., 3.], 10_000, 42)?;
        assert_eq!(result.median, 2.);
        assert_eq!(result.mad, 1.);
        assert_eq!(result.ci95, Interval { low: 1., high: 3. });
        Ok(())
    }

    #[test]
    fn translation_preserves_spread_and_moves_interval() -> Result<()> {
        let data = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10.];
        let result = summarize(&data, 1000, 31)?;
        let shifted = summarize(&data.map(|v| v + 100.), 1000, 31)?;
        assert_eq!(shifted.median, result.median + 100.);
        assert_eq!(shifted.mad, result.mad);
        assert_eq!(shifted.ci95.low, result.ci95.low + 100.);
        assert_eq!(shifted.ci95.high, result.ci95.high + 100.);
        Ok(())
    }

    #[test]
    fn nonrepresentable_deviations_fail_instead_of_emitting_infinity() {
        assert!(summarize(&[-f64::MAX, f64::MAX, f64::MAX], 100, 1).is_err());
    }
    #[test]
    fn improvements_require_entire_interval_below_negative_threshold() {
        assert_eq!(
            classify(
                Interval {
                    low: -0.5,
                    high: -0.2
                },
                0.1
            ),
            Change::Improvement
        );
        assert_eq!(
            classify(
                Interval {
                    low: -0.5,
                    high: -0.1
                },
                0.1
            ),
            Change::Uncertain
        );
        assert_eq!(
            classify(
                Interval {
                    low: -0.5,
                    high: 0.1
                },
                0.1
            ),
            Change::Uncertain
        );
        assert_eq!(
            classify(
                Interval {
                    low: 0.2,
                    high: 0.5
                },
                0.1
            ),
            Change::Regression
        );
        assert_eq!(Change::Improvement.label(), "improvement");
    }
}
