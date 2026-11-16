//! Paper (a themed surface) and Card (Paper with full-bleed sections).

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window, div,
    px,
};
use guiraffe_core::Size;

use crate::style::{hsla, radius, shadows, surfaces};

#[derive(IntoElement)]
pub struct Paper {
    style: StyleRefinement,
    shadow: Option<Size>,
    radius: Option<Size>,
    padding: Option<Size>,
    with_border: bool,
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
            padding: Some(Size::Md),
            with_border: false,
            children: Vec::new(),
        }
    }
}

crate::props!(Paper { shadow: Size => shadow, radius: Size => radius, p: Size => padding, with_border: bool => with_border });

impl Styled for Paper {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Paper {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Paper {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let mut el = div()
            .bg(hsla(s.body))
            .rounded(radius(self.radius, cx))
            .text_color(hsla(s.text));
        if let Some(p) = self.padding {
            el = el.p(px(p.spacing()));
        }
        if self.with_border {
            el = el.border_1().border_color(hsla(s.border));
        }
        if let Some(sh) = self.shadow {
            el = el.shadow(shadows(sh, cx));
        }
        let el = gpui_kit::base::StyledExt::refine_style(el, &self.style);
        el.children(self.children)
    }
}

/// A card: bordered paper whose [`CardSection`]s span the full width (images, headers, footers).
#[derive(IntoElement)]
pub struct Card {
    paper: Paper,
    padding: Size,
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    pub fn new() -> Self {
        Self {
            paper: Paper::new().with_border(true).shadow(Size::Sm).p(Size::Md),
            padding: Size::Md,
        }
    }
    pub fn shadow(mut self, s: Size) -> Self {
        self.paper = self.paper.shadow(s);
        self
    }
    pub fn radius(mut self, s: Size) -> Self {
        self.paper = self.paper.radius(s);
        self
    }
    pub fn with_border(mut self, b: bool) -> Self {
        self.paper = self.paper.with_border(b);
        self
    }
    pub fn p(mut self, s: Size) -> Self {
        self.padding = s;
        self.paper = self.paper.p(s);
        self
    }
    /// Add a full-bleed section (cancels the card padding horizontally).
    pub fn section(mut self, section: CardSection) -> Self {
        let pad = px(self.padding.spacing());
        self.paper = self.paper.child(div().mx(-pad).child(section));
        self
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.paper.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.paper
            .flex()
            .flex_col()
            .gap(px(self.padding.spacing() * 0.75))
            .overflow_hidden()
    }
}

#[derive(IntoElement, Default)]
pub struct CardSection {
    with_border: bool,
    children: Vec<AnyElement>,
}

impl CardSection {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_border(mut self, b: bool) -> Self {
        self.with_border = b;
        self
    }
}

impl ParentElement for CardSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for CardSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let mut el = div().w_full();
        if self.with_border {
            el = el.border_t_1().border_b_1().border_color(hsla(s.border));
        }
        el.children(self.children)
    }
}
