//! Value mapping shared by `SliderControl`, `KnobControl` and `NumberFieldControl`: a range, a
//! curve between the value and a control's travel, a step, a default, and the unit that
//! formats a value for display and parses typed entry. Plain data, so a 1 kHz cutoff scrubs,
//! steps, reads and parses the same way in every control.
//!
//! ```
//! use guiraffe_components::mapping::{Curve, Unit, ValueMap, ValueMapping};
//! let cutoff = ValueMap::new(20.0, 20_000.0).curve(Curve::Log).unit(Unit::Hertz).default_to(1_000.0);
//! assert_eq!(cutoff.format(1_250.0), "1.25 kHz");
//! assert_eq!(cutoff.parse("440"), Ok(440.0));
//! assert_eq!(cutoff.parse("2.5k"), Ok(2_500.0));
//! // Halfway along a log knob is the geometric middle of the range.
//! assert!((cutoff.value_at(0.5) - 632.46).abs() < 0.01);
//! ```

use std::fmt;

/// How a value maps onto a control's travel, normalised to `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// Equal travel, equal change.
    Linear,
    /// Equal travel, equal ratio (frequency, time). Needs a positive range; a range that
    /// touches zero falls back to linear.
    Log,
    /// `value = min + (max - min) · travel^k`: `k > 1` spends more travel near `min`.
    Exp(f64),
}

/// What a value measures: how it is formatted, and which typed suffixes parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// A bare number.
    None,
    /// Hertz: `440 Hz`, `1.25 kHz`. Parses `Hz`, `kHz` and a `k` suffix.
    Hertz,
    /// Seconds: `250 ms`, `1.50 s`. Parses `ms` and `s`; a bare number is seconds.
    Seconds,
    /// Decibels: `-6.0 dB`, `+3.0 dB`, and `-inf dB` at or below `min` when `min <= -96`.
    Decibels,
    /// A `0..=1` fraction shown as a percentage: `50%`. Parses `50%` and `0.5`.
    Percent,
    /// Semitones: `+7 st`, `-12 st`.
    Semitones,
    /// A MIDI note number shown as a name: `60` is `C4`. Parses `C4`, `F#3`, `Db5` and numbers.
    Note,
}

/// Why typed entry could not be read as a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    NotANumber,
    UnknownUnit,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "enter a value",
            Self::NotANumber => "not a number",
            Self::UnknownUnit => "unknown unit",
        })
    }
}

impl std::error::Error for ParseError {}

/// Maps between a value, a control's normalised travel, and text.
///
/// [`ValueMap`] implements it as plain data. Implement it yourself for a custom scale (a
/// table of detents, a tempo-synced rate) and the controls take it through
/// `.mapping(…)`.
pub trait ValueMapping: 'static {
    fn min(&self) -> f64;
    fn max(&self) -> f64;
    /// The value double-click and reset return to.
    fn default_value(&self) -> f64;
    /// Travel `0..=1` for a value (clamped).
    fn to_normal(&self, value: f64) -> f64;
    /// The value at travel `0..=1` (clamped, snapped to the step).
    fn value_at(&self, normal: f64) -> f64;
    /// One keyboard or bus step up (`fine`: a tenth of it) from `value`.
    fn step_from(&self, value: f64, up: bool, fine: bool) -> f64;
    /// Clamp and snap a value written from outside.
    fn constrain(&self, value: f64) -> f64;
    fn format(&self, value: f64) -> String;
    fn parse(&self, text: &str) -> Result<f64, ParseError>;
    /// Whether the control is centred on a middle value (pan, detune): the value arc grows
    /// from the centre and drags detent there.
    fn bipolar(&self) -> bool {
        false
    }
}

/// A range with a curve, step, default, unit and display precision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueMap {
    min: f64,
    max: f64,
    curve: Curve,
    step: Option<f64>,
    default: f64,
    unit: Unit,
    decimals: Option<u8>,
    bipolar: bool,
}

impl Default for ValueMap {
    fn default() -> Self {
        Self::new(0.0, 1.0)
    }
}

impl ValueMap {
    /// A linear map over `min..=max` (swapped if reversed), defaulting to `min`.
    pub fn new(min: f64, max: f64) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let finite = |v: f64, or: f64| if v.is_finite() { v } else { or };
        let (min, max) = (finite(min, 0.0), finite(max, 1.0));
        Self {
            min,
            max,
            curve: Curve::Linear,
            step: None,
            default: min,
            unit: Unit::None,
            decimals: None,
            bipolar: false,
        }
    }

    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// Snap values to multiples of `step` from `min`; it is also the keyboard step.
    pub fn step(mut self, step: f64) -> Self {
        self.step = (step.is_finite() && step > 0.0).then_some(step);
        self
    }

    /// The value double-click and reset return to (clamped).
    pub fn default_to(mut self, value: f64) -> Self {
        self.default = self.clamp(value);
        self
    }

    pub fn unit(mut self, unit: Unit) -> Self {
        self.unit = unit;
        self
    }

    /// Fixed decimals for display, instead of the unit's own precision.
    pub fn decimals(mut self, decimals: u8) -> Self {
        self.decimals = Some(decimals.min(6));
        self
    }

    /// Centred on the middle of the range (or on 0 when the range spans it).
    /// The default moves to the centre unless one was set.
    pub fn bipolar(mut self, bipolar: bool) -> Self {
        self.bipolar = bipolar;
        if bipolar && self.default == self.min {
            self.default = self.centre();
        }
        self
    }

    pub fn step_size(&self) -> Option<f64> {
        self.step
    }

    pub fn unit_kind(&self) -> Unit {
        self.unit
    }

    pub fn curve_kind(&self) -> Curve {
        self.curve
    }

    /// The bipolar centre: 0 when the range spans it, else the middle of the travel.
    pub fn centre(&self) -> f64 {
        if self.min < 0.0 && self.max > 0.0 {
            0.0
        } else {
            self.value_at_raw(0.5)
        }
    }

    fn clamp(&self, value: f64) -> f64 {
        if value.is_nan() {
            return self.default;
        }
        value.clamp(self.min, self.max)
    }

    fn log(&self) -> bool {
        self.curve == Curve::Log && self.min > 0.0
    }

    fn snap(&self, value: f64) -> f64 {
        match self.step {
            Some(step) => self.clamp(self.min + ((value - self.min) / step).round() * step),
            None => self.clamp(value),
        }
    }

    fn value_at_raw(&self, normal: f64) -> f64 {
        let n = if normal.is_nan() {
            0.0
        } else {
            normal.clamp(0.0, 1.0)
        };
        let span = self.max - self.min;
        if self.log() {
            self.min * (self.max / self.min).powf(n)
        } else {
            match self.curve {
                Curve::Exp(k) if k.is_finite() && k > 0.0 => self.min + span * n.powf(k),
                _ => self.min + span * n,
            }
        }
    }

    fn precision(&self, magnitude: f64) -> usize {
        if let Some(d) = self.decimals {
            return usize::from(d);
        }
        let m = magnitude.abs();
        if m < 10.0 {
            2
        } else if m < 100.0 {
            1
        } else {
            0
        }
    }

    fn number(&self, value: f64) -> String {
        let p = self.precision(value);
        trim(format!("{value:.p$}"), self.decimals.is_none())
    }
}

/// Drop trailing zeros of an adaptive-precision number (`1.50` → `1.5`, `2.00` → `2`).
fn trim(text: String, adaptive: bool) -> String {
    if !adaptive || !text.contains('.') {
        return text;
    }
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// `60` → `C4` (MIDI convention, middle C is C4).
pub fn note_name(note: f64) -> String {
    let n = note.round() as i64;
    let name = NOTES[n.rem_euclid(12) as usize];
    format!("{name}{}", n.div_euclid(12) - 1)
}

/// `C4` → 60, `F#3` → 54, `Db5` → 73. Case-insensitive letter, `#`/`b` accidentals.
pub fn parse_note(text: &str) -> Option<f64> {
    let mut chars = text.trim().chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (shift, octave) = if let Some(o) = rest.strip_prefix('#') {
        (1, o)
    } else if let Some(o) = rest.strip_prefix('b') {
        (-1, o)
    } else {
        (0, rest.as_str())
    };
    let octave: i64 = octave.trim().parse().ok()?;
    Some(((octave + 1) * 12 + base + shift) as f64)
}

/// Split `"-6.5 dB"` into the number and the lower-cased suffix.
fn split(text: &str) -> Result<(f64, String), ParseError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    let end = text
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0)))
        .map_or(text.len(), |(i, _)| i);
    let number: f64 = text[..end].parse().map_err(|_| ParseError::NotANumber)?;
    Ok((number, text[end..].trim().to_ascii_lowercase()))
}

impl ValueMapping for ValueMap {
    fn min(&self) -> f64 {
        self.min
    }

    fn max(&self) -> f64 {
        self.max
    }

    fn default_value(&self) -> f64 {
        self.default
    }

    fn to_normal(&self, value: f64) -> f64 {
        let v = self.clamp(value);
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        if self.log() {
            return ((v / self.min).ln() / (self.max / self.min).ln()).clamp(0.0, 1.0);
        }
        let t = (v - self.min) / span;
        match self.curve {
            Curve::Exp(k) if k.is_finite() && k > 0.0 => t.powf(1.0 / k),
            _ => t,
        }
        .clamp(0.0, 1.0)
    }

    fn value_at(&self, normal: f64) -> f64 {
        self.snap(self.value_at_raw(normal))
    }

    fn step_from(&self, value: f64, up: bool, fine: bool) -> f64 {
        let sign = if up { 1.0 } else { -1.0 };
        match self.step {
            // A value step; fine steps snap to it, so they still move by one step.
            Some(step) if !fine => self.snap(value + sign * step),
            _ => {
                let travel = if fine { 0.001 } else { 0.01 };
                let next = self.value_at_raw(self.to_normal(value) + sign * travel);
                let snapped = self.snap(next);
                // A step too small for the snap still moves by one step.
                match self.step {
                    Some(step) if snapped == self.snap(value) => self.snap(value + sign * step),
                    _ => snapped,
                }
            }
        }
    }

    fn constrain(&self, value: f64) -> f64 {
        self.snap(value)
    }

    fn format(&self, value: f64) -> String {
        let v = self.clamp(value);
        match self.unit {
            Unit::None => self.number(v),
            Unit::Hertz if v.abs() >= 1000.0 => format!("{} kHz", self.number(v / 1000.0)),
            Unit::Hertz => format!("{} Hz", self.number(v)),
            Unit::Seconds if v.abs() < 1.0 => format!("{} ms", self.number(v * 1000.0)),
            Unit::Seconds => format!("{} s", self.number(v)),
            Unit::Decibels if self.min <= -96.0 && v <= self.min => "-inf dB".into(),
            Unit::Decibels => {
                let p = usize::from(self.decimals.unwrap_or(1));
                let sign = if v > 0.0 { "+" } else { "" };
                format!("{sign}{v:.p$} dB")
            }
            Unit::Percent => {
                let p = usize::from(self.decimals.unwrap_or(0));
                format!("{:.p$}%", v * 100.0)
            }
            Unit::Semitones => {
                let p = usize::from(self.decimals.unwrap_or(0));
                let sign = if v > 0.0 { "+" } else { "" };
                format!("{sign}{v:.p$} st")
            }
            Unit::Note => note_name(v),
        }
    }

    fn parse(&self, text: &str) -> Result<f64, ParseError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(ParseError::Empty);
        }
        if self.unit == Unit::Note
            && let Some(note) = parse_note(trimmed)
        {
            return Ok(self.constrain(note));
        }
        if self.unit == Unit::Decibels
            && (trimmed.eq_ignore_ascii_case("-inf dB") || trimmed.eq_ignore_ascii_case("-inf"))
        {
            return Ok(self.min);
        }
        let (number, suffix) = split(trimmed)?;
        let value = match (self.unit, suffix.as_str()) {
            (_, "") => match self.unit {
                // A bare percentage above 1 is a percentage, not a fraction.
                Unit::Percent if number.abs() > 1.0 => number / 100.0,
                _ => number,
            },
            (Unit::Hertz, "hz") => number,
            (Unit::Hertz, "k" | "khz") => number * 1000.0,
            (Unit::Seconds, "s") => number,
            (Unit::Seconds, "ms") => number / 1000.0,
            (Unit::Decibels, "db") => number,
            (Unit::Percent, "%") => number / 100.0,
            (Unit::Semitones, "st" | "semitones" | "semi") => number,
            _ => return Err(ParseError::UnknownUnit),
        };
        Ok(self.constrain(value))
    }

    fn bipolar(&self) -> bool {
        self.bipolar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hz() -> ValueMap {
        ValueMap::new(20.0, 20_000.0)
            .curve(Curve::Log)
            .unit(Unit::Hertz)
    }

    #[test]
    fn log_mapping_at_the_edges() {
        let m = hz();
        assert_eq!(m.to_normal(20.0), 0.0);
        assert_eq!(m.to_normal(20_000.0), 1.0);
        assert_eq!(m.to_normal(1.0), 0.0, "below the range clamps");
        assert_eq!(m.to_normal(1e9), 1.0, "above the range clamps");
        assert_eq!(m.to_normal(f64::NAN), m.to_normal(m.default_value()));
        assert!((m.value_at(0.0) - 20.0).abs() < 1e-9);
        assert!((m.value_at(1.0) - 20_000.0).abs() < 1e-6);
        assert!((m.value_at(-0.5) - 20.0).abs() < 1e-9);
        assert!((m.value_at(2.0) - 20_000.0).abs() < 1e-6);
        // Equal travel, equal ratio: each tenth multiplies by the same factor.
        let r1 = m.value_at(0.2) / m.value_at(0.1);
        let r2 = m.value_at(0.9) / m.value_at(0.8);
        assert!((r1 - r2).abs() < 1e-9);
        for i in 0..=100 {
            let n = f64::from(i) / 100.0;
            assert!((m.to_normal(m.value_at(n)) - n).abs() < 1e-9, "{n}");
        }
        // A range touching zero cannot be logarithmic and falls back to linear.
        let z = ValueMap::new(0.0, 10.0).curve(Curve::Log);
        assert!((z.value_at(0.5) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn linear_mapping_round_trips_within_a_step() {
        let m = ValueMap::new(-10.0, 30.0).step(0.5);
        assert_eq!(m.to_normal(-10.0), 0.0);
        assert_eq!(m.to_normal(30.0), 1.0);
        assert_eq!(m.value_at(0.0), -10.0);
        assert_eq!(m.value_at(1.0), 30.0);
        for i in 0..=80 {
            let v = -10.0 + f64::from(i) * 0.5;
            assert!((m.value_at(m.to_normal(v)) - v).abs() <= 0.5, "{v}");
        }
    }

    #[test]
    fn exp_mapping_round_trips() {
        let m = ValueMap::new(0.0, 10.0).curve(Curve::Exp(3.0));
        assert!((m.value_at(0.5) - 1.25).abs() < 1e-9);
        for i in 0..=20 {
            let n = f64::from(i) / 20.0;
            assert!((m.to_normal(m.value_at(n)) - n).abs() < 1e-9);
        }
    }

    #[test]
    fn formatting_and_parsing_round_trip() {
        let cases: [(ValueMap, &[f64]); 6] = [
            (hz(), &[20.0, 440.0, 999.0, 1_000.0, 1_250.0, 15_000.0]),
            (
                ValueMap::new(0.001, 10.0).unit(Unit::Seconds),
                &[0.001, 0.25, 0.5, 1.0, 1.5, 10.0],
            ),
            (
                ValueMap::new(-60.0, 12.0).unit(Unit::Decibels),
                &[-60.0, -6.0, 0.0, 3.0, 12.0],
            ),
            (
                ValueMap::new(0.0, 1.0).unit(Unit::Percent),
                &[0.0, 0.25, 0.5, 1.0],
            ),
            (
                ValueMap::new(-24.0, 24.0).step(1.0).unit(Unit::Semitones),
                &[-24.0, -7.0, 0.0, 7.0, 24.0],
            ),
            (
                ValueMap::new(0.0, 127.0).step(1.0).unit(Unit::Note),
                &[0.0, 21.0, 60.0, 61.0, 69.0, 127.0],
            ),
        ];
        for (map, values) in cases {
            for &v in values {
                let text = map.format(v);
                let back = map.parse(&text).unwrap_or(f64::NAN);
                let tolerance = (v.abs() * 0.01).max(0.01);
                assert!((back - v).abs() <= tolerance, "{v} -> {text} -> {back}");
            }
        }
        assert_eq!(hz().format(440.0), "440 Hz");
        assert_eq!(hz().format(1_250.0), "1.25 kHz");
        assert_eq!(
            ValueMap::new(0.0, 10.0).unit(Unit::Seconds).format(0.25),
            "250 ms"
        );
        assert_eq!(
            ValueMap::new(-96.0, 6.0).unit(Unit::Decibels).format(-96.0),
            "-inf dB"
        );
        assert_eq!(
            ValueMap::new(-24.0, 6.0).unit(Unit::Decibels).format(3.0),
            "+3.0 dB"
        );
        assert_eq!(
            ValueMap::new(0.0, 1.0).unit(Unit::Percent).format(0.5),
            "50%"
        );
        assert_eq!(note_name(60.0), "C4");
        assert_eq!(note_name(69.0), "A4");
        assert_eq!(parse_note("F#3"), Some(54.0));
        assert_eq!(parse_note("db5"), Some(73.0));
    }

    #[test]
    fn typed_entry_understands_units_and_rejects_nonsense() {
        let m = hz();
        assert_eq!(m.parse("2.5 kHz"), Ok(2_500.0));
        assert_eq!(m.parse("880hz"), Ok(880.0));
        assert_eq!(m.parse("1e3"), Err(ParseError::UnknownUnit));
        assert_eq!(m.parse("abc"), Err(ParseError::NotANumber));
        assert_eq!(m.parse("  "), Err(ParseError::Empty));
        assert_eq!(m.parse("50 ms"), Err(ParseError::UnknownUnit));
        assert_eq!(m.parse("5"), Ok(20.0), "clamped into range");
        let t = ValueMap::new(0.0, 10.0).unit(Unit::Seconds);
        assert_eq!(t.parse("250ms"), Ok(0.25));
        let p = ValueMap::new(0.0, 1.0).unit(Unit::Percent);
        assert_eq!(p.parse("40%"), Ok(0.4));
        assert_eq!(p.parse("40"), Ok(0.4));
        assert_eq!(p.parse("0.4"), Ok(0.4));
    }

    #[test]
    fn steps_snap_and_fine_steps_are_smaller() {
        let m = ValueMap::new(0.0, 10.0).step(0.5);
        assert_eq!(m.step_from(2.0, true, false), 2.5);
        assert_eq!(m.step_from(0.0, false, false), 0.0, "clamped");
        assert_eq!(m.constrain(2.26), 2.5);
        let free = hz();
        let up = free.step_from(1_000.0, true, false);
        let fine = free.step_from(1_000.0, true, true);
        assert!(up > fine && fine > 1_000.0, "{up} {fine}");
        let bipolar = ValueMap::new(-1.0, 1.0).bipolar(true);
        assert_eq!(bipolar.centre(), 0.0);
        assert_eq!(
            bipolar.default_value(),
            0.0,
            "a bipolar map defaults to its centre"
        );
    }
}
