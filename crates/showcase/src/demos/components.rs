//! guiraffe-components: typography, surfaces, icons, feedback.

use gpui_kit::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window,
    assets::IconName, div, px,
};
use guiraffe_components::*;
use guiraffe_core::{ColorName, Size, Variant};

use super::{column, example};
use crate::catalog::{Entry, stateless};

const VARIANTS: [Variant; 5] = [
    Variant::Filled,
    Variant::Light,
    Variant::Outline,
    Variant::Subtle,
    Variant::Default,
];

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "components",
            name: "Typography",
            summary: "Text and Title follow the theme's type scale; sizes come from context unless overridden.",
            covers: &["Text", "Title"],
            demo: typography,
        },
        Entry {
            krate: "components",
            name: "Highlight & Mark",
            summary: "Highlight search terms inside text; Mark a span.",
            covers: &["Highlight", "Mark"],
            demo: highlight,
        },
        Entry {
            krate: "components",
            name: "Paper & Card",
            summary: "Surfaces with theme shadows, radius and borders; Card sections bleed to the edge.",
            covers: &["Paper", "Card", "CardSection"],
            demo: surfaces,
        },
        Entry {
            krate: "components",
            name: "Blockquote",
            summary: "Quoted text with an accent bar and optional citation.",
            covers: &["Blockquote"],
            demo: blockquote,
        },
        Entry {
            krate: "components",
            name: "ThemeIcon & ActionIcon",
            summary: "Icon tiles and icon buttons in every variant and color.",
            covers: &["ThemeIcon", "ActionIcon"],
            demo: icons,
        },
        Entry {
            krate: "components",
            name: "Indicator & Pill",
            summary: "Badges pinned to a corner; removable pills.",
            covers: &["Indicator", "Pill"],
            demo: indicator,
        },
        Entry {
            krate: "components",
            name: "RingProgress",
            summary: "Multi-section ring with a center label.",
            covers: &["RingProgress"],
            demo: ring,
        },
        Entry {
            krate: "components",
            name: "SegmentedControl",
            summary: "Animated single choice; keyboard and pointer.",
            covers: &["SegmentedControl"],
            demo: segmented,
        },
    ]
}

fn typography(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Title orders 1–6",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .children((1..=6).map(|o| Title::new(format!("Title order {o}")).order(o))),
                ),
                example(
                    "Text sizes, color, dimmed",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .children(
                            [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl].map(|s| {
                                Text::new(format!("{s:?}: the quick brown giraffe")).size(s)
                            }),
                        )
                        .child(Text::new("Colored text").color(ColorName::Grape))
                        .child(Text::new("Dimmed secondary text").dimmed(true)),
                ),
                example(
                    "Truncate",
                    div().w(px(220.)).child(
                        Text::new("A very long line that is truncated rather than wrapped")
                            .truncate(true),
                    ),
                ),
            ])
        },
        cx,
    )
}

fn highlight(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Highlight terms",
                    Highlight::new("Fast, fluid and beautiful native interfaces")
                        .term("fluid")
                        .term("native"),
                ),
                example(
                    "Mark",
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(Text::new("Press"))
                        .child(Mark::new("Ctrl+K"))
                        .child(Text::new("to search")),
                ),
            ])
        },
        cx,
    )
}

fn surfaces(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Paper shadows",
                    div().flex().gap(px(16.)).children(
                        [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl].map(|s| {
                            Paper::new()
                                .shadow(s)
                                .p(Size::Md)
                                .child(Text::new(format!("{s:?}")).size(Size::Sm))
                        }),
                    ),
                ),
                example(
                    "Card with sections",
                    div().w(px(320.)).child(
                        Card::new()
                            .with_border(true)
                            .section(
                                CardSection::new()
                                    .with_border(true)
                                    .child(div().h(px(80.)).bg(ColorName::Blue.shade(1).to_hsla())),
                            )
                            .child(Title::new("Norway fjord adventures").order(4))
                            .child(
                                Text::new("Explore the fjords with a guided tour.")
                                    .size(Size::Sm)
                                    .dimmed(true),
                            ),
                    ),
                ),
            ])
        },
        cx,
    )
}

fn blockquote(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([example(
                "With citation",
                Blockquote::new().color(ColorName::Indigo).cite("– Antoine de Saint-Exupéry").child(Text::new("Perfection is achieved not when there is nothing more to add, but when there is nothing left to take away.")),
            )])
        },
        cx,
    )
}

fn icons(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Variants",
                    div()
                        .flex()
                        .gap(px(12.))
                        .children(VARIANTS.iter().enumerate().map(|(i, v)| {
                            div()
                                .flex()
                                .gap(px(4.))
                                .child(
                                    ThemeIcon::new(IconName::Sparkles)
                                        .variant(*v)
                                        .color(ColorName::Indigo),
                                )
                                .child(
                                    ActionIcon::new(("act", i), IconName::Heart)
                                        .variant(*v)
                                        .color(ColorName::Pink),
                                )
                        })),
                ),
                example(
                    "Sizes",
                    div().flex().items_center().gap(px(12.)).children(
                        [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl].map(|s| {
                            ThemeIcon::new(IconName::Star)
                                .size(s)
                                .color(ColorName::Yellow)
                        }),
                    ),
                ),
                example("Close button", close_button("close")),
            ])
        },
        cx,
    )
}

fn indicator(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Indicator",
                    div()
                        .flex()
                        .gap(px(24.))
                        .child(
                            Indicator::new(ThemeIcon::new(IconName::Mail).variant(Variant::Light))
                                .label("12")
                                .color(ColorName::Red),
                        )
                        .child(
                            Indicator::new(ThemeIcon::new(IconName::Bell).variant(Variant::Light))
                                .color(ColorName::Teal),
                        ),
                ),
                example(
                    "Pills",
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(Pill::new("p1", "Rust").on_remove(|_, _, _| {}))
                        .child(Pill::new("p2", "GPUI"))
                        .child(Pill::new("p3", "Native")),
                ),
            ])
        },
        cx,
    )
}

fn ring(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([example(
                "Sections and label",
                div()
                    .flex()
                    .gap(px(24.))
                    .child(
                        RingProgress::new()
                            .diameter(110.)
                            .section(30., ColorName::Cyan)
                            .section(25., ColorName::Pink)
                            .section(15., ColorName::Orange)
                            .label("70%"),
                    )
                    .child(
                        RingProgress::new()
                            .diameter(80.)
                            .thickness(6.)
                            .section(92., ColorName::Teal)
                            .label("92%"),
                    ),
            )])
        },
        cx,
    )
}

struct SegmentedDemo(gpui_kit::Entity<SegmentedControl>);

impl Render for SegmentedDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([example(
            "Pick a view",
            div().w(px(360.)).child(self.0.clone()),
        )])
    }
}

fn segmented(_: &mut Window, cx: &mut App) -> AnyView {
    let control = SegmentedControl::new(["Preview", "Code", "Export"], cx);
    cx.new(|_| SegmentedDemo(control)).into()
}
