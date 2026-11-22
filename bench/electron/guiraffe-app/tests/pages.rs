//! Every page renders on the headless wgpu renderer. The images are written to
//! `target/shots/<page>.png` for eyeballing (not goldens: the dashboard is live data).
//! Skips (passes) when no wgpu adapter is available.

use std::{sync::Arc, time::Duration};

use gpui_kit::{AppContext, HeadlessAppContext, px, size};
use guiraffe_compare::shell::{PAGES, Shell};

#[test]
fn every_page_renders() -> anyhow::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shots");
    std::fs::create_dir_all(&dir)?;
    for (page, label) in PAGES {
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            Arc::new(guiraffe_icons::GuiraffeAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(guiraffe_compare::init);
        let window = cx.open_window(size(px(1280.), px(800.)), move |window, cx| {
            let shell = cx.new(|cx| Shell::new(page, window, cx));
            guiraffe_overlays::root(shell, window, cx)
        })?;
        // Several frames with pauses: Markdown and syntax highlighting settle on a
        // background thread, icons load during the first frame.
        for _ in 0..8 {
            cx.update_window(window.into(), |_, window, cx| {
                let _ = window.draw(cx);
            })?;
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(50));
        }
        let image = match cx.capture_screenshot(window.into()) {
            Ok(image) => image,
            Err(e) => {
                eprintln!("skipping (no wgpu adapter?): {e:#}");
                return Ok(());
            }
        };
        let first = *image.get_pixel(image.width() - 40, image.height() - 40);
        let distinct = image.pixels().filter(|p| **p != first).count();
        assert!(distinct > 10_000, "{label} looks blank");
        image.save(dir.join(format!("{}.png", label.to_lowercase().replace(' ', "-"))))?;
    }
    Ok(())
}
