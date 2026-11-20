use gpui_kit::{AppContext, WindowOptions, component::button::Button, div, prelude::*};
use guiraffe_ui::ui;

#[derive(Default)]
struct Counter {
    count: usize,
}

impl Render for Counter {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui! {
            div {
                size_full flex flex_col items_center justify_center gap_4;
                {format!("Count: {}", self.count)}
                Button("increment") {
                    label: "Add one"
                    on_click: cx.listener(|this, _, _, cx| {
                        this.count = this.count.saturating_add(1);
                        cx.notify();
                    })
                }
                Button("reset") {
                    label: "Reset"
                    on_click: cx.listener(|this, _, _, cx| {
                        this.count = 0;
                        cx.notify();
                    })
                }
            }
        }
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        if let Err(error) = cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| Counter::default())
        }) {
            eprintln!("Could not open the counter window: {error}");
            cx.quit();
        }
    });
}
