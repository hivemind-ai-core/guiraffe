//! Step 1: open a window and draw themed text.
//!
//! `cargo run -p getting-started-examples --example 01_hello --features gpui-fast`

use gpui_kit::{
    App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, WindowOptions,
    div, px,
};
use guiraffe_components::{Text, Title};
use guiraffe_core::GuiraffeTheme;

/// A view: a struct that implements `Render`. GPUI keeps it alive in an `Entity`.
struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Colours come from the theme, never from constants.
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .bg(s.body.to_hsla())
            .child(Title::new("Hello, GUIraffe").order(1))
            .child(Text::new("A native window, drawn on the GPU.").dimmed(true))
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            // Installs GPUI Controls and the GUIraffe theme (idempotent).
            guiraffe_components::init(cx);
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let hello = cx.new(|_| Hello);
                cx.new(|cx| gpui_kit::component::Root::new(hello, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the window: {error}");
                cx.quit();
            }
        });
}
