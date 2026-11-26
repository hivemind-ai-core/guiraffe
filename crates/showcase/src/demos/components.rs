//! guiraffe-components: typography, surfaces, icons, feedback.

use gpui_kit::{
    AnyView, App, AppContext, Entity, FontWeight, IntoElement, ParentElement, Styled, Window,
    assets::IconName, div, px,
};
use guiraffe_components::*;
use guiraffe_core::{ColorName, Size, Variant};

use super::{column, example};
use crate::catalog::{Entry, stateless};
use crate::events::EventLog;
use crate::knobs::{
    Knob, KnobValues, SIZE_NAMES, playground_with, playground_with_update, with_knobs,
};
use gpui_kit::AnyElement;

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
            name: "Accordion",
            summary: "Disclosure panels with configurable surfaces and chevrons.",
            covers: &["Accordion"],
            demo: accordion,
        },
        Entry {
            krate: "components",
            name: "ExpandingList",
            summary: "Elevated cards open in place with spring motion.",
            covers: &["ExpandingList"],
            demo: expanding_list,
        },
        Entry {
            krate: "components",
            name: "SpeedDial",
            summary: "Staggered actions in linear and radial arrangements.",
            covers: &["SpeedDial"],
            demo: speed_dial,
        },
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
            name: "StepGrid",
            summary: "Step sequencer cells: levels, accents, probability, ratchets, polymeter rows.",
            covers: &["StepGrid"],
            demo: step_grid,
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

fn typography(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Text",
        &[
            Knob::size(),
            Knob::color(),
            Knob::new("Weight", &["normal", "medium", "bold"]),
            Knob::toggle("Dimmed"),
            Knob::toggle("Truncate"),
        ],
        typography_live,
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
                    div().flex().flex_col().gap(px(4.)).children(
                        [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl]
                            .map(|s| Text::new(format!("{s:?}: the quick brown giraffe")).size(s)),
                    ),
                ),
            ])
        },
        window,
        cx,
    )
}

fn typography_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    let weight = v
        .pick(
            2,
            &[FontWeight::NORMAL, FontWeight::MEDIUM, FontWeight::BOLD],
        )
        .unwrap_or_default();
    let mut text = Text::new("Release 0.1 ships native charts, flow graphs and a data grid.")
        .size(v.size(0))
        .fw(weight)
        .dimmed(v.on(3))
        .truncate(v.on(4));
    if let Some(c) = v.color(1) {
        text = text.color(c);
    }
    div().w(px(360.)).child(text).into_any_element()
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

fn surfaces(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Paper",
        &[
            Knob::new("Shadow", SIZE_NAMES).default(2),
            Knob::radius(),
            Knob::new("Padding", SIZE_NAMES).default(3),
            Knob::toggle("Border"),
        ],
        surfaces_live,
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
                    "Paper p_0: flush content",
                    div().w(px(320.)).child(
                        Paper::new()
                            .with_border(true)
                            .p_0()
                            .child(div().h(px(56.)).bg(ColorName::Teal.shade(1).to_hsla())),
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
        window,
        cx,
    )
}

fn surfaces_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    div()
        .w(px(360.))
        .child(
            Paper::new()
                .shadow(v.size(0))
                .radius(v.size(1))
                .p(v.size(2))
                .with_border(v.on(3))
                .child(Title::new("Quarterly report").order(4))
                .child(
                    Text::new("Revenue grew 14% on strong renewals.")
                        .size(Size::Sm)
                        .dimmed(true),
                ),
        )
        .into_any_element()
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

fn icons(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live ThemeIcon & ActionIcon",
        &[
            Knob::variant(),
            Knob::color(),
            Knob::size(),
            Knob::radius(),
            Knob::toggle("Disabled"),
            Knob::toggle("Loading"),
        ],
        |_, cx| EventLog::new(cx),
        |log, v, window, cx| {
            column([
                icons_live(v, log, window, cx),
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
        window,
        cx,
    )
}

fn icons_live(v: &KnobValues, log: &Entity<EventLog>, _: &mut Window, _: &mut App) -> AnyElement {
    let (l1, l2) = (log.clone(), log.clone());
    let color = v.color(1).unwrap_or(ColorName::Blue);
    div()
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            ThemeIcon::new(IconName::Sparkles)
                .variant(v.variant(0))
                .color(color)
                .size(v.size(2))
                .radius(v.size(3)),
        )
        .child(
            ActionIcon::new("live-act", IconName::Heart)
                .variant(v.variant(0))
                .color(color)
                .size(v.size(2))
                .label("Like")
                .disabled(v.on(4))
                .loading(v.on(5))
                .on_click(move |_, _, cx| EventLog::log(&l1, "Like · on_click", cx)),
        )
        .child(
            ActionIcon::new("live-act-focus", IconName::Bookmark)
                .variant(v.variant(0))
                .color(color)
                .size(v.size(2))
                .label("Save (focusable: Tab reaches it)")
                .focusable(true)
                .on_click(move |_, _, cx| EventLog::log(&l2, "Save · on_click", cx)),
        )
        .child(div().ml(px(16.)).child(log.clone()))
        .into_any_element()
}

fn indicator(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live Indicator & Pill",
        &[
            Knob::color().default(4),
            Knob::new("Label", &["dot", "3", "99+"]).default(1),
        ],
        |_, cx| EventLog::new(cx),
        |log, v, _, _| {
            let mut ind = Indicator::new(ThemeIcon::new(IconName::Inbox).variant(Variant::Light))
                .color(v.color(0).unwrap_or(ColorName::Blue));
            if let Some(Some(l)) = v.pick(1, &[None, Some("3"), Some("99+")]) {
                ind = ind.label(l);
            }
            let pill = |id: &'static str, label: &'static str| {
                let log = log.clone();
                Pill::new(id, label).on_remove(move |id, _, cx| {
                    EventLog::log(&log, format!("on_remove {id:?}"), cx)
                })
            };
            column([
                div()
                    .flex()
                    .items_center()
                    .gap(px(32.))
                    .child(ind)
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(pill("rust", "Rust"))
                            .child(pill("gpui", "GPUI"))
                            .child(Pill::new("native", "Native (not removable)")),
                    )
                    .into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

/// A drum pattern: click to toggle, drag to paint, Alt-drag for level, arrows and Space.
struct StepDemo(Entity<StepGrid>);

impl gpui_kit::Render for StepDemo {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        column([example(
            "Click toggles, drag paints, Alt-drag sets the level; Hat repeats every 6 steps",
            self.0.clone(),
        )])
    }
}

fn step_grid(_: &mut Window, cx: &mut App) -> AnyView {
    use guiraffe_components::step_grid::{StepDoc, step_store};
    let rows = vec![
        StepRow::new(1, "Kick", 16)
            .with_color(ColorName::Red)
            .with_cell(0, StepCell::on())
            .with_cell(4, StepCell::on().with_level(0.7))
            .with_cell(8, StepCell::on().flagged())
            .with_cell(12, StepCell::on().with_ratchet(2)),
        StepRow::new(2, "Snare", 16)
            .with_color(ColorName::Blue)
            .with_cell(4, StepCell::on())
            .with_cell(12, StepCell::on())
            .with_cell(15, StepCell::on().with_probability(0.4)),
        StepRow::new(3, "Hat", 6)
            .with_color(ColorName::Yellow)
            .with_cell(0, StepCell::on().with_level(0.5))
            .with_cell(2, StepCell::on())
            .with_cell(4, StepCell::on().with_level(0.3)),
        StepRow::new(4, "Clap", 16)
            .with_color(ColorName::Grape)
            .with_cell(14, StepCell::on().with_ratchet(4)),
    ];
    let store = step_store(StepDoc::new(16, 4, rows), 100, cx);
    let grid = StepGrid::new(store, StepGridOptions::default(), cx);
    grid.read(cx).playhead().set(Some(6));
    cx.new(|_| StepDemo(grid)).into()
}

fn ring(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live RingProgress",
        &[
            Knob::new("Value", &["10%", "40%", "75%", "100%"]).default(2),
            Knob::size(),
            Knob::new("Thickness", &["thin", "normal", "thick"]).default(1),
            Knob::color(),
        ],
        ring_live,
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
        window,
        cx,
    )
}

fn ring_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    let value = v.pick(0, &[10., 40., 75., 100.]).unwrap_or(75.);
    let diameter = v.pick(1, &[60., 80., 110., 140., 180.]).unwrap_or(110.);
    let thickness = v.pick(2, &[4., 10., 18.]).unwrap_or(10.);
    RingProgress::new()
        .diameter(diameter)
        .thickness(thickness)
        .section(value, v.color(3).unwrap_or(ColorName::Blue))
        .label(format!("{value:.0}%"))
        .into_any_element()
}

fn segmented(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live SegmentedControl",
        &[
            Knob::size(),
            Knob::toggle("Full width"),
            Knob::toggle("Disabled"),
        ],
        |_, cx| {
            let control = SegmentedControl::new(["Preview", "Code", "Export"], cx);
            let log = EventLog::new(cx);
            EventLog::watch::<SegmentedEvent, _>(&log, &control, cx);
            (control, log)
        },
        |(control, _), v, cx| {
            let (size, full, disabled) = (v.size(0), v.on(1), v.on(2));
            control.update(cx, |c, cx| {
                c.set_size(Some(size), cx);
                c.set_full_width(full, cx);
                c.set_disabled(disabled, cx);
            });
        },
        |(control, log), _, _, _| {
            column([
                div().w(px(420.)).child(control.clone()).into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

fn disclosure_items() -> Vec<DisclosureItem> {
    [
        (
            "Workspace invitations",
            "Invite teammates from the workspace settings.",
        ),
        (
            "Notification preferences",
            "Choose which updates to receive in your inbox.",
        ),
        (
            "Export your data",
            "Download an archive of your workspace from settings.",
        ),
    ]
    .into_iter()
    .map(|(title, body)| DisclosureItem::new(title, move || Text::new(body).into_any_element()))
    .collect()
}

fn accordion(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Accordion",
        &[
            Knob::new("Variant", &["Contained", "Separated", "Filled"]),
            Knob::new("Chevron", &["End", "Start"]),
            Knob::toggle("Multiple"),
        ],
        |_, cx| {
            let control =
                Accordion::new(disclosure_items(), AccordionOptions::default(), vec![0], cx);
            let log = EventLog::new(cx);
            EventLog::watch::<AccordionChange, _>(&log, &control, cx);
            (control, log)
        },
        |(control, _), v, cx| {
            let variant = v
                .pick(
                    0,
                    &[
                        AccordionVariant::Contained,
                        AccordionVariant::Separated,
                        AccordionVariant::Filled,
                    ],
                )
                .unwrap_or_default();
            let chevron = v
                .pick(1, &[ChevronPosition::End, ChevronPosition::Start])
                .unwrap_or_default();
            control.update(cx, |control, cx| {
                control.set_options(
                    AccordionOptions {
                        variant,
                        chevron,
                        multiple: v.on(2),
                        focusable: Some(true),
                    },
                    cx,
                )
            });
        },
        |(control, log), _, _, _| {
            column([
                div().w(px(420.)).child(control.clone()).into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

fn expanding_list(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "ExpandingList",
        &[Knob::toggle("Keyboard navigation")],
        |_, cx| {
            let control = ExpandingList::new(disclosure_items(), px(72.), cx);
            let log = EventLog::new(cx);
            EventLog::watch::<ExpandingListChange, _>(&log, &control, cx);
            (control, log)
        },
        |(control, _), v, cx| {
            control.update(cx, |control, cx| control.set_focusable(Some(v.on(0)), cx));
        },
        |(control, log), _, _, _| {
            column([
                div().w(px(420.)).child(control.clone()).into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

fn speed_dial(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "SpeedDial",
        &[
            Knob::new(
                "Layout",
                &["Vertical", "Horizontal", "Quarter circle", "Half circle"],
            ),
            Knob::new("Radius", &["80 px", "120 px"]),
            Knob::toggle("Show labels"),
        ],
        |_, cx| {
            let actions = [
                (IconName::Search, "Search"),
                (IconName::Heart, "Favorite"),
                (IconName::Check, "Done"),
            ]
            .into_iter()
            .map(|(icon, label)| DialAction {
                icon,
                label: label.into(),
            })
            .collect();
            let control = SpeedDial::new(
                IconName::Plus,
                "Quick actions",
                actions,
                DialLayout::Vertical { spacing: 48. },
                cx,
            );
            let log = EventLog::new(cx);
            EventLog::watch::<SpeedDialEvent, _>(&log, &control, cx);
            (control, log)
        },
        |(control, _), v, cx| {
            let radius = v.pick(1, &[80., 120.]).unwrap_or(80.);
            let layout = match v.index(0) {
                1 => DialLayout::Horizontal { spacing: 48. },
                2 => DialLayout::Radial {
                    radius,
                    start_degrees: -90.,
                    arc: DialArc::Quarter,
                    clockwise: true,
                },
                3 => DialLayout::Radial {
                    radius,
                    start_degrees: -180.,
                    arc: DialArc::Half,
                    clockwise: true,
                },
                _ => DialLayout::Vertical { spacing: 48. },
            };
            control.update(cx, |control, cx| {
                control.set_focusable(Some(true), cx);
                control.set_layout(layout, cx);
                control.set_show_labels(v.on(2), cx);
            });
        },
        |(control, log), _, _, _| {
            column([
                div()
                    .w(px(410.))
                    .h(px(190.))
                    .flex()
                    .items_end()
                    .pl(px(128.))
                    .pb_2()
                    .child(control.clone())
                    .into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}
