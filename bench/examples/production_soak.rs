//! Headless draw and process RSS check for the two scale fixtures.
use anyhow::{Context as _, Result, bail, ensure};
use gpui_kit::{
    App, AppContext, Context, HeadlessAppContext, IntoElement, Render, Window, div, px, size,
};
use guiraffe_code::{CodeDocument, CodeHandle, CodeViewer, Position, SourceRange};
use guiraffe_datagrid::{CellPos, CellRef, ColumnDef, DataGrid, FnSource, GridHandle};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const ROWS: usize = 10_000_000;
const INTERVAL: Duration = Duration::from_millis(16);
const REPORT_INTERVAL: Duration = Duration::from_secs(10);

struct Empty;

impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[derive(Clone)]
enum Host {
    Grid(GridHandle),
    Code(CodeHandle),
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Grid(handle) => DataGrid::new(handle).into_any_element(),
            Self::Code(handle) => CodeViewer::new(handle).into_any_element(),
        }
    }
}

impl Host {
    fn interact(&self, step: usize, cx: &mut App) {
        // Revisit hot, mid-document and end positions under repeated selection.
        let row = match step % 3 {
            0 => step % 96,
            1 => ROWS / 2 + step % 96,
            _ => ROWS - 1 - step % 96,
        };
        match self {
            Self::Grid(grid) => {
                grid.select(CellPos { row, col: step % 6 }, false, false, cx);
                grid.scroll_to_row(row, cx);
            }
            Self::Code(code) => {
                code.select(
                    Some(SourceRange::new(
                        Position::new(row, 0),
                        Position::new(row, 1),
                    )),
                    cx,
                );
                code.scroll_to_line(row, cx);
            }
        }
    }
}

fn rss_kib() -> Result<u64> {
    std::fs::read_to_string("/proc/self/status")?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|line| line.split_whitespace().next())
        .context("missing process VmRSS")?
        .parse()
        .context("invalid process VmRSS")
}

fn report(samples: &mut [f64], elapsed: Duration) -> Result<(f64, f64, u64)> {
    samples.sort_unstable_by(f64::total_cmp);
    let percentile = |p: usize| samples[(samples.len() * p).div_ceil(100) - 1];
    let p99 = percentile(99);
    let max = *samples.last().context("no frames recorded")?;
    let rss = rss_kib()?;
    println!(
        "elapsed_s={:.2} frames={} p50_ms={:.3} p95_ms={:.3} p99_ms={p99:.3} max_ms={max:.3} rss_kib={rss}",
        elapsed.as_secs_f64(),
        samples.len(),
        percentile(50),
        percentile(95)
    );
    Ok((p99, max, rss))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let component = args
        .next()
        .context("use: production_soak grid|code [--nightly] [--no-renderer]")?;
    let mut nightly = false;
    let mut renderer = true;
    for arg in args {
        match arg.as_str() {
            "--nightly" => nightly = true,
            "--no-renderer" => renderer = false,
            other => bail!("unknown argument {other}"),
        }
    }
    ensure!(
        !nightly || !cfg!(debug_assertions),
        "nightly needs a release build"
    );
    run(&component, nightly, renderer)
}

fn run(component: &str, nightly: bool, renderer: bool) -> Result<()> {
    let document = match component {
        "grid" => None,
        "code" => Some(Arc::new(CodeDocument::new(Arc::<str>::from(
            "x\n".repeat(ROWS - 1) + "x",
        )))),
        _ => bail!("unknown component {component}"),
    };
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app =
        HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), move || {
            if renderer {
                gpui_kit::platform::current_headless_renderer()
            } else {
                Ok(None)
            }
        });
    let window = app.open_window(size(px(1200.), px(800.)), |_, cx| cx.new(|_| Empty))?;
    app.update_window(window.into(), |_, window, cx| {
        let _ = window.draw(cx);
    })?;
    app.run_until_parked();

    let mount_start = Instant::now();
    let mut control = None;
    app.update_window(window.into(), |_, window, cx| {
        window.replace_root(cx, |_, cx| {
            let host = match document {
                Some(document) => Host::Code(CodeHandle::new(1, document, cx)),
                None => {
                    let source =
                        FnSource::new(ROWS, |row, col| CellRef::Int((row * 100 + col) as i64));
                    let columns = (0..50)
                        .map(|col| ColumnDef::new(format!("c{col}"), format!("Col {col}"), col))
                        .collect();
                    Host::Grid(GridHandle::new(source, columns, cx))
                }
            };
            control = Some(host.clone());
            host
        });
    })?;
    let control = control.context("missing mounted component")?;
    let mut step = 0;
    let mut frame = |pace: bool| -> Result<f64> {
        let start = Instant::now();
        app.update(|cx| control.interact(step, cx));
        app.update_window(window.into(), |_, window, cx| {
            let _ = window.draw(cx);
        })?;
        app.run_until_parked();
        match &control {
            Host::Grid(grid) => ensure!(grid.stats().row_renders > 0 && grid.stats().pool < 100),
            Host::Code(code) => {
                ensure!(code.stats().text_renders > 0 && code.stats().last_visible_rows < 100)
            }
        }
        step += 1;
        let elapsed = start.elapsed();
        if pace {
            thread::sleep(INTERVAL.saturating_sub(elapsed));
        }
        Ok(elapsed.as_secs_f64() * 1000.)
    };
    frame(false)?;
    let mount_ms = mount_start.elapsed().as_secs_f64() * 1000.;
    println!("mount_to_first_draw_ms={mount_ms:.3}");
    if nightly {
        ensure!(mount_ms <= 200., "headless mount budget exceeded");
    }
    for _ in 1..if nightly { 600 } else { 120 } {
        frame(nightly)?;
    }
    let baseline = rss_kib()?;
    let start = Instant::now();
    let mut deadline = REPORT_INTERVAL;
    let mut samples = Vec::with_capacity(1_024);
    loop {
        samples.push(frame(nightly)?);
        if (!nightly && samples.len() == 60) || start.elapsed() >= deadline {
            let (p99, max, rss) = report(&mut samples, start.elapsed())?;
            if !nightly {
                break;
            }
            ensure!(
                rss * 100 <= baseline * 102,
                "RSS grew from {baseline} to {rss} KiB"
            );
            ensure!(max <= 50., "long headless draw task: {max:.3} ms");
            ensure!(p99 <= 8. && max <= 16., "headless frame budget exceeded");
            if deadline == Duration::from_secs(30 * 60) {
                break;
            }
            samples.clear();
            deadline += REPORT_INTERVAL;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_production_smoke() -> Result<()> {
        run("grid", false, true)
    }

    #[test]
    fn code_production_smoke() -> Result<()> {
        run("code", false, true)
    }
}
