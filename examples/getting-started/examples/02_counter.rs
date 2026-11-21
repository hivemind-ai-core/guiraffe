//! Step 2: state, events and the `ui!` macro.
//!
//! `cargo run -p getting-started-examples --example 02_counter --features gpui-fast`

use gpui_kit::{
    App, AppContext, Context, IntoElement, Render, SharedString, Window, WindowOptions,
    component::{
        Disableable,
        button::{Button, ButtonVariants},
    },
    div,
    prelude::*,
};
use guiraffe_components::{Card, Text, Title};
use guiraffe_ui::{component, ui};

/// A stateless component: props in, elements out. `#[component]` generates
/// `Stat::new(label, value)` plus a builder method for each optional prop.
#[component]
fn Stat(
    label: &'static str,
    value: SharedString,
    #[prop(default)] hint: Option<&'static str>,
) -> impl IntoElement {
    ui! {
        div {
            flex flex_col items_center gap_1
            Text(label) { dimmed: true }
            Title(value) { order: 1 }
            if let Some(hint) = hint { Text(hint) { dimmed: true } }
        }
    }
}

/// The view owns its state. Rendering reads it; listeners mutate it.
#[derive(Default)]
struct Counter {
    count: i64,
}

impl Counter {
    fn change(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.count = self.count.saturating_add(delta);
        // Ask GPUI to render this view again. Nothing else re-renders.
        cx.notify();
    }
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.count;
        ui! {
            div {
                size_full flex items_center justify_center
                Card {
                    div {
                        flex flex_col items_center gap_4 p_4
                        Stat("Count", format!("{count}").into()) {
                            hint: (count == 0).then_some("Press +1 to start")
                        }
                        div {
                            flex gap_2
                            Button("decrement") {
                                label: "−1"
                                disabled: count == 0
                                on_click: cx.listener(|this, _, _, cx| this.change(-1, cx))
                            }
                            Button("increment") {
                                label: "+1";
                                .primary()
                                on_click: cx.listener(|this, _, _, cx| this.change(1, cx))
                            }
                            Button("reset") {
                                label: "Reset";
                                .ghost()
                                on_click: cx.listener(|this, _, _, cx| {
                                    this.count = 0;
                                    cx.notify();
                                })
                            }
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            guiraffe_components::init(cx);
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let counter = cx.new(|_| Counter::default());
                cx.new(|cx| gpui_kit::component::Root::new(counter, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the counter window: {error}");
                cx.quit();
            }
        });
}
