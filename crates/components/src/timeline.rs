//! Timeline: a vertical list of events with bullets and connector lines.
//!
//! Items at or before the *active* index are highlighted (filled bullet, coloured line);
//! the active item's line stops at the bullet, so the progress reads as "here".

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, StyleRefinement, Window,
    div, prelude::*, px,
};
use guiraffe_core::{ColorName, Size, Variant};

use crate::style::{hsla, metrics, surfaces, theme, variant};

/// Where an item sits relative to the timeline's active index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Before the active index: done.
    Completed,
    /// At the active index.
    Active,
    /// After the active index.
    Upcoming,
}

/// State of `index` for a timeline whose active item is `active`.
pub fn item_state(index: usize, active: usize) -> ItemState {
    match index.cmp(&active) {
        std::cmp::Ordering::Less => ItemState::Completed,
        std::cmp::Ordering::Equal => ItemState::Active,
        std::cmp::Ordering::Greater => ItemState::Upcoming,
    }
}

/// Whether the connector above item `index` is coloured: everything up to the active item.
pub fn line_reached(index: usize, active: usize) -> bool {
    index <= active
}

/// Whether the bullet of item `index` is filled.
pub fn bullet_filled(index: usize, active: usize) -> bool {
    index <= active
}

/// One event in a [`Timeline`].
#[derive(IntoElement)]
pub struct TimelineItem {
    title: SharedString,
    description: Option<SharedString>,
    color: Option<ColorName>,
    bullet: Option<AnyElement>,
    style: StyleRefinement,
}

impl TimelineItem {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            color: None,
            bullet: None,
            style: StyleRefinement::default(),
        }
    }

    /// Secondary line under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Accent for this item's bullet and the connector above it.
    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }

    /// Replace the bullet dot with custom content (an icon, an avatar, a label).
    pub fn bullet(mut self, bullet: impl IntoElement) -> Self {
        self.bullet = Some(bullet.into_any_element());
        self
    }
}

crate::styled!(TimelineItem);

impl RenderOnce for TimelineItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        // Items are only meaningful inside a `Timeline`; render the content column alone.
        let el = div()
            .flex()
            .flex_col()
            .child(self.title)
            .when_some(self.description, |el, d| el.child(d));
        crate::style::refine(el, &self.style)
    }
}

/// A vertical timeline of [`TimelineItem`]s; items up to `active` are complete.
#[derive(IntoElement)]
pub struct Timeline {
    items: Vec<TimelineItem>,
    active: usize,
    color: Option<ColorName>,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    bullet_size: Option<f32>,
    line_width: Option<f32>,
    style: StyleRefinement,
}

impl Timeline {
    pub fn new(items: impl IntoIterator<Item = TimelineItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
            active: 0,
            color: None,
            size: None,
            bullet_size: None,
            line_width: None,
            style: StyleRefinement::default(),
        }
    }

    /// Index of the current item; later items are "upcoming". Clamped at render time.
    pub fn active(mut self, active: usize) -> Self {
        self.active = active;
        self
    }

    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }

    /// Bullet diameter in px (default from the size scale).
    pub fn bullet_size(mut self, px: f32) -> Self {
        self.bullet_size = Some(px.max(1.0));
        self
    }

    /// Connector width in px (default from the size scale).
    pub fn line_width(mut self, px: f32) -> Self {
        self.line_width = Some(px.max(1.0));
        self
    }
}

crate::props!(Timeline { size: Size => size });
crate::styled!(Timeline);

impl RenderOnce for Timeline {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let s = surfaces(cx);
        let scale = theme(cx).scale;
        let bullet = self.bullet_size.unwrap_or(metrics::bullet(size));
        let line = self.line_width.unwrap_or(metrics::line_width(size));
        let count = self.items.len();
        let active = if count == 0 {
            0
        } else {
            self.active.min(count - 1)
        };
        let accent =
            |c: Option<ColorName>| variant(cx, c.or(self.color), Variant::Filled).background;
        let fs = size.font_size() * scale;
        let title_gap = px(size.spacing() / 4.0);
        let row_gap = px(size.spacing() * 0.75);
        let bullet_gap = px(size.spacing() / 2.0);
        let rows = self.items.into_iter().enumerate().map(|(i, item)| {
            let color = accent(item.color);
            let filled = bullet_filled(i, active);
            let reached = line_reached(i, active);
            let is_last = i + 1 == count;
            let bullet_el = match item.bullet {
                Some(custom) => div()
                    .size(px(bullet))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(custom),
                None => div()
                    .size(px(bullet))
                    .rounded_full()
                    .border_2()
                    .border_color(hsla(if filled { color } else { s.border }))
                    .bg(hsla(if filled { color } else { s.body })),
            };
            let content = div()
                .flex()
                .flex_col()
                .gap(title_gap)
                .pb(bullet_gap)
                .text_size(px(fs))
                .text_color(hsla(s.text))
                .child(
                    div()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(item.title),
                )
                .when_some(item.description, |el, d| {
                    el.child(
                        div()
                            .text_size(px(Size::Sm.font_size() * scale))
                            .text_color(hsla(s.dimmed))
                            .child(d),
                    )
                });
            div()
                .flex()
                .items_stretch()
                .gap(row_gap)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(bullet_el)
                        .when(!is_last, |el| {
                            el.child(div().w(px(line)).flex_1().bg(hsla(if reached {
                                color
                            } else {
                                s.border
                            })))
                        }),
                )
                .child(content.flex_1())
        });
        crate::style::refine(div().flex().flex_col(), &self.style).children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{ItemState, bullet_filled, item_state, line_reached};

    #[test]
    fn states_split_around_the_active_index() {
        assert_eq!(item_state(0, 1), ItemState::Completed);
        assert_eq!(item_state(1, 1), ItemState::Active);
        assert_eq!(item_state(2, 1), ItemState::Upcoming);
    }

    #[test]
    fn colouring_stops_after_the_active_item() {
        assert!(line_reached(0, 1) && line_reached(1, 1));
        assert!(!line_reached(2, 1));
        assert!(bullet_filled(1, 1));
        assert!(!bullet_filled(2, 1));
    }

    #[test]
    fn empty_or_out_of_range_active_is_safe() {
        // The renderer clamps; the pure predicates must simply not panic at the edges.
        assert_eq!(item_state(0, usize::MAX), ItemState::Completed);
        assert!(line_reached(0, usize::MAX));
    }
}
