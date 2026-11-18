//! AvatarGroup: overlapping avatars with a `+N` overflow chip.
//!
//! Controls ships `Avatar` (initial or image) and a minimal group; this is the Mantine-shaped
//! one: a fixed overlap that scales with the size, a `+N` chip for people who do not fit, and
//! safe handling of limits, empty groups and fewer avatars than the limit.

use gpui_kit::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, Rgb, Size, Variant};

use crate::color_swatch::check_color;
use crate::style::{hsla, surfaces, theme, variant};

/// One person in an [`AvatarGroup`]: a name, optionally an image URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub name: SharedString,
    pub image: Option<SharedString>,
}

impl Person {
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            image: None,
        }
    }

    pub fn image(mut self, url: impl Into<SharedString>) -> Self {
        self.image = Some(url.into());
        self
    }

    /// Initials for the fallback: the first letter of the first two words, upper-cased.
    ///
    /// A single word takes its first two characters, so `"zoe"` reads `ZO`, not `Z`.
    pub fn initials(&self) -> String {
        let words: Vec<&str> = self.name.split_whitespace().collect();
        let mut out: String = words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect();
        if out.chars().count() == 1 {
            out = self.name.chars().take(2).collect();
        }
        out.to_uppercase()
    }
}

/// How many avatars fit and how many are hidden: `(shown, hidden)` for `count` and `limit`.
///
/// A `limit` of 0 shows nobody (and hides everybody); a limit at or above the count shows all
/// and hides none, so the `+N` chip never appears when it would read `+0`.
pub fn visible(count: usize, limit: usize) -> (usize, usize) {
    let shown = count.min(limit);
    (shown, count - shown)
}

/// Diameter of one avatar at `size` (Mantine: ~1.4× the control height for `md`).
pub fn avatar_size(size: Size) -> f32 {
    (size.control_height() * 0.85).round()
}

/// How far each avatar overlaps the one before it.
pub fn overlap(size: Size) -> f32 {
    (avatar_size(size) * 0.3).round()
}

/// An overlapping row of avatars with a `+N` chip for the rest.
///
/// ```
/// use guiraffe_components::{AvatarGroup, Person};
///
/// let people = AvatarGroup::new([Person::new("Ada Lovelace"), Person::new("Grace Hopper")])
///     .limit(3);
/// ```
#[derive(IntoElement)]
pub struct AvatarGroup {
    people: Vec<Person>,
    limit: usize,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    /// A custom `+N` chip; by default it is a neutral one.
    more: Option<AnyElement>,
    style: StyleRefinement,
}

impl AvatarGroup {
    pub fn new(people: impl IntoIterator<Item = Person>) -> Self {
        Self {
            people: people.into_iter().collect(),
            limit: usize::MAX,
            size: None,
            color: None,
            more: None,
            style: StyleRefinement::default(),
        }
    }

    /// Show at most `limit` avatars, with a `+N` chip for the rest.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }

    /// Replace the `+N` chip's content (it keeps the overlap and ring).
    pub fn more(mut self, element: impl IntoElement) -> Self {
        self.more = Some(element.into_any_element());
        self
    }
}

crate::props!(AvatarGroup { size: Size => size });
crate::styled!(AvatarGroup);

/// The text of the overflow chip.
pub fn more_label(hidden: usize) -> String {
    format!("+{hidden}")
}

impl RenderOnce for AvatarGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let s = surfaces(cx);
        let side = px(avatar_size(size) * scale);
        let overlap = px(overlap(size) * scale);
        let (shown, hidden) = visible(self.people.len(), self.limit);
        let ring = hsla(s.body);
        let text = px(size.font_size() * scale * 0.8);
        // The +N chip is a neutral surface with body text, so it stays legible in both schemes
        // whatever colour the group uses.
        // Filled, so the label always has a contrast-checked foreground; without a colour the
        // chip is the neutral track with body text.
        let (background, foreground) = match self.color {
            Some(color) => {
                let v = variant(cx, Some(color), Variant::Filled);
                (hsla(v.background), hsla(v.foreground))
            }
            None => (hsla(s.track), hsla(s.text)),
        };
        let people = self.people.into_iter().take(shown);
        let more = self.more;
        let label = more_label(hidden);
        crate::style::refine(
            div()
                .id("gr-avatar-group")
                .flex()
                .items_center()
                .children(people.enumerate().map(move |(i, person)| {
                    let initials = person.initials();
                    let tint = tint_for(&person.name, cx);
                    div()
                        .size(side)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(hsla(tint))
                        .text_color(hsla(check_color(tint)))
                        .text_size(text)
                        .border_2()
                        .border_color(ring)
                        .when(i > 0, |el| el.ml(-overlap))
                        .child(initials)
                }))
                .when(hidden > 0, |el| {
                    el.child(
                        div()
                            .size(side)
                            .ml(-overlap)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(background)
                            .text_color(foreground)
                            .text_size(text)
                            .border_2()
                            .border_color(ring)
                            .when_some(more, |el, more| el.child(more))
                            .child(label),
                    )
                }),
            &self.style,
        )
    }
}

/// A stable tint per name, so the same person always reads the same in a group.
///
/// 12 evenly spaced hues keep every avatar at one visual weight; the lightness is fixed so the
/// initials' contrast is predictable (the check colour is then chosen by WCAG contrast).
pub fn tint_for(name: &str, _cx: &App) -> Rgb {
    let color = hue_for(name);
    color.shade(crate::color_swatch::swatch_shade(
        guiraffe_core::Scheme::Light,
    ))
}

/// The palette colour a name always maps to.
pub fn hue_for(name: &str) -> ColorName {
    const HUES: [ColorName; 12] = [
        ColorName::Red,
        ColorName::Pink,
        ColorName::Grape,
        ColorName::Violet,
        ColorName::Indigo,
        ColorName::Blue,
        ColorName::Cyan,
        ColorName::Teal,
        ColorName::Green,
        ColorName::Lime,
        ColorName::Yellow,
        ColorName::Orange,
    ];
    let hash = name.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    HUES[(hash % HUES.len() as u32) as usize]
}

#[cfg(test)]
mod tests {
    use guiraffe_core::Size;

    use super::{Person, avatar_size, hue_for, more_label, overlap, visible};

    #[test]
    fn limits_and_overflow_counts() {
        assert_eq!(visible(5, 3), (3, 2));
        assert_eq!(visible(2, 3), (2, 0), "no +0 chip");
        assert_eq!(visible(3, 3), (3, 0), "exactly the limit fits");
        assert_eq!(visible(4, 0), (0, 4), "a zero limit shows nobody");
        assert_eq!(visible(0, 3), (0, 0), "an empty group has nothing to hide");
    }

    #[test]
    fn more_label_reads_plus_n() {
        assert_eq!(more_label(2), "+2");
        assert_eq!(more_label(0), "+0");
    }

    #[test]
    fn initials_handle_one_word_and_whitespace() {
        assert_eq!(Person::new("Ada Lovelace").initials(), "AL");
        assert_eq!(Person::new("Ada Byron Lovelace").initials(), "AB");
        assert_eq!(Person::new("zoe").initials(), "ZO");
        assert_eq!(Person::new("  grace   hopper ").initials(), "GH");
        assert_eq!(Person::new("").initials(), "");
    }

    #[test]
    fn sizes_overlap_proportionally() {
        for w in Size::ALL.windows(2) {
            assert!(avatar_size(w[1]) > avatar_size(w[0]));
            assert!(overlap(w[1]) > overlap(w[0]));
        }
        assert!(
            overlap(Size::Md) < avatar_size(Size::Md),
            "never fully covered"
        );
    }

    #[test]
    fn tints_are_stable_and_vary_between_people() {
        // The hue is a pure function of the name, so this needs no App.
        assert_eq!(hue_for("Ada Lovelace"), hue_for("Ada Lovelace"));
        let distinct: std::collections::HashSet<u32> = [
            "Ada Lovelace",
            "Grace Hopper",
            "Alan Turing",
            "Katherine Johnson",
            "Edsger Dijkstra",
            "Barbara Liskov",
        ]
        .iter()
        .map(|n| hue_for(n) as u32)
        .collect();
        assert!(
            distinct.len() >= 4,
            "different people mostly differ: {distinct:?}"
        );
    }
}
