//! Throwaway investigation of GPUI 0.3.12's Linux WGPU offscreen path.
use gpui_kit::{
    AppContext, Context, HeadlessAppContext, IntoElement, Render, Styled, Window, div, prelude::*,
    px, rgb, size,
};
use std::{path::PathBuf, sync::Arc};
struct Scene;
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x17202a))
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .w(px(200.))
                    .h(px(64.))
                    .bg(rgb(0x2471a3))
                    .text_color(rgb(0xffffff))
                    .child("Save (0)"),
            )
            .child(div().w(px(120.)).h(px(40.)).bg(rgb(0xe67e22)))
    }
}
fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("output directory required"))?,
    );
    std::fs::create_dir_all(&output)?;
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut baseline = None;
    for frame in 0..5 {
        let mut cx = HeadlessAppContext::with_platform(
            text.clone(),
            Arc::new(()),
            gpui_kit::platform::current_headless_renderer,
        );
        let window = cx.open_window(size(px(320.), px(200.)), |_, cx| cx.new(|_| Scene))?;
        cx.update_window(window.into(), |_, window, cx| {
            let _ = window.draw(cx);
        })?;
        cx.run_until_parked();
        let image = cx.capture_screenshot(window.into())?;
        anyhow::ensure!(
            image.dimensions() == (640, 400),
            "wrong image size: {:?} (GPUI TestWindow defaults to scale 2)",
            image.dimensions()
        );
        anyhow::ensure!(
            image.pixels().any(|p| p.0[..3] == [0x24, 0x71, 0xa3]),
            "missing scene"
        );
        image.save(output.join(format!("offscreen-{frame}.png")))?;
        if let Some(first) = &baseline {
            anyhow::ensure!(first == &image, "pixels differ on frame {frame}");
        } else {
            baseline = Some(image);
        }
        eprintln!("offscreen {frame}: scene present, pixels match baseline");
    }
    Ok(())
}
