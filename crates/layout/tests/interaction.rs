//! Interaction tests: the widgets that answer the pointer.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::ResizableState;
use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, Entity, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    Point, Render, TestAppContext, Window, div, point, px,
};
use guiraffe_layout::{AppShell, Size, Splitter};

macro_rules! harness {
    ($cx:expr, $view:expr) => {{
        let (_, cx) = $cx.add_window_view(|_, _| $view);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx
    }};
}

fn click_at(cx: &mut gpui_kit::VisualTestContext, position: Point<Pixels>) {
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
}

struct SplitterHarness {
    state: Entity<ResizableState>,
    resizes: Rc<Cell<usize>>,
}

impl Render for SplitterHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let resizes = self.resizes.clone();
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new("workspace")
                .with_state(&self.state)
                .first_size(px(200.))
                .on_resize(move |_| resizes.set(resizes.get() + 1))
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

#[gpui_kit::test]
fn dragging_the_divider_resizes_the_panes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.update(|cx| cx.new(|_| ResizableState::default()));
    let resizes = Rc::new(Cell::new(0));
    let cx = harness!(
        cx,
        SplitterHarness {
            state: state.clone(),
            resizes: resizes.clone(),
        }
    );

    let first = cx.debug_bounds("first").expect("the first pane");
    let second = cx.debug_bounds("second").expect("the second pane");
    assert_eq!(first.size.width, px(200.), "the first pane starts wide");
    assert_eq!(second.size.width, px(200.), "the second pane starts wide");

    let boundary = first.right();
    cx.update(|window, cx| {
        window.drag(
            point(boundary - px(2.), px(100.)),
            point(boundary + px(60.), px(100.)),
            cx,
        );
    });

    let first = cx.debug_bounds("first").expect("the first pane");
    let second = cx.debug_bounds("second").expect("the second pane");
    assert_eq!(first.size.width, px(260.), "the divider moved to the right");
    assert_eq!(
        second.size.width,
        px(140.),
        "the second pane gave up the room"
    );
    assert_eq!(resizes.get(), 1, "the resize was reported once");

    state.read_with(cx, |state, _| {
        assert_eq!(state.sizes(), &vec![px(260.), px(140.)]);
    });
}

struct ShellHarness {
    collapsed: bool,
}

impl Render for ShellHarness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.collapsed;
        let toggle = cx.listener(|this: &mut Self, _, _, _| {
            this.collapsed = !this.collapsed;
        });
        div().w(px(400.)).h(px(300.)).child(
            AppShell::new()
                .navbar_width(px(100.))
                .navbar_collapsed(collapsed)
                .navbar(
                    div().h_full().child(
                        div()
                            .id("toggle")
                            .debug_selector(|| "toggle".into())
                            .on_click(move |event, window, cx| toggle(event, window, cx))
                            .child("menu"),
                    ),
                )
                .main(div().h_full().debug_selector(|| "main".into())),
        )
    }
}

#[gpui_kit::test]
fn a_button_in_the_navbar_can_collapse_it(cx: &mut TestAppContext) {
    let cx = harness!(cx, ShellHarness { collapsed: false });

    let navbar = cx.debug_bounds("main").expect("the main content");
    assert_eq!(navbar.left(), px(100.), "the navbar takes its room");

    let toggle = cx.debug_bounds("toggle").expect("the toggle");
    click_at(cx, toggle.center());

    let main = cx.debug_bounds("main").expect("the main content");
    assert_eq!(main.left(), px(0.), "the navbar gave its room back");
    assert!(cx.debug_bounds("toggle").is_none(), "the navbar is gone");
}

struct SizedHarness;

impl Render for SizedHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(100.)).child(
            guiraffe_layout::Stack::new()
                .gap(Size::Sm)
                .child(div().debug_selector(|| "content".into())),
        )
    }
}

#[gpui_kit::test]
fn a_stack_keeps_its_gap(cx: &mut TestAppContext) {
    let cx = harness!(cx, SizedHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(content.top(), px(0.));
}
