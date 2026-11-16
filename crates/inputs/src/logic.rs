//! Pure input logic (no GPUI): selection, tags, range maths, password strength, file filters.

use std::collections::BTreeSet;

/// Chip group selection: single (radio-like, re-click keeps it) or multiple (toggle).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChipSelection {
    pub multiple: bool,
    pub selected: BTreeSet<usize>,
}

impl ChipSelection {
    /// Toggle chip `i`; returns whether the selection changed.
    pub fn toggle(&mut self, i: usize) -> bool {
        if self.multiple {
            if !self.selected.remove(&i) {
                self.selected.insert(i);
            }
            true
        } else if self.selected.len() == 1 && self.selected.contains(&i) {
            false
        } else {
            self.selected.clear();
            self.selected.insert(i);
            true
        }
    }
}

/// Rules for a tags input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRules {
    pub max_tags: Option<usize>,
    pub allow_duplicates: bool,
    /// Characters that complete a tag while typing (in addition to Enter).
    pub split_chars: Vec<char>,
    pub max_len: usize,
}

impl Default for TagRules {
    fn default() -> Self {
        Self {
            max_tags: None,
            allow_duplicates: false,
            split_chars: vec![','],
            max_len: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagRejected {
    Empty,
    Duplicate,
    Full,
    TooLong,
}

/// Normalise and validate one candidate tag against `existing`.
pub fn accept_tag(existing: &[String], raw: &str, rules: &TagRules) -> Result<String, TagRejected> {
    let tag = raw.trim();
    if tag.is_empty() {
        return Err(TagRejected::Empty);
    }
    if tag.chars().count() > rules.max_len {
        return Err(TagRejected::TooLong);
    }
    if rules.max_tags.is_some_and(|m| existing.len() >= m) {
        return Err(TagRejected::Full);
    }
    if !rules.allow_duplicates && existing.iter().any(|e| e.eq_ignore_ascii_case(tag)) {
        return Err(TagRejected::Duplicate);
    }
    Ok(tag.to_owned())
}

/// Split pasted or typed text on split characters (and newlines) into candidate tags,
/// returning the completed candidates and the unfinished remainder.
pub fn split_input<'a>(text: &'a str, rules: &TagRules) -> (Vec<&'a str>, &'a str) {
    let is_split = |c: char| c == '\n' || rules.split_chars.contains(&c);
    match text.rfind(is_split) {
        None => (Vec::new(), text),
        Some(last) => {
            let done = &text[..last];
            let rest = &text[last + text[last..].chars().next().map_or(1, char::len_utf8)..];
            (
                done.split(is_split)
                    .filter(|s| !s.trim().is_empty())
                    .collect(),
                rest,
            )
        }
    }
}

/// A two-thumb range on `[min, max]` with `step` and a minimum distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range2 {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub min_distance: f32,
    pub low: f32,
    pub high: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thumb {
    Low,
    High,
}

impl Range2 {
    /// A sanitised range: finite bounds with `min < max`, positive step, values clamped.
    pub fn new(min: f32, max: f32, step: f32, low: f32, high: f32) -> Range2 {
        let (min, max) = if min.is_finite() && max.is_finite() && min < max {
            (min, max)
        } else {
            (0.0, 100.0)
        };
        let step = if step.is_finite() && step > 0.0 {
            step
        } else {
            1.0
        };
        let mut r = Range2 {
            min,
            max,
            step,
            min_distance: 0.0,
            low: min,
            high: max,
        };
        r.set(Thumb::Low, low);
        r.set(Thumb::High, high);
        r
    }

    fn snap(&self, v: f32) -> f32 {
        let v = if v.is_finite() {
            v.clamp(self.min, self.max)
        } else {
            self.min
        };
        (self.min + ((v - self.min) / self.step).round() * self.step).clamp(self.min, self.max)
    }

    /// Move one thumb; it cannot cross the other (respecting `min_distance`). Returns
    /// whether anything changed.
    pub fn set(&mut self, thumb: Thumb, v: f32) -> bool {
        let v = self.snap(v);
        let gap = self.min_distance.max(0.0);
        let before = (self.low, self.high);
        match thumb {
            Thumb::Low => self.low = v.min(self.high - gap).max(self.min),
            Thumb::High => self.high = v.max(self.low + gap).min(self.max),
        }
        (self.low, self.high) != before
    }

    /// The thumb a press at value `v` should grab: the nearer one; ties go to the one that
    /// can move toward `v` (so overlapping thumbs stay draggable both ways).
    pub fn nearest(&self, v: f32) -> Thumb {
        let (dl, dh) = ((v - self.low).abs(), (v - self.high).abs());
        if dl < dh || (dl == dh && v < self.low) {
            Thumb::Low
        } else {
            Thumb::High
        }
    }

    /// Value at fraction `t` ∈ [0,1] of the track.
    pub fn value_at(&self, t: f32) -> f32 {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.min + (self.max - self.min) * t
    }

    /// Fraction of the track for value `v`.
    pub fn fraction(&self, v: f32) -> f32 {
        ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }
}

/// Password strength 0..=4 from length and character variety (Mantine's meter model).
pub fn password_strength(pw: &str) -> u8 {
    let len = pw.chars().count();
    if len == 0 {
        return 0;
    }
    let classes = [
        pw.chars().any(|c| c.is_lowercase()),
        pw.chars().any(|c| c.is_uppercase()),
        pw.chars().any(|c| c.is_ascii_digit()),
        pw.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count() as u8;
    let length_score = match len {
        0..=5 => 0,
        6..=9 => 1,
        10..=13 => 2,
        _ => 3,
    };
    ((length_score + classes) / 2).min(4)
}

/// Whether `file_name` matches an accept list like `[".png", "image/*", ".JSON"]`
/// (extensions case-insensitively; `*/*` or empty accepts all; MIME globs by family).
pub fn accepts(accept: &[&str], file_name: &str) -> bool {
    if accept.is_empty() || accept.contains(&"*/*") {
        return true;
    }
    let ext = file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let family = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "image",
        "mp4" | "mov" | "webm" | "mkv" => "video",
        "mp3" | "wav" | "flac" | "ogg" => "audio",
        "txt" | "md" | "csv" | "json" | "toml" | "yaml" | "yml" => "text",
        _ => "",
    };
    accept.iter().any(|a| {
        let a = a.trim().to_ascii_lowercase();
        match (a.strip_prefix('.'), a.strip_suffix("/*")) {
            (Some(e), _) => !ext.is_empty() && e == ext,
            (_, Some(f)) => !family.is_empty() && f == family,
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_selection_single_and_multiple() {
        let mut s = ChipSelection::default();
        assert!(s.toggle(1));
        assert!(!s.toggle(1), "single: re-click keeps the selection");
        assert!(s.toggle(2));
        assert_eq!(s.selected.iter().copied().collect::<Vec<_>>(), vec![2]);
        let mut m = ChipSelection {
            multiple: true,
            ..Default::default()
        };
        m.toggle(1);
        m.toggle(3);
        m.toggle(1);
        assert_eq!(m.selected.iter().copied().collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn tags_validate_and_split() {
        let rules = TagRules {
            max_tags: Some(2),
            ..Default::default()
        };
        let existing = vec!["Rust".to_string()];
        assert_eq!(accept_tag(&existing, "  gpui ", &rules), Ok("gpui".into()));
        assert_eq!(
            accept_tag(&existing, "rust", &rules),
            Err(TagRejected::Duplicate),
            "case-insensitive"
        );
        assert_eq!(
            accept_tag(&existing, "   ", &rules),
            Err(TagRejected::Empty)
        );
        assert_eq!(
            accept_tag(&["a".into(), "b".into()], "c", &rules),
            Err(TagRejected::Full)
        );
        assert_eq!(
            accept_tag(&[], &"x".repeat(65), &TagRules::default()),
            Err(TagRejected::TooLong)
        );
        assert_eq!(
            split_input("a, b,,c, d", &TagRules::default()),
            (vec!["a", " b", "c"], " d")
        );
        assert_eq!(
            split_input("no split", &TagRules::default()),
            (vec![], "no split")
        );
        assert_eq!(
            split_input("x\ny\n", &TagRules::default()),
            (vec!["x", "y"], "")
        );
        assert_eq!(
            split_input("ü,ä", &TagRules::default()),
            (vec!["ü"], "ä"),
            "utf-8 safe"
        );
    }

    #[test]
    fn range_clamps_snaps_and_never_crosses() {
        let mut r = Range2::new(0.0, 100.0, 5.0, 20.0, 80.0);
        r.min_distance = 10.0;
        assert!(r.set(Thumb::Low, 93.0));
        assert_eq!(
            (r.low, r.high),
            (70.0, 80.0),
            "low stops min_distance below high"
        );
        assert!(
            !r.set(Thumb::High, -5.0),
            "blocked by min_distance: no change reported"
        );
        assert_eq!(
            (r.low, r.high),
            (70.0, 80.0),
            "high stops min_distance above low"
        );
        assert!(!r.set(Thumb::High, r.high), "no-op reports no change");
        assert_eq!(Range2::new(0.0, 10.0, 1.0, 3.4, 7.6).low, 3.0);
        let bad = Range2::new(f32::NAN, -1.0, 0.0, f32::INFINITY, f32::NAN);
        assert!(bad.low.is_finite() && bad.high.is_finite() && bad.min < bad.max && bad.step > 0.0);
        let r = Range2::new(0.0, 100.0, 1.0, 40.0, 40.0);
        assert_eq!(r.nearest(10.0), Thumb::Low);
        assert_eq!(
            r.nearest(90.0),
            Thumb::High,
            "overlapping thumbs: drag toward the click"
        );
        assert_eq!(r.value_at(0.25), 25.0);
        assert_eq!(r.fraction(150.0), 1.0);
    }

    #[test]
    fn password_strength_grows_with_length_and_variety() {
        assert_eq!(password_strength(""), 0);
        assert!(password_strength("abc") < password_strength("abcdefghij"));
        assert!(password_strength("abcdefghij") < password_strength("Abcdefgh1!"));
        assert_eq!(password_strength("Correct-Horse-Battery-9"), 3);
        assert!(password_strength(&"aB3$".repeat(10)) <= 4);
    }

    #[test]
    fn accept_filters() {
        assert!(accepts(&[], "x.exe"));
        assert!(accepts(&["*/*"], "x"));
        assert!(accepts(&[".PNG"], "photo.png"));
        assert!(accepts(&["image/*"], "a.JPeG"));
        assert!(!accepts(&["image/*"], "notes.md"));
        assert!(!accepts(&[".png"], "noext"));
        assert!(accepts(&[".md", "text/*"], "data.csv"));
    }
}
