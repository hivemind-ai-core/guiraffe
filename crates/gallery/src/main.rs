//! `cargo run -p guiraffe-gallery`
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            guiraffe_gallery::init(cx);
            guiraffe_windows::WindowManager::init(guiraffe_windows::WindowConfig::default(), cx);
            if let Err(e) = guiraffe_gallery::shell::open(guiraffe_gallery::Page::Overview, cx) {
                eprintln!("failed to open gallery: {e}");
                cx.quit();
            }
        });
}
