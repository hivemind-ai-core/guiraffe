//! The GUIraffe side of the Electron comparison (plans/ELECTRON-COMPARISON.md).
//!
//! `cargo run --release [-- --bench <scenario>] [-- --print-row0]`; the contract shared with
//! the Electron app is `examples/compare/README.md`.

use guiraffe_compare::{bench, data, shell};

use gpui_kit::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};

#[derive(Clone, Copy)]
struct StartupTrace(Option<std::time::Instant>);

impl StartupTrace {
    fn mark(self, phase: &str) {
        if let Some(start) = self.0 {
            eprintln!(
                "COMPARE_STARTUP {phase} {:.3}",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
    }
}

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
    let trace =
        StartupTrace(std::env::var_os("COMPARE_TRACE_STARTUP").map(|_| std::time::Instant::now()));
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

    // `--route /grid`: open on another page (screenshots, manual checks).
    let initial = arg_value("--route")
        .and_then(|r| {
            shell::PAGES
                .iter()
                .find(|(p, _)| guiraffe_router::Route::to_path(p) == r)
        })
        .map_or(shell::Page::Dashboard, |(p, _)| *p);
    let application = gpui_kit::application();
    trace.mark("platform-created");
    application
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(move |cx| {
            trace.mark("app-running");
            guiraffe_compare::init(cx);
            trace.mark("app-initialized");
            #[cfg(target_os = "linux")]
            {
                // Match the shell's fonts on the background executor while
                // open_window initializes the renderer.
                let text_system = cx.text_system().clone();
                let family = cx
                    .global::<gpui_kit::component::Theme>()
                    .font_family
                    .clone();
                cx.background_executor()
                    .spawn(async move {
                        use gpui_kit::{FontWeight, font};

                        let normal = font(family);
                        let mut bold = normal.clone();
                        bold.weight = FontWeight::BOLD;
                        text_system.prewarm_fonts(&[normal, bold]);
                        trace.mark("fonts-prewarmed");
                    })
                    .detach();
            }
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
                trace.mark("window-created");
                let shell = cx.new(|cx| shell::Shell::new(initial, window, cx));
                if let Some(name) = scenario {
                    let s = shell.clone();
                    window.on_next_frame(move |window, cx| {
                        trace.mark("first-frame-callback");
                        bench::start(name, s, bench::epoch_ms(), window, cx)
                    });
                }
                let root = guiraffe_overlays::root(shell, window, cx);
                trace.mark("root-created");
                root
            });
            trace.mark("open-window-returned");
            if let Err(e) = opened {
                eprintln!("failed to open the window: {e}");
                cx.quit();
            }
        });
}
