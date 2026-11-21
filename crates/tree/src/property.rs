#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
    Enum(String),
    Colour(u32),
    Vector([f64; 2]),
    Rect([f64; 4]),
    Flags(Vec<String>),
    Path(String),
}

impl Value {
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Text(s) | Self::Enum(s) | Self::Path(s) => s.clone(),
            Self::Number(n) => n.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Colour(rgb) => format!("#{rgb:06x}"),
            Self::Vector(values) => format_components(values),
            Self::Rect(values) => format_components(values),
            Self::Flags(flags) => flags.join(", "),
        }
    }

    pub(crate) fn parse(&self, text: &str) -> Option<Self> {
        match self {
            Self::Text(_) => Some(Self::Text(text.to_owned())),
            Self::Number(_) => text
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(Self::Number),
            Self::Colour(_) => {
                let digits = text.strip_prefix('#')?;
                (digits.len() == 6)
                    .then(|| u32::from_str_radix(digits, 16).ok())
                    .flatten()
                    .map(Self::Colour)
            }
            Self::Bool(_) => text.parse().ok().map(Self::Bool),
            Self::Enum(_) => Some(Self::Enum(text.to_owned())),
            Self::Vector(_) => parse_components(text).map(Self::Vector),
            Self::Rect(_) => parse_components(text).map(Self::Rect),
            Self::Flags(_) => Some(Self::Flags(
                text.split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect(),
            )),
            Self::Path(_) => Some(Self::Path(text.to_owned())),
        }
    }
}

fn format_components(values: &[f64]) -> String {
    values
        .iter()
        .map(f64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_components<const N: usize>(text: &str) -> Option<[f64; N]> {
    let mut parts = text.split(',');
    let mut values = [0.; N];
    for value in &mut values {
        let number: f64 = parts.next()?.trim().parse().ok()?;
        if !number.is_finite() {
            return None;
        }
        *value = number;
    }
    parts.next().is_none().then_some(values)
}

/// Optional presentation and constraints for a property.
#[derive(Clone, Copy, Debug, Default)]
pub struct PropertyOptions<'a> {
    pub description: Option<&'a str>,
    pub units: Option<&'a str>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub mixed: bool,
    pub advanced: bool,
    /// Maximum rows for a growing text editor, up to eight.
    pub multiline_rows: Option<usize>,
}

impl PropertyOptions<'_> {
    pub(crate) fn slider_range(self) -> Option<(f32, f32, f32)> {
        let (min, max) = (self.min? as f32, self.max? as f32);
        if self.mixed || !min.is_finite() || !max.is_finite() || min >= max {
            return None;
        }
        let span = max - min;
        let step = self.step.map_or(span / 100., |step| step as f32);
        (step.is_finite() && step > 0.).then_some((min, max, step))
    }
}

pub struct Property<'a> {
    pub value: &'a Value,
    pub default: &'a Value,
    pub choices: &'a [String],
    pub read_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_values_round_trip_and_validate_arity() {
        for value in [Value::Vector([1., -2.5]), Value::Rect([1., 2., 3., 4.])] {
            assert_eq!(value.parse(&value.text()), Some(value.clone()));
            assert_eq!(value.parse("1, 2, NaN, 4"), None);
            assert_eq!(value.parse("1"), None);
        }
    }

    #[test]
    fn slider_range_rejects_invalid_bounds_and_steps() {
        let range = PropertyOptions {
            min: Some(0.),
            max: Some(100.),
            step: Some(2.),
            ..Default::default()
        };
        assert_eq!(range.slider_range(), Some((0., 100., 2.)));
        for step in [0., -1., f64::INFINITY] {
            assert_eq!(
                PropertyOptions {
                    step: Some(step),
                    ..range
                }
                .slider_range(),
                None
            );
        }
        assert_eq!(
            PropertyOptions {
                mixed: true,
                ..range
            }
            .slider_range(),
            None
        );
    }
}
