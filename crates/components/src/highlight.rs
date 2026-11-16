//! Highlight search terms inside text, and Mark (an inline highlighted span).

use gpui_kit::{
    App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
};
use guiraffe_core::{ColorName, Variant};

use crate::style::{hsla, theme};

/// Split `text` into `(segment, highlighted)` runs for case-insensitive `terms`.
/// Overlapping/adjacent matches merge; empty terms are ignored. Byte ranges always fall on
/// char boundaries (Unicode-safe), using simple per-char lowercasing.
pub fn split_highlights(text: &str, terms: &[&str]) -> Vec<(String, bool)> {
    let lower_chars: Vec<(usize, char)> = text
        .char_indices()
        .map(|(i, c)| (i, c.to_lowercase().next().unwrap_or(c)))
        .collect();
    let mut marked = vec![false; lower_chars.len()];
    for term in terms.iter().filter(|t| !t.is_empty()) {
        let needle: Vec<char> = term
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        if needle.len() > lower_chars.len() {
            continue;
        }
        for start in 0..=lower_chars.len() - needle.len() {
            if lower_chars[start..start + needle.len()]
                .iter()
                .map(|(_, c)| *c)
                .eq(needle.iter().copied())
            {
                marked[start..start + needle.len()]
                    .iter_mut()
                    .for_each(|m| *m = true);
            }
        }
    }
    let mut runs: Vec<(String, bool)> = Vec::new();
    for (idx, (byte, _)) in lower_chars.iter().enumerate() {
        let end = lower_chars
            .get(idx + 1)
            .map(|(b, _)| *b)
            .unwrap_or(text.len());
        let piece = &text[*byte..end];
        match runs.last_mut() {
            Some((s, m)) if *m == marked[idx] => s.push_str(piece),
            _ => runs.push((piece.to_owned(), marked[idx])),
        }
    }
    runs
}

#[derive(IntoElement)]
pub struct Highlight {
    text: SharedString,
    terms: Vec<SharedString>,
    color: ColorName,
}

impl Highlight {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            terms: Vec::new(),
            color: ColorName::Yellow,
        }
    }
    pub fn term(mut self, t: impl Into<SharedString>) -> Self {
        self.terms.push(t.into());
        self
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = c;
        self
    }
}

impl RenderOnce for Highlight {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        let terms: Vec<&str> = self.terms.iter().map(|t| t.as_ref()).collect();
        let runs = split_highlights(&self.text, &terms);
        let color = self.color;
        let text = hsla(crate::style::surfaces(_cx).text);
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .text_color(text)
            .children(runs.into_iter().map(move |(s, m)| {
                if m {
                    Mark::new(s).color(color).into_any_element()
                } else {
                    div().child(s).into_any_element()
                }
            }))
    }
}

#[derive(IntoElement)]
pub struct Mark {
    text: SharedString,
    color: ColorName,
}

impl Mark {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            color: ColorName::Yellow,
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = c;
        self
    }
}

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let v = theme(cx).variant(Some(self.color), Variant::Light);
        let text = crate::style::surfaces(cx).text;
        div()
            .bg(hsla(v.background))
            .text_color(hsla(text))
            .rounded(px(2.0))
            .child(self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::split_highlights as s;

    fn runs(v: Vec<(String, bool)>) -> Vec<(&'static str, bool)> {
        v.into_iter()
            .map(|(t, m)| (&*Box::leak(t.into_boxed_str()), m))
            .collect()
    }

    #[test]
    fn case_insensitive_multiple_and_merged() {
        assert_eq!(
            runs(s("Hello World", &["world"])),
            vec![("Hello ", false), ("World", true)]
        );
        assert_eq!(
            runs(s("aaaa", &["aa"])),
            vec![("aaaa", true)],
            "overlaps merge"
        );
        assert_eq!(
            runs(s("cat dog", &["cat", "dog"])),
            vec![("cat", true), (" ", false), ("dog", true)]
        );
    }

    #[test]
    fn edge_cases() {
        assert_eq!(s("", &["x"]), vec![]);
        assert_eq!(
            runs(s("abc", &[""])),
            vec![("abc", false)],
            "empty term ignored"
        );
        assert_eq!(
            runs(s("ab", &["abc"])),
            vec![("ab", false)],
            "term longer than text"
        );
        assert_eq!(
            runs(s("Ünïcode ÜNÏ", &["ünï"])),
            vec![("Ünï", true), ("code ", false), ("ÜNÏ", true)],
            "unicode-safe"
        );
        assert_eq!(
            runs(s("🦒 giraffe", &["giraffe"])),
            vec![("🦒 ", false), ("giraffe", true)]
        );
    }
}
