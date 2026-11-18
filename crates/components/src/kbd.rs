//! KbdGroup: a keyboard shortcut shown as a sequence of keys ("Ctrl + K", "g then d").
//!
//! GPUI Controls ships `Kbd` for a single keystroke; this adds the *group* Mantine calls
//! `Kbd.Group`: several keys with separators, one shared size and variant, and an optional
//! "then" join for sequences. Both share the same theme metrics, so a group next to a single
//! `Kbd` lines up.

use gpui_kit::{
    App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window,
    div, prelude::*, px,
};
use guiraffe_core::{ColorName, Size, Variant};

use crate::style::{hsla, metrics, radius, theme, variant};

/// How keys in a group are separated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KbdSeparator {
    /// `Ctrl + K`: a compact plus between keys that are pressed together.
    #[default]
    Plus,
    /// `g then d`: a word between keys pressed one after another.
    Then,
}

impl KbdSeparator {
    /// The text drawn between two keys.
    pub fn glyph(self) -> &'static str {
        match self {
            KbdSeparator::Plus => "+",
            KbdSeparator::Then => "then",
        }
    }
}

/// A sequence of keycaps, themed and sized like one [`Size`].
///
/// ```
/// use guiraffe_components::{KbdGroup, KbdSeparator};
///
/// let shortcut = KbdGroup::new(["Ctrl", "K"]);
/// let sequence = KbdGroup::new(["g", "d"]).separator(KbdSeparator::Then);
/// ```
#[derive(IntoElement)]
pub struct KbdGroup {
    keys: Vec<SharedString>,
    separator: KbdSeparator,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    variant: Option<Variant>,
    style: StyleRefinement,
}

impl KbdGroup {
    pub fn new(keys: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self {
            keys: keys.into_iter().map(Into::into).collect(),
            separator: KbdSeparator::Plus,
            size: None,
            color: None,
            variant: None,
            style: StyleRefinement::default(),
        }
    }

    /// `Plus` (default) for chords, `Then` for sequences.
    pub fn separator(mut self, separator: KbdSeparator) -> Self {
        self.separator = separator;
        self
    }
}

crate::props!(KbdGroup { size: Size => size, variant: Variant => variant });
crate::styled!(KbdGroup);

impl KbdGroup {
    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }
}

/// The keys of a shortcut, split for display: `"ctrl-shift-k"` → `["ctrl", "shift", "k"]`.
///
/// Parsing is deliberately simple: GPUI's own `Keystroke` formatter decides the labels, so a
/// caller passes pre-formatted keys and this only helps with the `-` separator most bindings use.
pub fn split_keys(source: &str) -> Vec<&str> {
    source
        .split('-')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Height of one keycap at `size`.
pub fn key_height(size: Size) -> f32 {
    (size.font_size() * 1.6).round()
}

impl RenderOnce for KbdGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let kind = self.variant.unwrap_or(Variant::Default);
        let v = variant(cx, self.color, kind);
        let key_text = px(size.font_size() * scale);
        let height = px(key_height(size));
        let glyph = self.separator.glyph();
        let dimmed = hsla(crate::style::surfaces(cx).dimmed);
        let count = self.keys.len();
        let el = div()
            .flex()
            .items_center()
            .gap(px(metrics::gap(size) / 2.0))
            .text_size(key_text)
            .children(self.keys.into_iter().enumerate().flat_map(move |(i, key)| {
                let selector = format!("gr-kbd:{key}");
                let cap = div()
                    .debug_selector(move || selector)
                    .flex()
                    .items_center()
                    .justify_center()
                    .min_w(height)
                    .h(height)
                    .px(px(size.spacing() / 3.0))
                    .rounded(radius(Some(Size::Xs), cx))
                    .bg(hsla(v.background))
                    .border_1()
                    .border_color(hsla(v.border))
                    .text_color(hsla(v.foreground))
                    .child(key);
                let mut parts = vec![cap.into_any_element()];
                if i + 1 < count {
                    parts.push(
                        div()
                            .text_color(dimmed)
                            .text_size(px(size.font_size() * scale * 0.85))
                            .child(glyph)
                            .into_any_element(),
                    );
                }
                parts
            }));
        crate::style::refine(el, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{KbdSeparator, key_height, split_keys};
    use guiraffe_core::Size;

    #[test]
    fn separators_read_naturally() {
        assert_eq!(KbdSeparator::Plus.glyph(), "+");
        assert_eq!(KbdSeparator::Then.glyph(), "then");
        assert_eq!(KbdSeparator::default(), KbdSeparator::Plus);
    }

    #[test]
    fn splitting_drops_empty_parts() {
        assert_eq!(split_keys("ctrl-shift-k"), vec!["ctrl", "shift", "k"]);
        assert_eq!(split_keys(" a -- b "), vec!["a", "b"]);
        assert!(split_keys("").is_empty());
    }

    #[test]
    fn keycap_height_tracks_the_size_scale() {
        assert!(key_height(Size::Md) > key_height(Size::Sm));
        assert!(key_height(Size::Xs) > 12.0);
    }
}
