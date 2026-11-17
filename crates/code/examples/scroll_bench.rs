//! Deterministic headless draw smoke measurement (not a desktop input-latency benchmark).
//! `cargo run -p guiraffe-code --example scroll_bench --features gpui-fast`
use gpui_kit::{AppContext, Context, HeadlessAppContext, IntoElement, Render, Window, px, size};
use guiraffe_code::{CodeDocument, CodeHandle, CodeOptions, CodeViewer};
use std::{sync::Arc, time::Instant};
struct Host(CodeHandle);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CodeViewer::new(&self.0)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = std::thread::spawn(|| {
        use std::fmt::Write;
        let mut source = String::with_capacity(48_000_000);
        for line in 0..999_999 {
            let _ = writeln!(source, "let value_{line} = compute({line});");
        }
        Arc::new(CodeDocument::new(Arc::<str>::from(source)))
    })
    .join()
    .map_err(|_| "source worker panicked")?;
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app = HeadlessAppContext::with_platform(
        text,
        Arc::new(gpui_kit::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    let mut control = None;
    let window = app.open_window(size(px(1200.), px(800.)), |_, cx| {
        let code = CodeHandle::new(1, document, cx);
        code.set_options(
            CodeOptions {
                margin_chars: 0,
                ..Default::default()
            },
            cx,
        );
        control = Some(code.clone());
        cx.new(|_| Host(code))
    })?;
    let code = control.ok_or("missing surface")?;
    let mut samples = Vec::new();
    for step in 0..120 {
        let start = Instant::now();
        app.update(|cx| code.scroll_to_line((step * 7919) % 999_900, cx));
        app.update_window(window.into(), |_, window, cx| {
            let _ = window.draw(cx);
        })?;
        app.run_until_parked();
        if step >= 20 {
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
    }
    samples.sort_by(f64::total_cmp);
    let stats = code.stats();
    assert!(
        stats.text_renders >= 100 && stats.last_visible_rows < 100,
        "measurement actually drew virtualized frames"
    );
    println!(
        "{{\"backend\":\"{}\",\"profile\":\"{}\",\"lines\":1000000,\"frames\":100,\"p50_ms\":{:.6},\"p95_ms\":{:.6},\"visible_rows\":{},\"shaped_lines\":{},\"cache_hits\":{}}}",
        if gpui_kit::GPUI_FAST {
            "gpui-fast"
        } else {
            "upstream"
        },
        if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        },
        samples[50],
        samples[95],
        stats.last_visible_rows,
        stats.shaped_lines,
        stats.cache_hits
    );
    Ok(())
}
