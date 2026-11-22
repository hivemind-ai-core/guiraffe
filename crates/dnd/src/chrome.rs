//! Shared drag feedback: the pointer preview and the drop indicator. Every color, radius and
//! shadow comes from the theme (spec/00 §12.1).

use gpui_kit::{
    App, Bounds, BoxShadow, Context, CursorStyle, IntoElement, ParentElement, Pixels, Point,
    Render, SharedString, Styled, Window, canvas, div, point, px,
};
use guiraffe_core::{GuiraffeTheme, Rgb, Size};
use std::{cell::Cell, rc::Rc};

/// GPUI box shadows for a theme shadow token (scheme-scaled strength).
pub(crate) fn shadows(theme: &GuiraffeTheme, size: Size) -> Vec<BoxShadow> {
    theme
        .shadow(size)
        .iter()
        .map(|layer| BoxShadow {
            color: Rgb::BLACK.alpha(layer.alpha).to_hsla(),
            offset: point(px(0.), px(layer.y)),
            blur_radius: px(layer.blur),
            spread_radius: px(layer.spread),
            inset: false,
        })
        .collect()
}

/// The theme's default control radius.
pub(crate) fn radius(theme: &GuiraffeTheme) -> Pixels {
    px(theme.default_radius.radius())
}

/// The lifted item following the pointer: an elevated card (theme `Md` shadow) painted with
/// the translucent `drag_preview` token, so what lies beneath stays visible while moving and
/// the primary outline marks the ghost as "being moved" (not translucency alone).
pub(crate) struct Preview {
    pub(crate) label: SharedString,
    pub(crate) position: Point<Pixels>,
    pub(crate) origin: Rc<Cell<Point<Pixels>>>,
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let ghost = theme.drag_preview();
        let offset = self.position - self.origin.get();
        div()
            .absolute()
            .left(offset.x + px(8.))
            .top(offset.y + px(8.))
            .px_3()
            .py_2()
            .rounded(radius(theme))
            .border_1()
            .border_color(ghost.border.to_hsla())
            .bg(ghost.background.to_hsla())
            .text_color(ghost.foreground.to_hsla())
            .shadow(shadows(theme, Size::Md))
            .child(self.label.clone())
    }
}

/// How a drop indicator is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IndicatorStyle {
    /// A 2px insertion line between items.
    Line,
    /// An outlined region (the active drop column, a split or dock placeholder). It has no
    /// fill: it is drawn over the content, which must stay readable.
    Zone,
}

pub(crate) type Indicator = (Bounds<Pixels>, IndicatorStyle);

/// Drop feedback drawn in the drag overlay: zones first, lines on top.
pub(crate) struct DropOutline {
    pub(crate) indicators: Vec<Indicator>,
    /// Cursor for the gesture in progress. Set here rather than by the overlay so a gesture
    /// that starts without other feedback re-renders only this small view.
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) origin: Rc<Cell<Point<Pixels>>>,
}

impl DropOutline {
    pub(crate) fn new(origin: Rc<Cell<Point<Pixels>>>) -> Self {
        Self {
            indicators: Vec::new(),
            cursor: None,
            origin,
        }
    }
}

impl Render for DropOutline {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let primary = theme.primary_color().to_hsla();
        let corner = radius(theme);
        let origin = self.origin.get();
        let cursor = self.cursor;
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(cursor.map(|cursor| {
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| window.set_window_cursor_style(cursor),
                )
                .size_0()
            }))
            .children(self.indicators.iter().map(|&(bounds, style)| {
                let offset = bounds.origin - origin;
                let el = div()
                    .absolute()
                    .left(offset.x)
                    .top(offset.y)
                    .w(bounds.size.width)
                    .h(bounds.size.height);
                match style {
                    IndicatorStyle::Line => el.rounded_full().bg(primary),
                    IndicatorStyle::Zone => el.rounded(corner).border(px(2.)).border_color(primary),
                }
            }))
    }
}

/// Update `outline` only when the indicators actually change (pointer moves stay cheap).
pub(crate) fn set_indicators(
    outline: &gpui_kit::Entity<DropOutline>,
    indicators: Vec<Indicator>,
    cursor: Option<CursorStyle>,
    cx: &mut App,
) {
    let current = outline.read(cx);
    if current.indicators != indicators || current.cursor != cursor {
        outline.update(cx, |outline, cx| {
            outline.indicators = indicators;
            outline.cursor = cursor;
            cx.notify();
        });
    }
}
