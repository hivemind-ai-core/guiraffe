//! Blockquote, Indicator and Pill.

use gpui_kit::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, Size, Variant};

use crate::style::{hsla, radius, surfaces, theme, variant};

#[derive(IntoElement)]
pub struct Blockquote {
    color: Option<ColorName>,
    cite: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl Blockquote {
    pub fn new() -> Self {
        Self {
            color: None,
            cite: None,
            children: Vec::new(),
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
    pub fn cite(mut self, c: impl Into<SharedString>) -> Self {
        self.cite = Some(c.into());
        self
    }
}

impl Default for Blockquote {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for Blockquote {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Blockquote {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let v = variant(cx, self.color, Variant::Light);
        let accent = variant(cx, self.color, Variant::Filled).background;
        let s = surfaces(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(Size::Xs.spacing()))
            .p(px(Size::Md.spacing()))
            .bg(hsla(v.background))
            .text_color(hsla(s.text))
            .border_l_4()
            .border_color(hsla(accent))
            .rounded_r(radius(None, cx))
            .children(self.children)
            .when_some(self.cite, |el, c| {
                el.child(
                    div()
                        .text_size(px(Size::Sm.font_size()))
                        .text_color(hsla(s.dimmed))
                        .child(format!("— {c}")),
                )
            })
    }
}

/// Where an [`Indicator`] dot sits relative to its child.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndicatorPosition {
    #[default]
    TopEnd,
    TopStart,
    BottomEnd,
    BottomStart,
}

/// A dot or count badge anchored to the corner of its child.
#[derive(IntoElement)]
pub struct Indicator {
    child: AnyElement,
    label: Option<SharedString>,
    color: Option<ColorName>,
    position: IndicatorPosition,
    disabled: bool,
}

impl Indicator {
    pub fn new(child: impl IntoElement) -> Self {
        Self {
            child: child.into_any_element(),
            label: None,
            color: None,
            position: IndicatorPosition::TopEnd,
            disabled: false,
        }
    }
    pub fn label(mut self, l: impl Into<SharedString>) -> Self {
        self.label = Some(l.into());
        self
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
    pub fn position(mut self, p: IndicatorPosition) -> Self {
        self.position = p;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let v = variant(cx, self.color, Variant::Filled);
        let body = hsla(surfaces(cx).body);
        let dot = match &self.label {
            Some(_) => div()
                .min_w(px(16.0))
                .h(px(16.0))
                .px(px(4.0))
                .text_size(px(10.0)),
            None => div().size(px(10.0)),
        }
        .absolute()
        .rounded_full()
        .bg(hsla(v.background))
        .text_color(hsla(v.foreground))
        .border_2()
        .border_color(body)
        .flex()
        .items_center()
        .justify_center()
        .when_some(self.label, |el, l| el.child(l));
        let dot = match self.position {
            IndicatorPosition::TopEnd => dot.top(px(-4.0)).right(px(-4.0)),
            IndicatorPosition::TopStart => dot.top(px(-4.0)).left(px(-4.0)),
            IndicatorPosition::BottomEnd => dot.bottom(px(-4.0)).right(px(-4.0)),
            IndicatorPosition::BottomStart => dot.bottom(px(-4.0)).left(px(-4.0)),
        };
        div()
            .relative()
            .child(self.child)
            .when(!self.disabled, |el| el.child(dot))
    }
}

type OnRemove = Box<dyn Fn(&mut Window, &mut App) + 'static>;

/// A compact rounded label, optionally removable.
#[derive(IntoElement)]
pub struct Pill {
    id: ElementId,
    label: SharedString,
    size: Size,
    on_remove: Option<OnRemove>,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            size: Size::Sm,
            on_remove: None,
        }
    }
    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }
    pub fn on_remove(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Pill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let dark = theme(cx).scheme == guiraffe_core::Scheme::Dark;
        let bg = if dark {
            s.hover
        } else {
            guiraffe_core::ColorName::Gray.shade(1)
        };
        let id = self.id.clone();
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(4.0))
            .h(px(self.size.control_height() * 0.6))
            .px(px(self.size.spacing() * 0.75))
            .rounded_full()
            .bg(hsla(bg))
            .text_color(hsla(s.text))
            .text_size(px(self.size.font_size() * 0.9))
            .child(self.label)
            .when_some(self.on_remove, move |el, f| {
                el.child(
                    crate::icons::close_button((id, "remove"))
                        .size(Size::Xs)
                        .on_click(move |_, w, cx| f(w, cx)),
                )
            })
    }
}
