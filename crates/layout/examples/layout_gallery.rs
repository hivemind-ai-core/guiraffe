//! A gallery of every widget in `guiraffe-layout`.
//!
//! Run it with `cargo run -p guiraffe-layout --example layout_gallery`.

use gpui_kit::prelude::*;
use gpui_kit::{Context, Div, Entity, Render, Window, WindowOptions, div, px, rgb};
use guiraffe_layout::{
    AppShell, AppShellState, AspectRatio, Breakpoint, Center, Col, Container, ContainerWidth,
    Divider, Flex, FlexDirection, Grid, Group, Justify, LabelPosition, Responsive, ScrollArea,
    SimpleGrid, Size, Space, Splitter, SplitterState, Stack,
};

const INK: u32 = 0x263b38;
const MUTED: u32 = 0x76827c;
const LINE: u32 = 0xe4e8e1;
const GREEN: u32 = 0x277764;
const PALE: u32 = 0xeaf2eb;

fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_4()
        .bg(rgb(0xffffff))
        .border_1()
        .border_color(rgb(LINE))
        .rounded_lg()
}

fn heading(text: impl Into<gpui_kit::SharedString>) -> Div {
    div()
        .text_sm()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .text_color(rgb(INK))
        .child(text.into())
}

fn muted(text: impl Into<gpui_kit::SharedString>) -> Div {
    div().text_xs().text_color(rgb(MUTED)).child(text.into())
}

fn pill(text: impl Into<gpui_kit::SharedString>) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(PALE))
        .text_xs()
        .text_color(rgb(GREEN))
        .child(text.into())
}

struct Gallery {
    shell: Entity<AppShellState>,
    splitter: Entity<SplitterState>,
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        AppShell::new(&self.shell)
            .header(
                Group::new()
                    .justify(Justify::Between)
                    .w_full()
                    .child(heading("GUIraffe layout"))
                    .child(pill("gallery")),
            )
            .navbar(
                Stack::new()
                    .gap_3()
                    .p_4()
                    .child(muted("Sections"))
                    .child(pill("Stack"))
                    .child(pill("Group"))
                    .child(pill("Flex"))
                    .child(pill("Grid"))
                    .child(pill("SimpleGrid"))
                    .child(pill("Container"))
                    .child(pill("Center"))
                    .child(pill("AspectRatio"))
                    .child(pill("Space"))
                    .child(pill("AppShell"))
                    .child(pill("Splitter")),
            )
            .main(
                ScrollArea::new("gallery-scroll").child(
                    Stack::new()
                        .gap_5()
                        .p_5()
                        .w_full()
                        .child(stacks())
                        .child(grids())
                        .child(shapes())
                        .child(shell(&self.splitter)),
                ),
            )
            .footer(
                Group::new()
                    .justify(Justify::Between)
                    .w_full()
                    .px_4()
                    .child(muted("guiraffe-layout"))
                    .child(muted("gpui-kit 0.7.0")),
            )
    }
}

fn stacks() -> Div {
    card().child(heading("Stack, Group and Flex")).child(
        Flex::new()
            .direction(FlexDirection::Column)
            .gap(Size::Md)
            .child(
                Group::new()
                    .gap(Size::Sm)
                    .child(pill("left"))
                    .child(pill("middle"))
                    .child(pill("right")),
            )
            .child(
                Stack::new()
                    .gap(Size::Xs)
                    .child(muted("A stack puts its children one below the other."))
                    .child(muted(
                        "Its gap is a size token, like every other space here.",
                    )),
            ),
    )
}

fn grids() -> Div {
    card()
        .child(heading("Grid and SimpleGrid"))
        .child(
            Grid::new()
                .gutter(Size::Sm)
                .container_width(ContainerWidth::Viewport)
                .child(Col::span(6).child(muted("Six columns")))
                .child(Col::span(6).child(muted("Six columns")))
                .child(
                    Col::span(4)
                        .offset(2)
                        .child(muted("Four columns, two of them empty")),
                ),
        )
        .child(
            SimpleGrid::new()
                .cols(3)
                .spacing(Size::Sm)
                .child(pill("one"))
                .child(pill("two"))
                .child(pill("three")),
        )
        .child(
            Grid::new().gutter(Size::Sm).child(
                Col::span(Responsive::base(12).at(Breakpoint::Md, 6))
                    .child(muted("Half the width from the medium breakpoint up")),
            ),
        )
}

fn shapes() -> Div {
    card()
        .child(heading("Container, Center, AspectRatio and Space"))
        .child(
            Container::new()
                .size(Size::Sm)
                .padding(Size::Sm)
                .child(muted(
                    "A container is centered and never wider than its size says.",
                )),
        )
        .child(
            Center::new().min_height(px(80.)).child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(PALE))
                    .child(muted("Centered in its own space")),
            ),
        )
        .child(
            AspectRatio::new(16. / 9.).child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(rgb(PALE))
                    .child(muted("Sixteen by nine")),
            ),
        )
        .child(
            Group::new()
                .child(muted("left"))
                .child(Space::w(Size::Lg))
                .child(muted("right")),
        )
        .child(
            Divider::new()
                .label("Section")
                .label_position(LabelPosition::Center),
        )
}

fn shell(splitter: &Entity<SplitterState>) -> Div {
    card().child(heading("AppShell and Splitter")).child(
        Splitter::new(splitter)
            .first_size(px(160.))
            .first(
                div()
                    .size_full()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(PALE))
                    .child(muted("A pane you can drag")),
            )
            .second(
                div()
                    .size_full()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(LINE))
                    .child(muted("The other pane")),
            ),
    )
}

fn main() {
    gpui_kit::application().run(|cx| {
        guiraffe_layout::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            let splitter = SplitterState::new(cx);
            let shell = AppShellState::new(cx);
            cx.new(|_| Gallery { shell, splitter })
        })
        .expect("the gallery window opens");
    });
}
