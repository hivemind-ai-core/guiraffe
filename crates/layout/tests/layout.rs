//! Layout assertions for the widgets in this crate.

use gpui_kit::prelude::*;
use gpui_kit::{Context, Render, TestAppContext, Window, div, px};
use guiraffe_layout::{
    AppShell, AspectRatio, Breakpoint, Center, Col, Container, ContainerWidth, Divider, Flex,
    FlexDirection, Grid, Group, LabelPosition, Responsive, ScrollArea, SimpleGrid, Size, Space,
    Stack,
};

macro_rules! harness {
    ($cx:expr, $view:expr) => {{
        let (_, cx) = $cx.add_window_view(|_, _| $view);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx
    }};
}

struct StackHarness;

impl Render for StackHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(200.)).child(
            Stack::new()
                .gap(Size::Md)
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .debug_selector(|| "first".into()),
                )
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .debug_selector(|| "second".into()),
                ),
        )
    }
}

#[gpui_kit::test]
fn a_stack_puts_its_children_one_below_the_other(cx: &mut TestAppContext) {
    let cx = harness!(cx, StackHarness);

    let first = cx.debug_bounds("first").expect("the first child");
    let second = cx.debug_bounds("second").expect("the second child");

    assert_eq!(first.top(), px(0.));
    assert_eq!(second.top(), px(36.), "the gap sits between the children");
    assert_eq!(first.left(), second.left());
}

struct GroupHarness;

impl Render for GroupHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(200.)).child(
            Group::new()
                .gap(Size::Xs)
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .debug_selector(|| "first".into()),
                )
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .debug_selector(|| "second".into()),
                ),
        )
    }
}

#[gpui_kit::test]
fn a_group_puts_its_children_side_by_side(cx: &mut TestAppContext) {
    let cx = harness!(cx, GroupHarness);

    let first = cx.debug_bounds("first").expect("the first child");
    let second = cx.debug_bounds("second").expect("the second child");

    assert_eq!(first.left(), px(0.));
    assert_eq!(second.left(), px(30.), "the gap sits between the children");
    assert_eq!(first.top(), second.top());
}

struct FlexHarness;

impl Render for FlexHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(200.)).child(
            Flex::new()
                .direction(FlexDirection::Column)
                .gap(Size::Xs)
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .debug_selector(|| "first".into()),
                )
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .debug_selector(|| "second".into()),
                ),
        )
    }
}

#[gpui_kit::test]
fn a_flex_can_run_downwards(cx: &mut TestAppContext) {
    let cx = harness!(cx, FlexHarness);

    let first = cx.debug_bounds("first").expect("the first child");
    let second = cx.debug_bounds("second").expect("the second child");

    assert_eq!(second.top(), px(30.));
    assert_eq!(first.left(), second.left());
}

struct GridHarness;

impl Render for GridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).h(px(120.)).child(
            Grid::new()
                .gutter_px(0.)
                .child(Col::span(6).child(div().h(px(10.)).debug_selector(|| "a".into())))
                .child(Col::span(6).child(div().h(px(10.)).debug_selector(|| "b".into()))),
        )
    }
}

#[gpui_kit::test]
fn a_grid_splits_its_columns_between_its_children(cx: &mut TestAppContext) {
    let cx = harness!(cx, GridHarness);

    let first = cx.debug_bounds("a").expect("the first column");
    let second = cx.debug_bounds("b").expect("the second column");

    assert_eq!(first.size.width, px(120.), "six of twelve columns");
    assert_eq!(second.size.width, px(120.));
    assert_eq!(
        second.left(),
        px(120.),
        "the second column starts where the first ends"
    );
    assert_eq!(first.top(), second.top());
}

struct OffsetGridHarness;

impl Render for OffsetGridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).h(px(120.)).child(
            Grid::new()
                .gutter_px(0.)
                .child(Col::span(4).child(div().h(px(10.)).debug_selector(|| "a".into())))
                .child(
                    Col::span(4)
                        .offset(2)
                        .child(div().h(px(10.)).debug_selector(|| "b".into())),
                ),
        )
    }
}

#[gpui_kit::test]
fn an_offset_leaves_the_columns_before_it_empty(cx: &mut TestAppContext) {
    let cx = harness!(cx, OffsetGridHarness);

    let first = cx.debug_bounds("a").expect("the first column");
    let second = cx.debug_bounds("b").expect("the offset column");

    assert_eq!(first.left(), px(0.));
    assert_eq!(second.left(), px(40.), "two empty columns of twenty pixels");
    assert_eq!(second.size.width, px(80.), "four of twelve columns");
}

struct ResponsiveGridHarness;

impl Render for ResponsiveGridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).h(px(120.)).child(
            Grid::new()
                .gutter_px(0.)
                .container_width(ContainerWidth::fixed(px(100.)))
                .child(
                    Col::span(Responsive::base(12).at(Breakpoint::Md, 6))
                        .child(div().h(px(10.)).debug_selector(|| "a".into())),
                ),
        )
    }
}

#[gpui_kit::test]
fn a_column_changes_its_span_with_the_width_it_is_given(cx: &mut TestAppContext) {
    let cx = harness!(cx, ResponsiveGridHarness);

    let column = cx.debug_bounds("a").expect("the column");
    assert_eq!(
        column.size.width,
        px(240.),
        "a narrow width keeps the full span"
    );
}

struct WideResponsiveGridHarness;

impl Render for WideResponsiveGridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).h(px(120.)).child(
            Grid::new()
                .gutter_px(0.)
                .container_width(ContainerWidth::fixed(px(1000.)))
                .child(
                    Col::span(Responsive::base(12).at(Breakpoint::Md, 6))
                        .child(div().h(px(10.)).debug_selector(|| "a".into())),
                ),
        )
    }
}

#[gpui_kit::test]
fn a_wide_container_halves_the_span(cx: &mut TestAppContext) {
    let cx = harness!(cx, WideResponsiveGridHarness);

    let column = cx.debug_bounds("a").expect("the column");
    assert_eq!(column.size.width, px(120.), "six of twelve columns");
}

struct WrappingGridHarness;

impl Render for WrappingGridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).h(px(120.)).child(
            Grid::new()
                .gutter_px(0.)
                .child(Col::span(8).child(div().h(px(10.)).debug_selector(|| "a".into())))
                .child(Col::span(8).child(div().h(px(10.)).debug_selector(|| "b".into()))),
        )
    }
}

#[gpui_kit::test]
fn a_column_that_does_not_fit_moves_to_the_next_row(cx: &mut TestAppContext) {
    let cx = harness!(cx, WrappingGridHarness);

    let first = cx.debug_bounds("a").expect("the first column");
    let second = cx.debug_bounds("b").expect("the second column");

    assert_eq!(first.size.width, px(160.), "eight of twelve columns");
    assert_eq!(
        second.size.width,
        px(160.),
        "the column keeps its span on the next row"
    );
    assert_eq!(
        second.top(),
        px(10.),
        "the column that does not fit moves down a row"
    );
}

struct DefaultSpanGridHarness;

impl Render for DefaultSpanGridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).h(px(120.)).child(
            Grid::new()
                .columns(6)
                .gutter_px(0.)
                .child(Col::new().child(div().h(px(10.)).debug_selector(|| "a".into()))),
        )
    }
}

#[gpui_kit::test]
fn a_column_without_a_span_fills_the_grid(cx: &mut TestAppContext) {
    let cx = harness!(cx, DefaultSpanGridHarness);

    let column = cx.debug_bounds("a").expect("the column");
    assert_eq!(
        column.size.width,
        px(240.),
        "the column takes every column of the grid"
    );
}

struct SimpleGridHarness;

impl Render for SimpleGridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(300.)).h(px(120.)).child(
            SimpleGrid::new()
                .cols(3)
                .spacing_px(0.)
                .child(div().h(px(10.)).debug_selector(|| "a".into()))
                .child(div().h(px(10.)).debug_selector(|| "b".into()))
                .child(div().h(px(10.)).debug_selector(|| "c".into())),
        )
    }
}

#[gpui_kit::test]
fn a_simple_grid_splits_its_cells_evenly(cx: &mut TestAppContext) {
    let cx = harness!(cx, SimpleGridHarness);

    let first = cx.debug_bounds("a").expect("the first cell");
    let second = cx.debug_bounds("b").expect("the second cell");
    let third = cx.debug_bounds("c").expect("the third cell");

    assert_eq!(first.size.width, px(100.));
    assert_eq!(second.left(), px(100.));
    assert_eq!(third.left(), px(200.));
}

struct ContainerHarness;

impl Render for ContainerHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(1200.)).h(px(120.)).child(
            Container::new()
                .size(Size::Sm)
                .child(div().h(px(10.)).debug_selector(|| "content".into())),
        )
    }
}

#[gpui_kit::test]
fn a_container_is_centered_and_never_wider_than_its_size(cx: &mut TestAppContext) {
    let cx = harness!(cx, ContainerHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(content.size.width, px(720.), "the small container");
    assert_eq!(content.left(), px(240.), "the container sits in the middle");
}

struct NarrowContainerHarness;

impl Render for NarrowContainerHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(120.)).child(
            Container::new()
                .size(Size::Sm)
                .child(div().h(px(10.)).debug_selector(|| "content".into())),
        )
    }
}

#[gpui_kit::test]
fn a_container_fits_the_space_it_is_given(cx: &mut TestAppContext) {
    let cx = harness!(cx, NarrowContainerHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(
        content.size.width,
        px(400.),
        "the container does not reach past a narrow parent"
    );
    assert_eq!(content.left(), px(0.));
}

struct CenterHarness;

impl Render for CenterHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(100.)).flex().flex_col().child(
            Center::new().min_height(px(100.)).child(
                div()
                    .w(px(20.))
                    .h(px(20.))
                    .debug_selector(|| "content".into()),
            ),
        )
    }
}

#[gpui_kit::test]
fn a_center_puts_its_content_in_the_middle(cx: &mut TestAppContext) {
    let cx = harness!(cx, CenterHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(content.left(), px(90.));
    assert_eq!(content.top(), px(40.));
}

struct AspectRatioHarness;

impl Render for AspectRatioHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).flex().flex_col().child(
            AspectRatio::new(2.).child(div().size_full().debug_selector(|| "content".into())),
        )
    }
}

#[gpui_kit::test]
fn an_aspect_ratio_keeps_the_shape_of_its_box(cx: &mut TestAppContext) {
    let cx = harness!(cx, AspectRatioHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(content.size.width, px(200.));
    assert_eq!(content.size.height, px(100.), "half as tall as it is wide");
}

struct SpaceHarness;

impl Render for SpaceHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(200.))
            .h(px(100.))
            .flex()
            .flex_row()
            .child(Space::w(Size::Md))
            .child(
                div()
                    .w(px(20.))
                    .h(px(20.))
                    .debug_selector(|| "content".into()),
            )
    }
}

#[gpui_kit::test]
fn a_space_pushes_what_follows_it_along(cx: &mut TestAppContext) {
    let cx = harness!(cx, SpaceHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(content.left(), px(16.), "the medium gap");
}

struct AppShellHarness;

impl Render for AppShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(300.)).child(
            AppShell::new()
                .navbar_width(px(100.))
                .aside_width(px(80.))
                .navbar(div().h_full().debug_selector(|| "navbar".into()))
                .main(div().h_full().debug_selector(|| "main".into()))
                .aside(div().h_full().debug_selector(|| "aside".into())),
        )
    }
}

#[gpui_kit::test]
fn an_app_shell_keeps_its_parts_where_they_belong(cx: &mut TestAppContext) {
    let cx = harness!(cx, AppShellHarness);

    let navbar = cx.debug_bounds("navbar").expect("the navbar");
    let main = cx.debug_bounds("main").expect("the main content");
    let aside = cx.debug_bounds("aside").expect("the aside");

    assert_eq!(navbar.left(), px(0.));
    assert_eq!(navbar.size.width, px(100.));
    assert_eq!(main.left(), px(100.));
    assert_eq!(
        main.size.width,
        px(220.),
        "what is left between the two bars"
    );
    assert_eq!(aside.left(), px(320.));
    assert_eq!(aside.size.width, px(80.));
}

struct CollapsedShellHarness;

impl Render for CollapsedShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(300.)).child(
            AppShell::new()
                .navbar_width(px(100.))
                .navbar_collapsed(true)
                .navbar(div().h_full().debug_selector(|| "navbar".into()))
                .main(div().h_full().debug_selector(|| "main".into())),
        )
    }
}

#[gpui_kit::test]
fn a_collapsed_navbar_takes_no_room(cx: &mut TestAppContext) {
    let cx = harness!(cx, CollapsedShellHarness);

    assert!(cx.debug_bounds("navbar").is_none(), "the navbar is gone");
    let main = cx.debug_bounds("main").expect("the main content");
    assert_eq!(main.left(), px(0.));
    assert_eq!(main.size.width, px(400.));
}

struct DividerHarness;

impl Render for DividerHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).child(
            Divider::new()
                .label("Section")
                .label_position(LabelPosition::Center),
        )
    }
}

#[gpui_kit::test]
fn a_divider_keeps_its_label(cx: &mut TestAppContext) {
    let cx = harness!(cx, DividerHarness);

    assert!(
        cx.debug_bounds("Section").is_none(),
        "labels are not elements of their own"
    );
}

struct ScrollHarness;

impl Render for ScrollHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(100.)).h(px(50.)).child(
            ScrollArea::new("scroll").child(div().h(px(400.)).debug_selector(|| "content".into())),
        )
    }
}

#[gpui_kit::test]
fn a_scroll_area_keeps_its_content_in_one_place(cx: &mut TestAppContext) {
    let cx = harness!(cx, ScrollHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(
        content.size.height,
        px(400.),
        "the content keeps its height"
    );
}
