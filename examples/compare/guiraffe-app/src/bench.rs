//! `--bench <scenario>`: scripted, deterministic scenarios that print one `BENCH_JSON` line
//! and exit (contract: `examples/compare/README.md`, "Bench mode").
//!
//! Frame times come from a continuous `Window::on_next_frame` chain (the GPUI analogue of a
//! re-armed `requestAnimationFrame`). "Settled" = the first frame presented after the
//! action's result is visible.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::Write as _,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures::channel::oneshot;
use gpui_kit::{App, AsyncWindowContext, Entity, Window, px, size};
use guiraffe_datagrid::{GridHandle, SortDir, SortKey};
use serde_json::{Value, json};

use crate::pages::name_filter;
use crate::shell::{Page, Shell, toggle_theme};

pub const SCENARIOS: [&str; 9] = [
    "startup",
    "idle",
    "grid-fling",
    "sort",
    "filter",
    "theme",
    "chat",
    "resize",
    "load",
];
const CHAT_PROMPT: &str = "Explain the render pipeline.";

pub fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |d| d.as_secs_f64() * 1000.)
}

/// Process start (epoch ms) from `/proc/self/stat` field 22 and `btime`; 10 ms resolution.
pub fn process_start_epoch_ms() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Fields after the parenthesised command name; field 22 is the 20th of those.
    let rest = stat.rsplit_once(')')?.1;
    let ticks: f64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    let btime: f64 = std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    Some((btime + ticks / 100.) * 1000.)
}

#[derive(Default)]
struct Rec {
    running: bool,
    last: Option<Instant>,
    frames: Vec<f64>,
}

/// A continuous frame-callback chain recording intervals between frames.
#[derive(Clone, Default)]
struct Frames(Rc<RefCell<Rec>>);

impl Frames {
    fn start(window: &Window) -> Self {
        let f = Self::default();
        f.0.borrow_mut().running = true;
        f.arm(window);
        f
    }
    fn arm(&self, window: &Window) {
        let this = self.clone();
        window.on_next_frame(move |window, _| {
            {
                let mut r = this.0.borrow_mut();
                if !r.running {
                    return;
                }
                let now = Instant::now();
                if let Some(last) = r.last {
                    r.frames.push((now - last).as_secs_f64() * 1000.);
                }
                r.last = Some(now);
            }
            this.arm(window);
        });
    }
    fn stop(&self) -> Vec<f64> {
        let mut r = self.0.borrow_mut();
        r.running = false;
        std::mem::take(&mut r.frames)
    }
}

type Cx = AsyncWindowContext;

fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

async fn next_frame(cx: &mut Cx) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    cx.on_next_frame(move |_, _| {
        let _ = tx.send(());
    });
    rx.await.map_err(err)
}

async fn sleep(cx: &mut Cx, ms: u64) {
    cx.background_executor()
        .timer(Duration::from_millis(ms))
        .await;
}

fn with<R>(cx: &mut Cx, f: impl FnOnce(&mut Window, &mut App) -> R) -> Result<R, String> {
    cx.update(f).map_err(err)
}

async fn navigate(cx: &mut Cx, shell: &Entity<Shell>, page: Page) -> Result<(), String> {
    with(cx, |_, cx| {
        let router = shell.read(cx).router.clone();
        router.navigate(page, cx);
    })?;
    next_frame(cx).await?;
    next_frame(cx).await
}

async fn open_grid(cx: &mut Cx, shell: &Entity<Shell>) -> Result<GridHandle, String> {
    navigate(cx, shell, Page::Grid).await?;
    let grid = with(cx, |_, cx| {
        shell
            .read(cx)
            .grid
            .as_ref()
            .map(|g| g.read(cx).grid.clone())
    })?
    .ok_or("grid page was not created")?;
    sleep(cx, 500).await;
    Ok(grid)
}

/// Run `action`, then wait until the grid is idle and one more frame is presented.
async fn settled(
    cx: &mut Cx,
    grid: Option<&GridHandle>,
    action: impl FnOnce(&mut Window, &mut App),
) -> Result<f64, String> {
    let t0 = Instant::now();
    with(cx, action)?;
    loop {
        next_frame(cx).await?;
        if grid.is_none_or(|g| !g.is_busy()) {
            break;
        }
    }
    if grid.is_some() {
        next_frame(cx).await?;
    }
    Ok(t0.elapsed().as_secs_f64() * 1000.)
}

/// One viewport page down per frame, wrapping at the end; `pages` frames or `ms` elapsed.
async fn fling(cx: &mut Cx, grid: &GridHandle, pages: usize, ms: u64) -> Result<(), String> {
    let end = Instant::now() + Duration::from_millis(ms);
    for _ in 0..pages {
        if Instant::now() >= end {
            break;
        }
        with(cx, |_, cx| {
            let page = f64::from(f32::from(grid.bounds().size.height)).max(100.);
            let total = grid.view_row_count() as f64 * f64::from(grid.row_height());
            let (x, y) = grid.scroll_offset();
            let next = y + page;
            grid.set_scroll(x, if next + page > total { 0. } else { next }, cx);
        })?;
        next_frame(cx).await?;
    }
    Ok(())
}

async fn stream_chat(
    cx: &mut Cx,
    shell: &Entity<Shell>,
    ms: u64,
) -> Result<(Vec<f64>, usize), String> {
    navigate(cx, shell, Page::Chat).await?;
    let chat = with(cx, |_, cx| shell.read(cx).chat.clone())?.ok_or("chat page missing")?;
    sleep(cx, 500).await;
    let frames = with(cx, |window, cx| {
        let frames = Frames::start(window);
        chat.update(cx, |c, cx| c.send(CHAT_PROMPT, cx));
        frames
    })?;
    sleep(cx, ms).await;
    let frames = frames.stop();
    let tokens = with(cx, |_, cx| {
        chat.read(cx)
            .tokens
            .load(std::sync::atomic::Ordering::Relaxed)
    })?;
    Ok((frames, tokens))
}

pub struct Report {
    frames: Vec<f64>,
    timings: BTreeMap<&'static str, Vec<f64>>,
    info: BTreeMap<&'static str, Value>,
}

async fn scenario(name: &str, shell: &Entity<Shell>, cx: &mut Cx) -> Result<Report, String> {
    let mut r = Report {
        frames: Vec::new(),
        timings: BTreeMap::new(),
        info: BTreeMap::new(),
    };
    match name {
        "startup" => {}
        "idle" => {
            navigate(cx, shell, Page::Form).await?;
            let secs = std::env::var("BENCH_IDLE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(95_u64);
            sleep(cx, secs * 1000).await;
            r.info.insert("idle_secs", json!(secs));
        }
        "grid-fling" => {
            let grid = open_grid(cx, shell).await?;
            let frames = with(cx, |w, _| Frames::start(w))?;
            let t0 = Instant::now();
            fling(cx, &grid, 200, u64::MAX / 4).await?;
            r.timings
                .insert("total_ms", vec![t0.elapsed().as_secs_f64() * 1000.]);
            r.frames = frames.stop();
            r.info.insert("rows", json!(grid.view_row_count()));
        }
        "sort" => {
            let grid = open_grid(cx, shell).await?;
            let (mut sort, mut unsort) = (Vec::new(), Vec::new());
            for _ in 0..5 {
                let keys = vec![SortKey {
                    column: 7,
                    dir: SortDir::Desc,
                }];
                sort.push(settled(cx, Some(&grid), |_, cx| grid.set_sort(keys, cx)).await?);
                unsort.push(settled(cx, Some(&grid), |_, cx| grid.set_sort(vec![], cx)).await?);
            }
            r.timings.insert("sort_ms", sort);
            r.timings.insert("unsort_ms", unsort);
            r.info.insert("rows", json!(grid.view_row_count()));
        }
        "filter" => {
            let grid = open_grid(cx, shell).await?;
            let (mut filter, mut clear, mut matched) = (Vec::new(), Vec::new(), 0);
            for _ in 0..5 {
                filter.push(
                    settled(cx, Some(&grid), |_, cx| {
                        grid.set_filter(1, name_filter("ada"), cx)
                    })
                    .await?,
                );
                matched = grid.view_row_count();
                clear.push(settled(cx, Some(&grid), |_, cx| grid.set_filter(1, None, cx)).await?);
            }
            r.timings.insert("filter_ms", filter);
            r.timings.insert("clear_ms", clear);
            r.info.insert("rows", json!(grid.view_row_count()));
            r.info.insert("filtered_rows", json!(matched));
        }
        "theme" => {
            sleep(cx, 500).await;
            let mut t = Vec::new();
            for _ in 0..10 {
                t.push(settled(cx, None, |_, cx| toggle_theme(cx)).await?);
            }
            r.timings.insert("theme_ms", t);
        }
        "chat" => {
            let (frames, tokens) = stream_chat(cx, shell, 10_000).await?;
            r.frames = frames;
            r.info.insert("tokens", json!(tokens));
        }
        "resize" => {
            let frames = with(cx, |w, _| Frames::start(w))?;
            for k in 0..120 {
                let w = 1000. + 280. * (f64::from(k) / 10.).sin().abs();
                with(cx, |window, _| window.resize(size(px(w as f32), px(800.))))?;
                next_frame(cx).await?;
            }
            r.frames = frames.stop();
        }
        "load" => {
            phase("grid");
            let grid = open_grid(cx, shell).await?;
            let frames = with(cx, |w, _| Frames::start(w))?;
            fling(cx, &grid, usize::MAX, 10_000).await?;
            let grid_frames = frames.stop();
            phase("chat");
            let (chat_frames, tokens) = stream_chat(cx, shell, 10_000).await?;
            r.frames = grid_frames.iter().chain(&chat_frames).copied().collect();
            r.timings.insert("grid_frames_ms", grid_frames);
            r.timings.insert("chat_frames_ms", chat_frames);
            r.info.insert("tokens", json!(tokens));
        }
        other => return Err(format!("unknown bench scenario: {other}")),
    }
    Ok(r)
}

fn phase(name: &str) {
    println!("BENCH_PHASE {name}");
    let _ = std::io::stdout().flush();
}

/// Start `name` once the first frame has been presented (`first_frame` = its epoch ms).
pub fn start(
    name: String,
    shell: Entity<Shell>,
    first_frame: f64,
    window: &mut Window,
    cx: &mut App,
) {
    window
        .spawn(cx, async move |cx| {
            let result = scenario(&name, &shell, cx).await;
            match result {
                Ok(r) => {
                    let start = process_start_epoch_ms();
                    let out = json!({
                        "app": "guiraffe",
                        "scenario": name,
                        "first_frame_epoch_ms": first_frame,
                        "process_start_epoch_ms": start,
                        "first_frame_since_process_start_ms": start.map(|s| first_frame - s),
                        "frames_ms": r.frames,
                        "timings_ms": r.timings,
                        "info": r.info,
                    });
                    println!("BENCH_JSON {out}");
                    let _ = std::io::stdout().flush();
                    let _ = cx.update(|_, cx| cx.quit());
                }
                Err(e) => {
                    eprintln!("BENCH_FAIL {e}");
                    std::process::exit(1);
                }
            }
        })
        .detach();
}
