//! Throwaway real GPUI app for the Linux visual/e2e spike.
use gpui_kit::{prelude::*, *};
use guiraffe_test::e2e::{Config, Probe, Registry, Target};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

struct Scene {
    registry: Registry,
    clicks: usize,
    typed: String,
    focus: FocusHandle,
    _probe: Probe,
}
impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let registry = self.registry.clone();
        let clicks = self.clicks;
        let typed = self.typed.clone();
        div()
            .size_full()
            .bg(rgb(0x17202a))
            .p_4()
            .gap_4()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "backspace" {
                    this.typed.pop();
                } else if let Some(text) = &event.keystroke.key_char {
                    this.typed.push_str(text);
                }
                cx.notify();
            }))
            .child(
                div()
                    .id("save")
                    .debug_selector(|| "save".into())
                    .relative()
                    .w(px(200.))
                    .h(px(64.))
                    .bg(rgb(0x2471a3))
                    .text_color(rgb(0xffffff))
                    .child(format!("Save ({clicks})"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clicks += 1;
                        cx.notify();
                    }))
                    .child(
                        canvas(
                            move |bounds, _, _| bounds,
                            move |bounds, _, _, _| {
                                let _ = registry.publish(BTreeMap::from([(
                                    "save".into(),
                                    Target {
                                        window: "main".into(),
                                        bounds: [
                                            bounds.origin.x.into(),
                                            bounds.origin.y.into(),
                                            bounds.size.width.into(),
                                            bounds.size.height.into(),
                                        ],
                                        state: json!({"clicks":clicks,"typed":typed}),
                                    },
                                )]));
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            )
            .child(div().w(px(120.)).h(px(40.)).bg(rgb(0xe67e22)))
    }
}
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let socket = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("socket argument required"))?,
    );
    let capture_dir = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("capture dir required"))?,
    );
    let registry = Registry::default();
    let probe = Probe::start(
        Config {
            socket,
            capture_dir,
            window_title: "GUIraffe visual spike".into(),
        },
        registry.clone(),
    )?;
    gpui_kit::application().run(move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(320.), px(200.)),
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("GUIraffe visual spike".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, |window, cx| {
            eprintln!(
                "GPUI GPU: {:?}; scale: {}",
                window.gpu_specs(),
                window.scale_factor()
            );
            #[cfg(feature = "offscreen")]
            eprintln!(
                "native render_to_image: {:?}",
                window.render_to_image().map(|image| image.dimensions())
            );
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            cx.new(|_| Scene {
                registry,
                clicks: 0,
                typed: String::new(),
                focus,
                _probe: probe,
            })
        }) {
            eprintln!("open window: {error:#}");
            cx.quit();
        }
    });
    Ok(())
}
