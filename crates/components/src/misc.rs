//! Blockquote, Indicator and Pill.

use gpui_kit::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, Size, Variant};

use crate::style::{hsla, metrics, radius, refine, surfaces, variant};

#[derive(IntoElement, Default)]
pub struct Blockquote {
    color: Option<ColorName>,
    cite: Option<SharedString>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Blockquote {
    pub fn new() -> Self {
        Self::default()
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

crate::styled!(Blockquote, Indicator, Pill);

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
        let el = div()
            .flex()
            .flex_col()
            .gap(px(Size::Xs.spacing()))
            .p(px(Size::Md.spacing()))
            .bg(hsla(v.background))
            .text_color(hsla(s.text))
            .border_l_4()
            .border_color(hsla(accent))
            .rounded_r(radius(None, cx));
        refine(el, &self.style)
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
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    disabled: bool,
    style: StyleRefinement,
}

impl Indicator {
    pub fn new(child: impl IntoElement) -> Self {
        Self {
            child: child.into_any_element(),
            label: None,
            color: None,
            position: IndicatorPosition::TopEnd,
            size: None,
            disabled: false,
            style: StyleRefinement::default(),
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
}

crate::props!(Indicator { position: IndicatorPosition => position, size: Size => size, disabled: bool => disabled });

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let v = variant(cx, self.color, Variant::Filled);
        let ring = hsla(surfaces(cx).body);
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let (dot, badge) = (metrics::dot(size), metrics::badge_height(size));
        let offset = px(-(if self.label.is_some() { badge } else { dot }) / 3.0);
        let mark = match &self.label {
            Some(_) => div()
                .min_w(px(badge))
                .h(px(badge))
                .px(px(metrics::gap(size)))
                .text_size(px(Size::Xs.font_size() * 0.85)),
            None => div().size(px(dot)),
        }
        .debug_selector(|| "indicator-mark".into())
        .absolute()
        .rounded_full()
        .bg(hsla(v.background))
        .text_color(hsla(v.foreground))
        .border_2()
        .border_color(ring)
        .flex()
        .items_center()
        .justify_center()
        .when_some(self.label, |el, l| el.child(l));
        let mark = match self.position {
            IndicatorPosition::TopEnd => mark.top(offset).right(offset),
            IndicatorPosition::TopStart => mark.top(offset).left(offset),
            IndicatorPosition::BottomEnd => mark.bottom(offset).right(offset),
            IndicatorPosition::BottomStart => mark.bottom(offset).left(offset),
        };
        // Reserve the mark's overhang so it never overflows the wrapper and gets clipped by
        // an ancestor with `overflow_hidden` (e.g. a Paper). The child keeps its position
        // relative to the mark; the wrapper is just that much larger on the mark's sides.
        let over = -offset;
        let wrapper = match self.position {
            IndicatorPosition::TopEnd => div().pt(over).pr(over),
            IndicatorPosition::TopStart => div().pt(over).pl(over),
            IndicatorPosition::BottomEnd => div().pb(over).pr(over),
            IndicatorPosition::BottomStart => div().pb(over).pl(over),
        };
        let inner = div().relative().child(self.child);
        let inner = if self.disabled {
            inner
        } else {
            inner.child(mark)
        };
        refine(wrapper.flex_none(), &self.style).child(inner)
    }
}

type OnRemove = Box<dyn Fn(&ElementId, &mut Window, &mut App) + 'static>;

/// A compact rounded label, optionally removable.
#[derive(IntoElement)]
pub struct Pill {
    id: ElementId,
    label: SharedString,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    on_remove: Option<OnRemove>,
    style: StyleRefinement,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            size: None,
            on_remove: None,
            style: StyleRefinement::default(),
        }
    }
    /// Called with the pill's id when its × is clicked.
    pub fn on_remove(mut self, f: impl Fn(&ElementId, &mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Box::new(f));
        self
    }
}

crate::props!(Pill { size: Size => size });

impl RenderOnce for Pill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let s = surfaces(cx);
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let id = self.id.clone();
        let el = div()
            .flex()
            .items_center()
            .gap(px(metrics::gap(size)))
            .h(px(metrics::pill_height(size)))
            .px(px(size.spacing() * 0.75))
            .rounded_full()
            .bg(hsla(s.track))
            .text_color(hsla(s.text))
            .text_size(px(size.font_size()));
        let name = format!("Remove {}", self.label);
        refine(el, &self.style)
            .id(self.id)
            .child(self.label)
            .when_some(self.on_remove, move |el, f| {
                let pill = id.clone();
                el.child(
                    crate::icons::close_button((id, "remove"))
                        .size(Size::Xs)
                        .label(name)
                        .on_click(move |_, w, cx| f(&pill, w, cx)),
                )
            })
    }
}
