//! The GUIraffe side of the Electron comparison (plans/ELECTRON-COMPARISON.md).
//!
//! `cargo run --release [-- --bench <scenario>] [-- --print-row0]`; the contract shared with
//! the Electron app is `examples/compare/README.md`.

mod bench;
mod data;
mod pages;
mod shell;
mod validate;

use gpui_kit::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};

fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
        if let Some(v) = a.strip_prefix(&format!("{flag}=")) {
            return Some(v.to_string());
        }
    }
    None
}

fn main() {
    if std::env::args().any(|a| a == "--print-row0") {
        let rows = data::generate(1);
        if let Some(r) = rows.first() {
            println!("{}", data::row_json(r));
        }
        return;
    }
    let scenario = arg_value("--bench").or_else(|| std::env::var("COMPARE_BENCH").ok());
    if let Some(s) = &scenario
        && !bench::SCENARIOS.contains(&s.as_str())
    {
        eprintln!(
            "unknown scenario {s:?}; expected one of {}",
            bench::SCENARIOS.join(", ")
        );
        std::process::exit(2);
    }

    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(move |cx| {
            guiraffe_components::init(cx);
            guiraffe_overlays::init(cx);
            guiraffe_chat::init(cx);
            guiraffe_datagrid::init(cx);
            shell::init(cx);
            let opts = WindowOptions {
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: Some("Compare: GUIraffe".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(800.)),
                    cx,
                ))),
                ..Default::default()
            };
            let scenario = scenario.clone();
            let opened = cx.open_window(opts, move |window, cx| {
                let shell = cx.new(|cx| shell::Shell::new(window, cx));
                if let Some(name) = scenario {
                    let s = shell.clone();
                    window.on_next_frame(move |window, cx| {
                        bench::start(name, s, bench::epoch_ms(), window, cx)
                    });
                }
                guiraffe_overlays::root(shell, window, cx)
            });
            if let Err(e) = opened {
                eprintln!("failed to open the window: {e}");
                cx.quit();
            }
        });
}
