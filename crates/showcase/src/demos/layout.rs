//! guiraffe-layout: placement primitives. Spacing comes from Size tokens and context, never
//! from per-child margins.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, prelude::*, px,
};
use guiraffe_components::Text;
use guiraffe_core::Size;
use guiraffe_layout::*;

use super::{column, example, tile};
use crate::catalog::{Entry, stateless};
use crate::knobs::{Knob, KnobValues, SIZE_NAMES, with_knobs};
use gpui_kit::AnyElement;

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
            Knob::new("Align", &["start", "center", "end"]),
            Knob::new("Justify", &["start", "center", "end", "between", "around"]),
            Knob::toggle("Grow"),
        ],
        stack_group_live,
        |_, cx| {
            column([
                example(
                    "Stack gap=Md",
                    Stack::new()
                        .gap(Size::Md)
                        .children((1..=3).map(|i| tile(format!("Item {i}"), cx))),
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
    let align = v
        .pick(1, &[Align::Start, Align::Center, Align::End])
        .unwrap_or_default();
    let justify = v
        .pick(
            2,
            &[
                Justify::Start,
                Justify::Center,
                Justify::End,
                Justify::Between,
                Justify::Around,
            ],
        )
        .unwrap_or_default();
    Group::new()
        .gap(v.size(0))
        .align(align)
        .justify(justify)
        .grow(v.on(3))
        .child(tile("Short", cx))
        .child(tile("Taller", cx).min_h(px(64.)))
        .child(tile("Medium width item", cx))
        .into_any_element()
}

fn flex(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
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
        cx,
    )
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

fn simple_grid(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            column([
                example(
                    "cols=3",
                    SimpleGrid::new()
                        .cols(3)
                        .children((1..=6).map(|i| tile(format!("Cell {i}"), cx))),
                ),
                example(
                    "min child width 140px",
                    SimpleGrid::new()
                        .min_child_width(px(140.))
                        .children((1..=8).map(|i| tile(format!("Card {i}"), cx))),
                ),
            ])
        },
        cx,
    )
}

fn container(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            column([
                example(
                    "Container size=Xs",
                    Container::new()
                        .size(Size::Xs)
                        .child(tile("max-width content", cx)),
                ),
                example(
                    "Center",
                    Center::new()
                        .min_height(px(120.))
                        .child(tile("centered", cx).w(px(140.))),
                ),
            ])
        },
        cx,
    )
}

fn aspect(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            column([example(
                "16:9 and 1:1",
                div()
                    .flex()
                    .gap(px(16.))
                    .child(
                        div()
                            .w(px(320.))
                            .child(AspectRatio::new(16. / 9.).child(tile("16:9", cx).size_full())),
                    )
                    .child(
                        div()
                            .w(px(160.))
                            .child(AspectRatio::new(1.).child(tile("1:1", cx).size_full())),
                    ),
            )])
        },
        cx,
    )
}

fn divider(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            column([
                example(
                    "Labels",
                    Stack::new()
                        .gap(Size::Md)
                        .child(Divider::new())
                        .child(Divider::new().label("Center"))
                        .child(
                            Divider::new()
                                .label("Start")
                                .label_position(LabelPosition::Start),
                        ),
                ),
                example(
                    "Vertical + Space",
                    Group::new()
                        .child(tile("A", cx))
                        .child(Divider::vertical())
                        .child(tile("B", cx))
                        .child(Space::w(Size::Xl))
                        .child(tile("after Space::w(Xl)", cx)),
                ),
            ])
        },
        cx,
    )
}

fn scroll(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            column([example(
                "Vertical, 200px tall",
                div().h(px(200.)).child(
                    ScrollArea::new("scroll-demo").child(
                        Stack::new()
                            .gap(Size::Xs)
                            .children((1..=30).map(|i| tile(format!("Row {i}"), cx))),
                    ),
                ),
            )])
        },
        cx,
    )
}

struct SplitterDemo(Entity<SplitterState>);

impl Render for SplitterDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column([example(
            "Drag the handle",
            div().h(px(220.)).child(
                Splitter::new(&self.0)
                    .first_size(px(220.))
                    .min_size(px(120.))
                    .first(tile("Files", cx).size_full())
                    .second(tile("Editor", cx).size_full()),
            ),
        )])
    }
}

fn splitter(_: &mut Window, cx: &mut App) -> AnyView {
    let state = SplitterState::new(cx);
    cx.new(|_| SplitterDemo(state)).into()
}

struct AppShellDemo {
    state: Entity<AppShellState>,
    main: AnyView,
}

impl Render for AppShellDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        column([example(
            "Header, navbar, main, aside",
            div().h(px(320.)).child(
                AppShell::new(&self.state)
                    .border()
                    .header(
                        Group::new()
                            .grow(true)
                            .justify(Justify::Between)
                            .child(Text::new("My app"))
                            .child(
                                div()
                                    .id("toggle-nav")
                                    .cursor_pointer()
                                    .child(Text::new("Toggle navbar").size(Size::Sm))
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |s, cx| s.toggle_navbar(cx))
                                    }),
                            ),
                    )
                    .navbar(
                        Stack::new()
                            .gap(Size::Xs)
                            .children(["Inbox", "Drafts", "Sent"].map(|l| tile(l, cx))),
                    )
                    .main(self.main.clone())
                    .aside(tile("Aside", cx).size_full()),
            ),
        )])
    }
}

fn app_shell(_: &mut Window, cx: &mut App) -> AnyView {
    let state = AppShellState::new(cx);
    let main = stateless(
        |_, cx| tile("Main content", cx).size_full().into_any_element(),
        cx,
    );
    cx.new(|_| AppShellDemo { state, main }).into()
}
