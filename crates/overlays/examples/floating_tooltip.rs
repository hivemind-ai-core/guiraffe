//! Hover the trigger to exercise the same measured floating solver as the tests.
use gpui_kit::{
    AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, WindowOptions, div, prelude::FluentBuilder, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use guiraffe_overlays::{FloatingTooltip, position::Rect};
struct Demo {
    hovered: bool,
}
impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surface = theme.surfaces();
        let anchor = Rect {
            x: Size::Lg.spacing(),
            y: Size::Lg.spacing(),
            width: Size::Xs.container_width() / 2.,
            height: Size::Md.control_height(),
        };
        div()
            .relative()
            .size_full()
            .bg(surface.body.to_hsla())
            .child(
                div()
                    .id("tooltip-trigger")
                    .absolute()
                    .left(px(anchor.x))
                    .top(px(anchor.y))
                    .w(px(anchor.width))
                    .h(px(anchor.height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(surface.surface.to_hsla())
                    .text_color(surface.text.to_hsla())
                    .rounded(px(Size::Md.radius()))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if this.hovered != *hovered {
                            this.hovered = *hovered;
                            cx.notify();
                        }
                    }))
                    .child("Hover for measured positioning"),
            )
            .when(self.hovered, |view| {
                view.child(FloatingTooltip::new(
                    "floating-tip",
                    "Flip and cross-axis shift use the measured tooltip size",
                    anchor,
                ))
            })
    }
}
fn main() {
    gpui_kit::application().run(|cx| {
        guiraffe_overlays::init(cx);
        if let Err(error) = gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| Demo { hovered: false })
        }) {
            eprintln!("{error}");
        }
    });
}
