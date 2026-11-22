use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use gpui_kit::{AppContext, Context, Entity, IntoElement, Render, TestAppContext, Window};
use guiraffe_calendar::{
    Calendar, CalendarItem, CalendarModel, CalendarSource, CalendarState, CalendarViewKind,
    CalendarZone, DragMode, EventId, SourceId, init,
};
use guiraffe_core::ColorName;

struct Host {
    state: Entity<CalendarState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Calendar::new(&self.state)
    }
}

fn model() -> CalendarModel {
    let epoch = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH);
    let items = (0..10_000u64).map(|index| {
        let start = epoch
            + chrono::Duration::days((index % 365) as i64)
            + chrono::Duration::hours((index % 16 + 6) as i64)
            + chrono::Duration::minutes((index % 4 * 15) as i64);
        Arc::new(
            CalendarItem::timed(
                EventId(index + 1),
                SourceId(index % 4 + 1),
                "Scale fixture",
                start,
                start + chrono::Duration::minutes(30),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        )
    });
    let sources = (1..=4).map(|id| CalendarSource::new(SourceId(id), "Source", ColorName::Blue));
    CalendarModel::new(items, sources).unwrap_or_else(|error| panic!("{error}"))
}

/// Adversarial fixture: all 10,000 items overlap inside one week of January.
fn dense_week_model() -> CalendarModel {
    let epoch = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH);
    let items = (0..10_000u64).map(|index| {
        let start = epoch
            + chrono::Duration::days((index % 7) as i64)
            + chrono::Duration::hours(8)
            + chrono::Duration::minutes((index % 480) as i64);
        Arc::new(
            CalendarItem::timed(
                EventId(index + 1),
                SourceId(index % 4 + 1),
                "Dense fixture",
                start,
                start + chrono::Duration::minutes(60),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        )
    });
    let sources = (1..=4).map(|id| CalendarSource::new(SourceId(id), "Source", ColorName::Blue));
    CalendarModel::new(items, sources).unwrap_or_else(|error| panic!("{error}"))
}

fn measure_with(
    cx: &mut TestAppContext,
    view: CalendarViewKind,
    model: fn() -> CalendarModel,
) -> (Duration, Duration) {
    cx.update(init);
    let start = Instant::now();
    let (host, vcx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|cx| {
            CalendarState::new(
                model(),
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap_or(NaiveDate::MIN),
                CalendarZone::Utc,
                cx,
            )
        });
        state.update(cx, |state, cx| {
            state.set_live_now(false, cx);
            state.set_view(view, cx);
        });
        Host { state }
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let mount = start.elapsed();
    let state = vcx.update(|_, cx| host.read(cx).state.clone());
    let anchor = vcx.update(|_, cx| state.read(cx).anchor());
    for _ in 0..12 {
        match view {
            CalendarViewKind::Month => {
                vcx.update(|_, cx| state.update(cx, |state, cx| state.navigate(1, cx)))
            }
            CalendarViewKind::Week => vcx.update(|_, cx| {
                state.update(cx, |state, cx| {
                    state.begin_item_drag(EventId(1), DragMode::Move, anchor, 360, cx);
                    state.update_timed_drag(anchor, 360, cx);
                })
            }),
            _ => {}
        }
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
    let mut frames = Vec::with_capacity(24);
    let mut updates = Vec::with_capacity(24);
    let mut draws = Vec::with_capacity(24);
    for iteration in 0..24 {
        let start = Instant::now();
        match view {
            CalendarViewKind::Month => {
                vcx.update(|_, cx| state.update(cx, |state, cx| state.navigate(1, cx)))
            }
            CalendarViewKind::Week => vcx.update(|_, cx| {
                state.update(cx, |state, cx| {
                    state.begin_item_drag(EventId(1), DragMode::Move, anchor, 360, cx);
                    state.update_timed_drag(anchor, 360 + iteration * 15, cx);
                })
            }),
            _ => {}
        }
        // `VisualTestContext::update` flushes effects and redraws the notified
        // window, so the update already includes the frame. A forced second
        // `window.draw` would count two frames per input; instead, settle any
        // deferred paint-time work and count it.
        updates.push(start.elapsed());
        let settle_start = Instant::now();
        vcx.run_until_parked();
        draws.push(settle_start.elapsed());
        frames.push(start.elapsed());
    }
    frames.sort();
    updates.sort();
    draws.sort();
    eprintln!(
        "calendar {view:?} phase maxima: update+frame={:?}, settle={:?}",
        updates.last(),
        draws.last()
    );
    let p99 = frames[(frames.len() * 99 / 100)
        .saturating_sub(1)
        .min(frames.len() - 1)];
    (mount, p99)
}

fn rss_pages() -> Option<usize> {
    std::fs::read_to_string("/proc/self/statm")
        .ok()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[gpui_kit::test]
#[ignore = "30-minute 10k-event soak; set GUIRAFFE_SOAK_SECONDS to override for smoke runs"]
fn week_soak_has_flat_post_warmup_memory(cx: &mut TestAppContext) {
    let seconds = std::env::var("GUIRAFFE_SOAK_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1_800);
    cx.update(init);
    let (host, vcx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|cx| {
            CalendarState::new(
                model(),
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap_or(NaiveDate::MIN),
                CalendarZone::Utc,
                cx,
            )
        });
        state.update(cx, |state, cx| {
            state.set_live_now(false, cx);
            state.set_view(CalendarViewKind::Week, cx);
        });
        Host { state }
    });
    let state = vcx.update(|_, cx| host.read(cx).state.clone());
    let anchor = vcx.update(|_, cx| state.read(cx).anchor());
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut iteration = 0i32;
    let mut samples = Vec::new();
    let mut last_sample = Instant::now() - Duration::from_secs(10);
    while Instant::now() < deadline {
        vcx.update(|_, cx| {
            state.update(cx, |state, cx| {
                state.begin_item_drag(EventId(1), DragMode::Move, anchor, 360, cx);
                state.update_timed_drag(anchor, 360 + (iteration % 16) * 15, cx);
            })
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        iteration = iteration.wrapping_add(1);
        if last_sample.elapsed() >= Duration::from_secs(10) {
            if let Some(rss) = rss_pages() {
                samples.push(rss);
            }
            last_sample = Instant::now();
        }
    }
    eprintln!("calendar 10k week soak: {seconds}s, {iteration} frames, RSS pages {samples:?}");
    if let (Some(first), Some(last)) = (samples.first(), samples.last()) {
        let allowed = first / 50 + 1;
        assert!(
            *last <= first + allowed,
            "post-warm-up RSS drift exceeded 2%: {samples:?}"
        );
    }
}

fn load_average() -> Option<f32> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[gpui_kit::test]
#[ignore = "release-only 10k-event performance gate"]
fn month_and_week_meet_ten_thousand_event_budgets(cx: &mut TestAppContext) {
    let cpus = std::thread::available_parallelism()
        .map(|value| value.get() as f32)
        .unwrap_or(1.0);
    // GUIRAFFE_INDICATIVE=1 prints numbers on a busy machine but never accepts
    // them: the budgets are only asserted in a quiet window.
    let indicative = std::env::var("GUIRAFFE_INDICATIVE").is_ok_and(|value| value == "1");
    let quiet = load_average().is_none_or(|load| load <= cpus);
    if !quiet && !indicative {
        panic!(
            "quiet-window precondition failed: load average {:?} exceeds {cpus} CPUs",
            load_average()
        );
    }
    let mut failures = Vec::new();
    for (view, fixture, model) in [
        (
            CalendarViewKind::Month,
            "spread",
            model as fn() -> CalendarModel,
        ),
        (CalendarViewKind::Week, "spread", model),
        (CalendarViewKind::Week, "dense-week", dense_week_model),
    ] {
        let (mount, p99) = measure_with(cx, view, model);
        eprintln!(
            "calendar {view:?} ({fixture}): 10k mount={mount:?}, warm interaction p99={p99:?}{}",
            if quiet {
                ""
            } else {
                " [INDICATIVE: machine busy, not accepted]"
            }
        );
        if mount > Duration::from_millis(200) {
            failures.push(format!("{view:?} mount exceeded 200 ms: {mount:?}"));
        }
        if p99 > Duration::from_millis(8) {
            failures.push(format!("{view:?} p99 exceeded 8 ms: {p99:?}"));
        }
    }
    if quiet {
        assert!(failures.is_empty(), "{}", failures.join("; "));
    }
}
