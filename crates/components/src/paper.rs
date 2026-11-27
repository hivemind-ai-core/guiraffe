//! Paper (a themed surface) and Card (Paper with full-bleed sections).

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, StyleRefinement, Window, div,
    prelude::*, px,
};
use guiraffe_core::{Size, SurfaceKind};

use crate::style::{hsla, radius, refine, shadows, surfaces};

#[derive(IntoElement)]
pub struct Paper {
    style: StyleRefinement,
    shadow: Option<Size>,
    radius: Option<Size>,
    /// `None`: `Md` spacing scaled by the context's density.
    padding: Option<Size>,
    no_padding: bool,
    with_border: bool,
    /// The level of the surface ladder this paper paints (body by default).
    surface: SurfaceKind,
    children: Vec<AnyElement>,
}

impl Default for Paper {
    fn default() -> Self {
        Self::new()
    }
}

impl Paper {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            shadow: None,
            radius: None,
            padding: None,
            no_padding: false,
            with_border: false,
            surface: SurfaceKind::Body,
            children: Vec::new(),
        }
    }
}

impl Paper {
    /// No padding (content that bleeds to the edge, e.g. a table).
    pub fn p_0(mut self) -> Self {
        self.no_padding = true;
        self
    }
    /// Paint a level of the theme's surface ladder: `Sunken` for wells and recessed areas,
    /// `Raised` for popover-like cards.
    pub fn surface(mut self, kind: SurfaceKind) -> Self {
        self.surface = kind;
        self
    }
}

/// Padding in px: explicit size, or `Md` scaled by the inherited density.
pub fn pad_for(size: Option<Size>) -> f32 {
    guiraffe_core::StyleContext::current()
        .density
        .space(size.unwrap_or(Size::Md))
}

crate::props!(Paper { shadow: Size => shadow, radius: Size => radius, p: Size => padding, with_border: bool => with_border });
crate::styled!(Paper);

impl ParentElement for Paper {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Paper {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let el = div()
            .bg(hsla(s.of(self.surface)))
            .rounded(radius(self.radius, cx))
            .text_color(hsla(s.text))
            .when(!self.no_padding, |el| el.p(px(pad_for(self.padding))))
            .when(self.with_border, |el| {
                el.border_1().border_color(hsla(s.border))
            })
            .when_some(self.shadow, |el, sh| el.shadow(shadows(sh, cx)));
        refine(el, &self.style).children(self.children)
    }
}

enum CardPart {
    Content(AnyElement),
    Section(Box<CardSection>),
}

/// A card: bordered paper whose [`CardSection`]s span the full width (images, headers,
/// footers). Layout happens in `render` with the final padding, so builder call order
/// never matters, and first/last sections also bleed to the top/bottom edge.
#[derive(IntoElement)]
pub struct Card {
    style: StyleRefinement,
    shadow: Option<Size>,
    radius: Option<Size>,
    padding: Option<Size>,
    with_border: bool,
    parts: Vec<CardPart>,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            shadow: Some(Size::Sm),
            radius: None,
            padding: None,
            with_border: true,
            parts: Vec::new(),
        }
    }
    pub fn section(mut self, section: CardSection) -> Self {
        self.parts.push(CardPart::Section(Box::new(section)));
        self
    }
}

crate::props!(Card { shadow: Size => shadow, radius: Size => radius, p: Size => padding, with_border: bool => with_border });
crate::styled!(Card);

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.parts
            .extend(elements.into_iter().map(CardPart::Content));
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pad_px = pad_for(self.padding);
        let pad = px(pad_px);
        let last = self.parts.len().saturating_sub(1);
        let mut paper = Paper::new().with_border(self.with_border);
        paper.padding = self.padding;
        if let Some(s) = self.shadow {
            paper = paper.shadow(s);
        }
        if let Some(r) = self.radius {
            paper = paper.radius(r);
        }
        paper.style = self.style;
        let recipe = crate::style::theme(cx).recipes.card;
        let body = div().flex().flex_col().gap(px(pad_px * 0.75)).children(
            self.parts
                .into_iter()
                .enumerate()
                .map(|(i, part)| match part {
                    CardPart::Content(el) => el,
                    CardPart::Section(sec) => div()
                        .mx(-pad)
                        .when(i == 0, |el| el.mt(-pad))
                        .when(i == last, |el| el.mb(-pad))
                        .child(*sec)
                        .into_any_element(),
                }),
        );
        // Everything inside a card inherits the card recipe (raised surface).
        paper
            .overflow_hidden()
            .child(guiraffe_core::provide(recipe, body))
    }
}

#[derive(IntoElement, Default)]
pub struct CardSection {
    style: StyleRefinement,
    with_border: bool,
    children: Vec<AnyElement>,
}

impl CardSection {
    pub fn new() -> Self {
        Self::default()
    }
}

crate::props!(CardSection { with_border: bool => with_border });
crate::styled!(CardSection);

impl ParentElement for CardSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for CardSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let el = div().w_full().when(self.with_border, |el| {
            el.border_t_1().border_b_1().border_color(hsla(s.border))
        });
        refine(el, &self.style).children(self.children)
    }
}
