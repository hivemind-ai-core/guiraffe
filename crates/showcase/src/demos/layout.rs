//! guiraffe-layout: placement primitives. Spacing comes from Size tokens and context, never
//! from per-child margins.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, prelude::*, px,
};
use guiraffe_components::Text;
use guiraffe_core::{ColorName, Size};
use guiraffe_layout::*;

use super::{column, example, tile};
use crate::catalog::{Entry, stateless};
use crate::events::EventLog;
use crate::knobs::{Knob, KnobValues, SIZE_NAMES, with_knobs};
use gpui_kit::AnyElement;

const ALIGNS: [Align; 4] = [Align::Start, Align::Center, Align::End, Align::Stretch];
const ALIGN_NAMES: &[&str] = &["start", "center", "end", "stretch"];
const JUSTIFIES: [Justify; 5] = [
    Justify::Start,
    Justify::Center,
    Justify::End,
    Justify::Between,
    Justify::Around,
];
const JUSTIFY_NAMES: &[&str] = &["start", "center", "end", "between", "around"];

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "layout",
            name: "Stack & Group",
            summary: "Vertical and horizontal runs with token gaps, alignment and justification.",
            covers: &["Stack", "Group"],
            demo: stack_group,
        },
        Entry {
            krate: "layout",
            name: "Flex",
            summary: "Full flexbox control: direction, wrap, grow.",
            covers: &["Flex"],
            demo: flex,
        },
        Entry {
            krate: "layout",
            name: "Grid",
            summary: "12-column responsive grid; spans and offsets per breakpoint, measured against the container.",
            covers: &["Grid", "Col"],
            demo: grid,
        },
        Entry {
            krate: "layout",
            name: "SimpleGrid",
            summary: "Equal columns, or as many as fit a minimum child width.",
            covers: &["SimpleGrid"],
            demo: simple_grid,
        },
        Entry {
            krate: "layout",
            name: "Container & Center",
            summary: "Max-width content column; centering.",
            covers: &["Container", "Center"],
            demo: container,
        },
        Entry {
            krate: "layout",
            name: "AspectRatio",
            summary: "Keeps a child at a fixed ratio.",
            covers: &["AspectRatio"],
            demo: aspect,
        },
        Entry {
            krate: "layout",
            name: "Divider & Space",
            summary: "Rules with labels; fixed spacers.",
            covers: &["Divider", "Space"],
            demo: divider,
        },
        Entry {
            krate: "layout",
            name: "ScrollArea",
            summary: "Scrolling region on either or both axes.",
            covers: &["ScrollArea"],
            demo: scroll,
        },
        Entry {
            krate: "layout",
            name: "Splitter",
            summary: "Resizable panes with min/max sizes and persisted layout.",
            covers: &["Splitter"],
            demo: splitter,
        },
        Entry {
            krate: "layout",
            name: "AppShell",
            summary: "Header, navbar, main, aside and footer; collapsible and resizable.",
            covers: &["AppShell"],
            demo: app_shell,
        },
    ]
}

fn stack_group(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Group",
        &[
            Knob::gap(),
            Knob::new("Align", ALIGN_NAMES),
            Knob::new("Justify", JUSTIFY_NAMES),
            Knob::toggle("Grow"),
            Knob::toggle("Wrap"),
        ],
        stack_group_live,
        |_, cx| {
            column([
                example(
                    "Stack gap=Md · Stack align=Center, gap_px(4)",
                    div()
                        .flex()
                        .gap(px(24.))
                        .child(
                            div().w(px(220.)).child(
                                Stack::new()
                                    .gap(Size::Md)
                                    .children((1..=3).map(|i| tile(format!("Item {i}"), cx))),
                            ),
                        )
                        .child(
                            div().w(px(220.)).child(
                                Stack::new().align(Align::Center).gap_px(4.).children(
                                    ["Narrow", "A wider row", "Mid"].map(|l| tile(l, cx)),
                                ),
                            ),
                        ),
                ),
                example(
                    "Group justify=Between",
                    Group::new()
                        .justify(Justify::Between)
                        .children(["Left", "Center", "Right"].map(|l| tile(l, cx))),
                ),
                example(
                    "Group grow",
                    Group::new()
                        .grow(true)
                        .children(["One", "Two", "Three"].map(|l| tile(l, cx))),
                ),
            ])
        },
        window,
        cx,
    )
}

fn stack_group_live(v: &KnobValues, _: &mut Window, cx: &mut App) -> AnyElement {
    Group::new()
        .gap(v.size(0))
        .align(v.pick(1, &ALIGNS).unwrap_or_default())
        .justify(v.pick(2, &JUSTIFIES).unwrap_or_default())
        .grow(v.on(3))
        .wrap(v.on(4))
        .child(tile("Short", cx))
        .child(tile("Taller", cx).min_h(px(64.)))
        .child(tile("Medium width item", cx))
        .children(["Tags", "Filters", "Sort by date", "Export"].map(|l| tile(l, cx)))
        .into_any_element()
}

fn flex(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Flex",
        &[
            Knob::new("Direction", &["row", "row-rev", "column", "col-rev"]),
            Knob::toggle("Wrap").default(1),
            Knob::gap(),
            Knob::new("Align", ALIGN_NAMES),
            Knob::new("Justify", JUSTIFY_NAMES),
            Knob::toggle("Grow"),
        ],
        flex_live,
        |_, cx| {
            column([
                example(
                    "Row, wrap, gap=Sm",
                    Flex::new()
                        .wrap(true)
                        .gap(Size::Sm)
                        .children((1..=10).map(|i| tile(format!("Chip {i}"), cx).w(px(90.)))),
                ),
                example(
                    "RowReverse",
                    Flex::new()
                        .direction(FlexDirection::RowReverse)
                        .gap(Size::Sm)
                        .children(["1", "2", "3"].map(|l| tile(l, cx).w(px(60.)))),
                ),
            ])
        },
        window,
        cx,
    )
}

fn flex_live(v: &KnobValues, _: &mut Window, cx: &mut App) -> AnyElement {
    let direction = v
        .pick(
            0,
            &[
                FlexDirection::Row,
                FlexDirection::RowReverse,
                FlexDirection::Column,
                FlexDirection::ColumnReverse,
            ],
        )
        .unwrap_or_default();
    Flex::new()
        .h(px(130.))
        .direction(direction)
        .wrap(v.on(1))
        .gap(v.size(2))
        .align(v.pick(3, &ALIGNS).unwrap_or_default())
        .justify(v.pick(4, &JUSTIFIES).unwrap_or_default())
        .grow(v.on(5))
        .children(
            ["Design", "Build", "Review", "Ship it", "Measure", "Learn"]
                .iter()
                .map(|l| tile(*l, cx).min_w(px(110.))),
        )
        .into_any_element()
}

fn grid(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Grid",
        &[
            Knob::new("Columns", &["4", "6", "12"]).default(2),
            Knob::new("Gutter", SIZE_NAMES).default(2),
            Knob::new("Span", &["1", "2", "3", "4", "6"]).default(3),
            Knob::toggle("Grow"),
        ],
        grid_live,
        |_, cx| {
            column([
                example(
                    "Spans 4 / 4 / 4",
                    Grid::new().children(
                        (1..=3).map(|i| Col::span(4).child(tile(format!("span 4 · {i}"), cx))),
                    ),
                ),
                example(
                    "Spans 8 / 4, offset",
                    Grid::new()
                        .child(Col::span(8).child(tile("span 8", cx)))
                        .child(Col::span(4).child(tile("span 4", cx)))
                        .child(Col::span(4).offset(4).child(tile("span 4, offset 4", cx))),
                ),
            ])
        },
        window,
        cx,
    )
}

fn grid_live(v: &KnobValues, _: &mut Window, cx: &mut App) -> AnyElement {
    let columns = v.pick(0, &[4u16, 6, 12]).unwrap_or(12);
    let span = v.pick(2, &[1u16, 2, 3, 4, 6]).unwrap_or(4);
    Grid::new()
        .columns(columns)
        .gutter(v.size(1))
        .grow(v.on(3))
        .children((1..=5).map(|i| Col::span(span).child(tile(format!("span {span} · {i}"), cx))))
        .into_any_element()
}

fn simple_grid(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live SimpleGrid",
        &[
            Knob::new("Cols", &["1", "2", "3", "4", "6"]).default(2),
            Knob::new("Spacing", SIZE_NAMES).default(2),
            Knob::new("Vertical", &["same", "xs", "xl"]),
        ],
        simple_grid_live,
        |_, cx| {
            column([
                example(
                    "Responsive cols: 1 → 2 at Xs → 4 at Sm (resize the window)",
                    SimpleGrid::new()
                        .cols(
                            Responsive::base(1u16)
                                .at(Breakpoint::Xs, 2)
                                .at(Breakpoint::Sm, 4),
                        )
                        .spacing(
                            Responsive::base(Size::Xs.spacing())
                                .at(Breakpoint::Sm, Size::Lg.spacing()),
                        )
                        .children(["Revenue", "Users", "Churn", "NPS"].map(|l| tile(l, cx))),
                ),
                example(
                    "min child width 140px",
                    SimpleGrid::new()
                        .min_child_width(px(140.))
                        .children((1..=8).map(|i| tile(format!("Card {i}"), cx))),
                ),
            ])
        },
        window,
        cx,
    )
}

fn simple_grid_live(v: &KnobValues, _: &mut Window, cx: &mut App) -> AnyElement {
    let mut grid = SimpleGrid::new()
        .cols(v.pick(0, &[1u16, 2, 3, 4, 6]).unwrap_or(3))
        .spacing(v.size(1));
    if let Some(Some(vs)) = v.pick(2, &[None, Some(Size::Xs), Some(Size::Xl)]) {
        grid = grid.vertical_spacing(vs);
    }
    grid.children((1..=6).map(|i| tile(format!("Cell {i}"), cx)))
        .into_any_element()
}

fn container(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Container",
        &[
            Knob::size().default(0),
            Knob::toggle("Fluid"),
            Knob::new("Padding", SIZE_NAMES).default(2),
        ],
        container_live,
        |_, cx| {
            column([example(
                "Center min_height=120",
                Center::new()
                    .min_height(px(120.))
                    .child(tile("centered", cx).w(px(140.))),
            )])
        },
        window,
        cx,
    )
}

fn container_live(v: &KnobValues, _: &mut Window, cx: &mut App) -> AnyElement {
    Container::new()
        .size(v.size(0))
        .fluid(v.on(1))
        .padding(v.size(2))
        .child(tile("max-width content column", cx))
        .into_any_element()
}

fn aspect(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live AspectRatio",
        &[Knob::new("Ratio", &["1:1", "4:3", "16:9", "21:9"]).default(2)],
        |v, _, cx| {
            let (label, ratio) = v
                .pick(
                    0,
                    &[
                        ("1:1", 1.),
                        ("4:3", 4. / 3.),
                        ("16:9", 16. / 9.),
                        ("21:9", 21. / 9.),
                    ],
                )
                .unwrap_or(("16:9", 16. / 9.));
            div()
                .w(px(300.))
                .child(
                    AspectRatio::new(1.)
                        .ratio(ratio)
                        .child(tile(label, cx).size_full()),
                )
                .into_any_element()
        },
        |_, _| column([]),
        window,
        cx,
    )
}

fn divider(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Divider",
        &[
            Knob::new("Label", &["none", "start", "center", "end"]).default(2),
            Knob::new("Thickness", &["1", "2", "4"]),
            Knob::new("Color", &["default", "teal", "red"]),
        ],
        divider_live,
        |_, cx| {
            column([example(
                "Vertical, orientation() and Space",
                Group::new()
                    .child(tile("A", cx))
                    .child(Divider::vertical())
                    .child(tile("B", cx))
                    .child(
                        Divider::new()
                            .orientation(DividerOrientation::Vertical)
                            .thickness(2.),
                    )
                    .child(tile("C", cx))
                    .child(Space::w(Size::Xl))
                    .child(tile("after Space::w(Xl)", cx))
                    .child(Space::new().width_px(48.).height(Size::Md))
                    .child(tile("after Space 48px", cx)),
            )])
        },
        window,
        cx,
    )
}

fn divider_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    let mut d = Divider::new().thickness(v.pick(1, &[1., 2., 4.]).unwrap_or(1.));
    if let Some(pos) = v
        .pick(
            0,
            &[
                None,
                Some(LabelPosition::Start),
                Some(LabelPosition::Center),
                Some(LabelPosition::End),
            ],
        )
        .flatten()
    {
        d = d.label("Continue with email").label_position(pos);
    }
    if let Some(c) = v
        .pick(2, &[None, Some(ColorName::Teal), Some(ColorName::Red)])
        .flatten()
    {
        d = d.color(c.shade(6).to_hsla());
    }
    Stack::new()
        .gap(Size::Md)
        .child(Text::new("Sign in with a passkey").size(Size::Sm))
        .child(d)
        .child(Text::new("you@example.com").size(Size::Sm).dimmed(true))
        .into_any_element()
}

fn scroll(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let cx: &App = cx;
            column([
                example(
                    "Vertical (default), 160px tall",
                    div().h(px(160.)).child(
                        ScrollArea::new("scroll-y").child(
                            Stack::new()
                                .gap(Size::Xs)
                                .children((1..=30).map(|i| tile(format!("Row {i}"), cx))),
                        ),
                    ),
                ),
                example(
                    "Horizontal: scroll_x(true).scroll_y(false)",
                    div().w_full().child(
                        ScrollArea::new("scroll-x")
                            .scroll_x(true)
                            .scroll_y(false)
                            .child(Group::new().children(
                                (1..=24).map(|i| tile(format!("Frame {i}"), cx).w(px(96.))),
                            )),
                    ),
                ),
                example(
                    "Both axes: ScrollArea::both",
                    div()
                        .h(px(140.))
                        .child(ScrollArea::both("scroll-both").child(
                            Stack::new().gap(Size::Xs).children((1..=12).map(|r| {
                                Group::new().children(
                                    (1..=14).map(move |c| tile(format!("R{r}C{c}"), cx).w(px(72.))),
                                )
                            })),
                        )),
                ),
            ])
        },
        cx,
    )
}

struct SplitterDemo {
    two: Entity<SplitterState>,
    three: Entity<SplitterState>,
    log: Entity<EventLog>,
}

impl Render for SplitterDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        column([
            example(
                "Horizontal: min 120px, max 360px, persist_key, on_resize",
                div().h(px(150.)).child(
                    Splitter::new(&self.two)
                        .first_size(px(220.))
                        .min_size(px(120.))
                        .max_size(px(360.))
                        .persist_key("showcase-splitter")
                        .on_resize(move |sizes, _, cx| {
                            let s: Vec<_> = sizes
                                .iter()
                                .map(|p| format!("{:.0}", f32::from(*p)))
                                .collect();
                            EventLog::log(&log, format!("on_resize [{}]", s.join(", ")), cx);
                        })
                        .first(tile("Files", cx).size_full())
                        .second(tile("Editor", cx).size_full()),
                ),
            ),
            example(
                "Vertical axis, three panes (pane_size)",
                div().h(px(220.)).child(
                    Splitter::new(&self.three)
                        .axis(SplitterAxis::Vertical)
                        .min_size(px(40.))
                        .pane_size(0, px(60.))
                        .pane_size(2, px(60.))
                        .panes(["Toolbar", "Canvas", "Console"].map(|l| tile(l, cx).size_full())),
                ),
            ),
            example("Events", self.log.clone()),
        ])
    }
}

fn splitter(_: &mut Window, cx: &mut App) -> AnyView {
    let two = SplitterState::new(cx);
    let three = SplitterState::new(cx);
    let log = EventLog::new(cx);
    EventLog::watch::<SplitterEvent, _>(&log, &two, cx);
    EventLog::watch::<SplitterEvent, _>(&log, &three, cx);
    cx.new(|_| SplitterDemo { two, three, log }).into()
}

struct AppShellDemo {
    state: Entity<AppShellState>,
    main: AnyView,
    log: Entity<EventLog>,
}

fn link(id: &'static str, label: &'static str) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .cursor_pointer()
        .child(Text::new(label).size(Size::Sm).color(ColorName::Blue))
}

impl Render for AppShellDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (nav, aside) = (self.state.clone(), self.state.clone());
        column([
            example(
                "Header, navbar, main, aside, footer · resizable panels · collapse below Sm",
                div().h(px(300.)).child(
                    AppShell::new(&self.state)
                        .border()
                        .header_height(px(44.))
                        .footer_height(px(28.))
                        .padding(Size::Sm)
                        .collapse_below(Breakpoint::Sm)
                        .collapse_aside_below(Breakpoint::Md)
                        .resizable_navbar()
                        .resizable_aside()
                        .header(
                            Group::new()
                                .grow(true)
                                .justify(Justify::Between)
                                .child(Text::new("Acme Mail"))
                                .child(
                                    Group::new()
                                        .child(link("toggle-nav", "Toggle navbar").on_click(
                                            move |_, _, cx| {
                                                nav.update(cx, |s, cx| s.toggle_navbar(cx))
                                            },
                                        ))
                                        .child(link("toggle-aside", "Toggle aside").on_click(
                                            move |_, _, cx| {
                                                aside.update(cx, |s, cx| s.toggle_aside(cx))
                                            },
                                        )),
                                ),
                        )
                        .navbar(
                            Stack::new()
                                .gap(Size::Xs)
                                .children(["Inbox", "Drafts", "Sent"].map(|l| tile(l, cx))),
                        )
                        .main(self.main.clone())
                        .aside(tile("Details", cx).size_full())
                        .footer(
                            Text::new("3 unread · synced just now")
                                .size(Size::Xs)
                                .dimmed(true),
                        ),
                ),
            ),
            example("Events", self.log.clone()),
        ])
    }
}

fn app_shell(_: &mut Window, cx: &mut App) -> AnyView {
    let state = AppShellState::new(cx);
    let log = EventLog::new(cx);
    EventLog::watch::<AppShellEvent, _>(&log, &state, cx);
    let main = stateless(
        |_, cx| tile("Main content", cx).size_full().into_any_element(),
        cx,
    );
    cx.new(|_| AppShellDemo { state, main, log }).into()
}
