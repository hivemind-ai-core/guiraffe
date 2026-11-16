//! `cargo run -p guiraffe-showcase [-- <entry name>]`
use gpui_kit::WindowOptions;

fn main() {
    let start = std::env::args().nth(1);
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(move |cx| {
            guiraffe_showcase::init(cx);
            let opts = WindowOptions {
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: Some("GUIraffe showcase".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let start = start.clone();
            if let Err(e) = cx.open_window(opts, move |window, cx| {
                guiraffe_showcase::window_root(start.as_deref(), window, cx)
            }) {
                eprintln!("failed to open showcase: {e}");
                cx.quit();
            }
        });
}
