use guiraffe_timeline::{Axis, TimeFormat, axis};
use std::sync::Arc;

/// A nonempty, finite interval with a representable span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Domain {
    min: f64,
    max: f64,
}

impl Domain {
    pub const UNIT: Self = Self { min: 0., max: 1. };

    pub fn new(min: f64, max: f64) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min < max && (max - min).is_finite())
            .then_some(Self { min, max })
    }

    pub fn min(self) -> f64 {
        self.min
    }

    pub fn max(self) -> f64 {
        self.max
    }

    pub fn span(self) -> f64 {
        self.max - self.min
    }

    pub fn contains(self, value: f64) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// Scale configuration. Categories are encoded as zero-based indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Scale {
    #[default]
    Linear,
    Log,
    Time(TimeFormat),
    Band {
        labels: Arc<[String]>,
        padding: f64,
    },
    Ordinal(Arc<[String]>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub value: f64,
    pub label: String,
}

impl Scale {
    pub fn categorical(&self) -> bool {
        self.labels().is_some()
    }

    fn labels(&self) -> Option<&[String]> {
        match self {
            Self::Band { labels, .. } | Self::Ordinal(labels) => Some(labels),
            _ => None,
        }
    }

    pub fn accepts(&self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        if let Some(labels) = self.labels() {
            return value >= 0. && value.fract() == 0. && value < labels.len() as f64;
        }
        !matches!(self, Self::Log) || value > 0.
    }

    pub(crate) fn resolve(&self, min: f64, max: f64) -> Domain {
        if let Some(labels) = self.labels() {
            let last = labels.len().saturating_sub(1) as f64;
            return match self {
                Self::Band { .. } => Domain::new(-0.5, last + 0.5),
                _ if last == 0. => Domain::new(-0.5, 0.5),
                _ => Domain::new(0., last),
            }
            .unwrap_or(Domain::UNIT);
        }
        if !self.accepts(min) || !self.accepts(max) {
            return if matches!(self, Self::Log) {
                Domain { min: 1., max: 10. }
            } else {
                Domain::UNIT
            };
        }
        if min == max {
            let pad = if matches!(self, Self::Log) {
                min / 2.
            } else {
                (min.abs() / 20.).max(0.5)
            };
            return Domain::new(min - pad, max + pad)
                .filter(|domain| self.accepts(domain.min))
                .or_else(|| Domain::new(min - pad, max))
                .or_else(|| Domain::new(min, max + pad))
                .unwrap_or(Domain::UNIT);
        }
        Domain::new(min, max).unwrap_or(Domain::UNIT)
    }

    /// Map to a fraction of the viewport. Values outside the domain can map beyond 0..=1.
    pub fn project(&self, domain: Domain, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        let fraction = if matches!(self, Self::Log) {
            (value.ln() - domain.min.ln()) / (domain.max.ln() - domain.min.ln())
        } else {
            (value - domain.min) / domain.span()
        };
        fraction.is_finite().then_some(fraction)
    }

    pub fn invert(&self, domain: Domain, fraction: f64) -> f64 {
        if matches!(self, Self::Log) {
            (domain.min.ln() + fraction * (domain.max.ln() - domain.min.ln())).exp()
        } else {
            domain.min + fraction * domain.span()
        }
    }

    /// Fraction of the viewport occupied by one category band.
    pub fn band_fraction(&self, domain: Domain) -> f64 {
        match self {
            Self::Band { padding, .. } if self.categorical() => {
                (1. - padding.clamp(0., 0.95)) / domain.span()
            }
            Self::Ordinal(_) => 0.8 / domain.span(),
            _ => 0.,
        }
    }

    pub fn format(&self, value: f64) -> String {
        match self {
            Self::Time(format)
                if value.abs() < 1e15
                    && (!matches!(format, TimeFormat::Days) || (value / 86_400.).fract() == 0.) =>
            {
                axis::label(value, *format)
            }
            Self::Time(TimeFormat::Days) if value.abs() < 1e15 => {
                let date = axis::label(value, TimeFormat::Days);
                let hour = value.rem_euclid(86_400.) / 3600.;
                format!(
                    "{date} {:02}:{:02}",
                    hour.floor() as u32,
                    (hour.fract() * 60.).round() as u32
                )
            }
            _ => self
                .labels()
                .filter(|_| self.accepts(value))
                .and_then(|labels| labels.get(value as usize))
                .cloned()
                .unwrap_or_else(|| number(value)),
        }
    }

    fn step(&self, domain: Domain, target: usize) -> f64 {
        let target = target.clamp(1, 256) as f64;
        if let Self::Time(format) = self {
            // Set an explicit scale so timeline editor zoom constraints do not clip the domain.
            let mut axis = Axis::new(domain.min, domain.max, target);
            axis.scale = domain.span() / target;
            return axis.tick_step(1., *format);
        }
        nice_step(domain.span() / target)
    }

    pub fn nice(&self, domain: Domain, target: usize) -> Domain {
        if self.categorical() {
            return domain;
        }
        let (min, max) = if matches!(self, Self::Log) {
            (
                10f64.powf(domain.min.log10().floor()),
                10f64.powf(domain.max.log10().ceil()),
            )
        } else {
            let step = self.step(domain, target);
            (
                (domain.min / step).floor() * step,
                (domain.max / step).ceil() * step,
            )
        };
        Domain::new(min, max)
            .filter(|domain| self.accepts(domain.min))
            .unwrap_or(domain)
    }

    pub fn ticks(&self, domain: Domain, target: usize) -> Vec<Tick> {
        let target = target.clamp(1, 256);
        let values: Vec<_> = if let Some(labels) = self.labels() {
            let first = (domain.min.ceil() as usize).min(labels.len());
            let end = (domain.max.floor() as usize)
                .saturating_add(1)
                .min(labels.len());
            (first..end)
                .step_by(end.saturating_sub(first).div_ceil(target).max(1))
                .map(|index| index as f64)
                .collect()
        } else if matches!(self, Self::Log) {
            let low = domain.min.log10();
            let high = domain.max.log10();
            if low.is_finite() && high.is_finite() {
                if high - low < 1. {
                    multiples(
                        domain.min,
                        domain.max,
                        nice_step(domain.span() / target as f64),
                    )
                } else {
                    let step = ((high - low) / target as f64).ceil().max(1.);
                    multiples(low, high, step)
                        .into_iter()
                        .map(|power| 10f64.powf(power))
                        .collect()
                }
            } else {
                Vec::new()
            }
        } else if let Self::Time(format) = self {
            let mut axis = Axis::new(domain.min, domain.max, target as f64);
            axis.scale = domain.span() / target as f64;
            axis.major_ticks(1., *format)
        } else {
            multiples(domain.min, domain.max, self.step(domain, target))
        };
        values
            .into_iter()
            .filter(|&value| domain.contains(value))
            .map(|value| Tick {
                value,
                label: self.format(value),
            })
            .collect()
    }
}

fn nice_step(value: f64) -> f64 {
    let base = 10f64.powf(value.log10().floor());
    [1., 2., 5., 10.]
        .into_iter()
        .map(|multiple| multiple * base)
        .find(|step| *step >= value && *step > 0.)
        .unwrap_or(value)
}

fn multiples(min: f64, max: f64, step: f64) -> Vec<f64> {
    let first = (min / step).ceil();
    let last = (max / step).floor();
    let count = last - first + 1.;
    if !first.is_finite() || !(1. ..=512.).contains(&count) {
        return Vec::new();
    }
    (0..count as usize)
        .map(|index| (first + index as f64) * step)
        .collect()
}

fn number(value: f64) -> String {
    let absolute = value.abs();
    if absolute != 0. && !(0.0001..1e7).contains(&absolute) {
        format!("{value:.3e}")
    } else {
        format!("{value:.5}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn narrow_log_domains_use_round_ticks() {
        let ticks = Scale::Log.ticks(Domain::new(21., 46.).unwrap(), 5);
        assert_eq!(
            ticks.into_iter().map(|tick| tick.value).collect::<Vec<_>>(),
            [25., 30., 35., 40., 45.]
        );
    }

    proptest! {
        #[test]
        fn continuous_scales_round_trip_and_bound_ticks(
            base in -12f64..12.,
            span in 0.01f64..12.,
            fraction in 0f64..1.,
            count in 1usize..40
        ) {
            for scale in [Scale::Linear, Scale::Log, Scale::Time(TimeFormat::Seconds)] {
                let (min, max) = if matches!(scale, Scale::Log) {
                    (10f64.powf(base), 10f64.powf(base + span))
                } else {
                    (base, base + span)
                };
                let domain = Domain::new(min, max).ok_or_else(|| TestCaseError::fail("finite test domain"))?;
                let value = scale.invert(domain, fraction);
                prop_assert!((scale.project(domain, value).ok_or_else(|| TestCaseError::fail("invertible test value"))? - fraction).abs() < 1e-9);
                let ticks = scale.ticks(domain, count);
                prop_assert!(ticks.windows(2).all(|pair| pair[0].value < pair[1].value));
                prop_assert!(ticks.iter().all(|tick| domain.contains(tick.value)));
                let nice = scale.nice(domain, count);
                prop_assert!(nice.min() <= min + min.abs() * 1e-12);
                prop_assert!(nice.max() >= max - max.abs() * 1e-12);
            }
        }
    }

    #[test]
    fn categorical_centers_and_log_ticks() {
        let labels = Arc::from(["A".into(), "B".into()]);
        let scale = Scale::Band {
            labels,
            padding: 0.2,
        };
        let domain = scale.resolve(0., 1.);
        assert_eq!(scale.project(domain, 0.), Some(0.25));
        assert_eq!(scale.project(domain, 1.), Some(0.75));
        assert_eq!(scale.ticks(domain, 5)[1].label, "B");
        assert!(!scale.accepts(1.5));
        let domain = Domain {
            min: 1.,
            max: 1000.,
        };
        assert_eq!(
            Scale::Log
                .ticks(domain, 8)
                .iter()
                .map(|t| t.value)
                .collect::<Vec<_>>(),
            vec![1., 10., 100., 1000.]
        );
    }
}
