//! Run with `cargo run -p guiraffe-overlays --example overlays`.
use gpui_kit::component::button::Button;
use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, WindowOptions, div, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use guiraffe_overlays::{
    Drawer, OverlayHost, Spotlight,
    search::Command,
    toast::{Toast, ToastId},
};
use guiraffe_windows::{OpenMode, WindowConfig, WindowKey, WindowManager};
struct Demo;
impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(Size::Lg.spacing()))
            .gap(px(Size::Md.spacing()))
            .child("GUIraffe overlays — independent state in every managed window")
            .child(
                Button::new("modal")
                    .label("Confirm")
                    .on_click(|_, window, cx| {
                        match OverlayHost::confirm(
                            window,
                            cx,
                            "Keep changes?",
                            "This result can be awaited.",
                        ) {
                            Ok((_, result)) => {
                                cx.spawn(async move |_| {
                                    let confirmed = result.await;
                                    println!("Confirmed: {confirmed}");
                                })
                                .detach();
                            }
                            Err(error) => eprintln!("{error}"),
                        }
                    }),
            )
            .child(
                Button::new("drawer")
                    .label("Drawer")
                    .on_click(|_, window, cx| {
                        if let Err(error) = Drawer::default().open(window, cx, |sheet, _, _| {
                            sheet
                                .title("Drawer")
                                .child("Controls provides focus trapping and transitions.")
                        }) {
                            eprintln!("{error}");
                        }
                    }),
            )
            .child(
                Button::new("toast")
                    .label("Toast (updates existing ID)")
                    .on_click(|_, window, cx| {
                        if let Err(error) = OverlayHost::notify(
                            window,
                            cx,
                            Toast::new(ToastId(1), "Saved — hover to pause"),
                        ) {
                            eprintln!("{error}");
                        }
                    }),
            )
            .child(
                Button::new("spotlight")
                    .label("Commands")
                    .on_click(|_, window, cx| {
                        let palette = cx.new(|cx| {
                            Spotlight::new(
                                vec![
                                    Command::new("open", "Open document"),
                                    Command::new("save", "Save document"),
                                ],
                                window,
                                cx,
                                |id, _, _| println!("Selected {id}"),
                            )
                        });
                        if let Err(error) = Spotlight::open(&palette, window, cx) {
                            eprintln!("{error}");
                        }
                    }),
            )
            .bg(GuiraffeTheme::global(cx).surfaces().body.to_hsla())
    }
}
fn main() {
    gpui_kit::application().run(|cx| {
        guiraffe_overlays::init(cx);
        WindowManager::init(WindowConfig::default(), cx);
        if let Err(error) = WindowManager::open(
            WindowKey::kind("overlays"),
            OpenMode::Reuse,
            WindowOptions::default(),
            |window, cx| {
                let demo = cx.new(|_| Demo);
                guiraffe_overlays::root(demo, window, cx)
            },
            cx,
        ) {
            eprintln!("{error}");
        }
    });
}
