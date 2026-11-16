//! Highlight search terms inside text, and Mark (an inline highlighted span).

use std::ops::Range;

use gpui_kit::{
    App, HighlightStyle, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement,
    StyledText, Window, div, prelude::*, px,
};

use crate::style::{hsla, refine, surfaces};

/// Merged, sorted byte ranges of case-insensitive matches of `terms` in `text`.
///
/// Matching uses simple per-char lowercasing (no length-changing folds such as `ß` → `ss`),
/// so ranges always fall on char boundaries. Cost is O(n·m) per term with no allocation per
/// candidate; compute once per text/terms change, not per frame.
pub fn highlight_ranges(text: &str, terms: &[&str]) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text
        .char_indices()
        .map(|(i, c)| (i, c.to_lowercase().next().unwrap_or(c)))
        .collect();
    let end_of = |i: usize| chars.get(i).map(|(b, _)| *b).unwrap_or(text.len());
    let mut hits: Vec<Range<usize>> = Vec::new();
    for term in terms.iter().filter(|t| !t.is_empty()) {
        let needle: Vec<char> = term
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        if needle.len() > chars.len() {
            continue;
        }
        for start in 0..=chars.len() - needle.len() {
            if chars[start..start + needle.len()]
                .iter()
                .map(|(_, c)| *c)
                .eq(needle.iter().copied())
            {
                hits.push(chars[start].0..end_of(start + needle.len()));
            }
        }
    }
    hits.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(hits.len());
    for r in hits {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// Text with search terms highlighted; wraps naturally as one text run.
#[derive(IntoElement)]
pub struct Highlight {
    text: SharedString,
    terms: Vec<SharedString>,
    style: StyleRefinement,
}

impl Highlight {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            terms: Vec::new(),
            style: StyleRefinement::default(),
        }
    }
    pub fn term(mut self, t: impl Into<SharedString>) -> Self {
        self.terms.push(t.into());
        self
    }
}

crate::styled!(Highlight);

impl RenderOnce for Highlight {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let terms: Vec<&str> = self.terms.iter().map(|t| t.as_ref()).collect();
        let mark = HighlightStyle {
            background_color: Some(hsla(s.mark)),
            ..Default::default()
        };
        let ranges = highlight_ranges(&self.text, &terms)
            .into_iter()
            .map(|r| (r, mark));
        let text = StyledText::new(self.text.clone()).with_highlights(ranges);
        refine(div().text_color(hsla(s.text)), &self.style).child(text)
    }
}

/// An inline highlighted span.
#[derive(IntoElement)]
pub struct Mark {
    text: SharedString,
    style: StyleRefinement,
}

impl Mark {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            style: StyleRefinement::default(),
        }
    }
}

crate::styled!(Mark);

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let el = div()
            .bg(hsla(s.mark))
            .text_color(hsla(s.text))
            .rounded(px(guiraffe_core::Size::Xs.radius()))
            .child(self.text);
        refine(el, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::highlight_ranges as h;

    #[test]
    fn case_insensitive_multiple_and_merged() {
        assert_eq!(h("Hello World", &["world"]), vec![6..11]);
        assert_eq!(h("aaaa", &["aa"]), vec![0..4], "overlaps merge");
        assert_eq!(h("cat dog", &["cat", "dog"]), vec![0..3, 4..7]);
        assert_eq!(
            h("abcd", &["ab", "cd"]),
            vec![0..4],
            "adjacent matches merge into one run"
        );
    }

    #[test]
    fn edge_cases_and_unicode_boundaries() {
        assert!(h("", &["x"]).is_empty());
        assert!(h("abc", &[""]).is_empty(), "empty term ignored");
        assert!(h("ab", &["abc"]).is_empty());
        let t = "Ünïcode ÜNÏ";
        let r = h(t, &["ünï"]);
        assert_eq!(
            r.iter().map(|r| &t[r.clone()]).collect::<Vec<_>>(),
            vec!["Ünï", "ÜNÏ"],
            "ranges on char boundaries"
        );
        let g = "🦒 giraffe";
        assert_eq!(
            h(g, &["giraffe"])
                .iter()
                .map(|r| &g[r.clone()])
                .collect::<Vec<_>>(),
            vec!["giraffe"]
        );
    }
}
