//! Release-only 10,000-event probes (spec/27 §Performance budgets and probes).
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, canvas, div, point, px,
};
use guiraffe_calendar::{
    Calendar, CalendarItem, CalendarModel, CalendarSource, CalendarState, CalendarViewKind,
    CalendarZone, DragMode, EventId, SourceId, init,
};
use guiraffe_core::ColorName;

struct Host {
    state: Entity<CalendarState>,
    paints: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let paints = self.paints.clone();
        div().size_full().child(Calendar::new(&self.state)).child(
            canvas(|_, _, _| {}, move |_, _, _, _| paints.set(paints.get() + 1))
                .absolute()
                .size_full(),
        )
    }
}

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap_or(NaiveDate::MIN)
}

fn events(count: u64, time: impl Fn(u64) -> (DateTime<Utc>, i64)) -> CalendarModel {
    let items = (0..count).map(|index| {
        let (start, minutes) = time(index);
        Arc::new(
            CalendarItem::timed(
                EventId(index + 1),
                SourceId(index % 4 + 1),
                "Scale fixture",
                start,
                start + chrono::Duration::minutes(minutes),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        )
    });
    let sources = (1..=4).map(|id| CalendarSource::new(SourceId(id), "Source", ColorName::Blue));
    CalendarModel::new(items, sources).unwrap_or_else(|error| panic!("{error}"))
}

fn at(date: NaiveDate, minute: u64) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_time(chrono::NaiveTime::MIN))
        + chrono::Duration::minutes(minute as i64)
}

/// 10,000 items over the whole year.
fn model() -> CalendarModel {
    events(10_000, |index| {
        let date = day(1, 1) + chrono::Duration::days((index % 365) as i64);
        (at(date, (index % 16 + 6) * 60 + index % 4 * 15), 30)
    })
}

/// Adversarial: all 10,000 items in July.
fn dense_month_model() -> CalendarModel {
    events(10_000, |index| {
        let date = day(7, 1) + chrono::Duration::days((index % 31) as i64);
        (at(date, 360 + index % 900), 30)
    })
}

/// Adversarial: all 10,000 items overlap in the visible Sunday-start week of July 12.
fn dense_week_model() -> CalendarModel {
    events(10_000, |index| {
        let date = day(7, 12) + chrono::Duration::days((index % 7) as i64);
        (at(date, 480 + index % 480), 60)
    })
}

#[derive(Clone, Copy, Debug)]
enum Interaction {
    Navigate,
    Scroll,
    Select,
    Hover,
    EditPreview,
}

struct Timing {
    interaction: Interaction,
    median: Duration,
    p99: Duration,
    worst: Duration,
}

const SAMPLES: usize = 200;

/// Nearest-rank percentile of sorted samples.
fn p99(sorted: &[Duration]) -> Duration {
    let rank = (sorted.len() * 99).div_ceil(100);
    sorted
        .get(rank.saturating_sub(1))
        .copied()
        .unwrap_or_default()
}

fn measure(
    cx: &mut TestAppContext,
    view: CalendarViewKind,
    model: fn() -> CalendarModel,
) -> (Duration, Vec<Timing>) {
    cx.update(init);
    let anchor = day(7, 15);
    let paints = Rc::new(Cell::new(0));
    let counter = paints.clone();
    let start = Instant::now();
    let (host, vcx) = cx.add_window_view(move |_, cx| {
        let state = cx.new(|cx| CalendarState::new(model(), anchor, CalendarZone::Utc, cx));
        state.update(cx, |state, cx| {
            state.set_live_now(false, cx);
            state.set_view(view, cx);
        });
        Host {
            state,
            paints: counter,
        }
    });
    vcx.run_until_parked();
    let mount = start.elapsed();
    assert!(paints.get() > 0, "mount must include a painted frame");

    let state = vcx.update(|_, cx| host.read(cx).state.clone());
    let visible: Vec<EventId> = vcx.update(|_, cx| {
        let state = state.read(cx);
        let index = state.model().index(CalendarZone::Utc);
        index
            .day(anchor)
            .iter()
            .map(|item| item.id)
            .take(2)
            .collect()
    });
    assert_eq!(visible.len(), 2, "the measured range must contain events");

    let mut timings = Vec::new();
    for interaction in [
        Interaction::Navigate,
        Interaction::Scroll,
        Interaction::Select,
        Interaction::Hover,
        Interaction::EditPreview,
    ] {
        let timed_view = matches!(view, CalendarViewKind::Week | CalendarViewKind::Day);
        if matches!(interaction, Interaction::Scroll) && !timed_view
            || matches!(interaction, Interaction::EditPreview) && view == CalendarViewKind::Year
        {
            continue;
        }
        if matches!(interaction, Interaction::EditPreview) {
            vcx.update(|_, cx| {
                state.update(cx, |state, cx| {
                    if timed_view {
                        state.begin_item_drag(visible[0], DragMode::Move, anchor, 600, cx);
                    } else {
                        state.begin_all_day_create(anchor, cx);
                    }
                })
            });
        }
        let mut frames = Vec::with_capacity(SAMPLES);
        for sample in 0..SAMPLES {
            let odd = sample % 2 == 1;
            let before = paints.get();
            let start = Instant::now();
            match interaction {
                // Out and back, so every other frame is the populated range.
                Interaction::Navigate => vcx.update(|_, cx| {
                    state.update(cx, |state, cx| state.navigate(if odd { -1 } else { 1 }, cx))
                }),
                Interaction::Scroll => vcx.update(|_, cx| {
                    state.update(cx, |state, cx| {
                        state.set_scroll_minutes(if odd { 480 } else { 420 }, cx)
                    })
                }),
                Interaction::Select => vcx.update(|_, cx| {
                    state.update(cx, |state, cx| {
                        state.select_item(visible[usize::from(odd)], cx)
                    })
                }),
                Interaction::Hover => vcx.simulate_mouse_move(
                    point(px(if odd { 300. } else { 700. }), px(330.)),
                    None,
                    Default::default(),
                ),
                Interaction::EditPreview => vcx.update(|_, cx| {
                    state.update(cx, |state, cx| {
                        if timed_view {
                            state.update_timed_drag(anchor, if odd { 630 } else { 660 }, cx);
                        } else {
                            state.update_day_drag(
                                anchor + chrono::Duration::days(if odd { 1 } else { 2 }),
                                cx,
                            );
                        }
                    })
                }),
            }
            // The update flushes effects and draws the dirty window. Prove it, so
            // the timing is one whole frame and no forced second draw is needed.
            if !matches!(interaction, Interaction::Hover) {
                assert!(
                    paints.get() > before,
                    "{view:?} {interaction:?} sample {sample} timed no painted frame"
                );
            }
            vcx.run_until_parked();
            frames.push(start.elapsed());
        }
        if matches!(interaction, Interaction::EditPreview) {
            vcx.update(|_, cx| state.update(cx, |state, cx| state.cancel(cx)));
        }
        frames.sort();
        timings.push(Timing {
            interaction,
            median: frames.get(SAMPLES / 2).copied().unwrap_or_default(),
            p99: p99(&frames),
            worst: frames.last().copied().unwrap_or_default(),
        });
    }
    (mount, timings)
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
        Host {
            state,
            paints: Rc::default(),
        }
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

fn assert_quiet(when: &str) -> f32 {
    let cpus = std::thread::available_parallelism()
        .map(|value| value.get() as f32)
        .unwrap_or(1.0);
    let load = load_average().unwrap_or_default();
    assert!(
        load <= cpus,
        "quiet-window precondition failed {when}: load average {load} exceeds {cpus} CPUs"
    );
    load
}

#[gpui_kit::test]
#[ignore = "release-only 10k-event performance gate"]
fn month_and_week_meet_ten_thousand_event_budgets(cx: &mut TestAppContext) {
    let before = assert_quiet("before measuring");
    let mut failures = Vec::new();
    for (view, fixture, model, gated) in [
        (
            CalendarViewKind::Month,
            "spread",
            model as fn() -> CalendarModel,
            true,
        ),
        (
            CalendarViewKind::Month,
            "dense-month",
            dense_month_model,
            true,
        ),
        (CalendarViewKind::Week, "spread", model, true),
        (CalendarViewKind::Week, "dense-week", dense_week_model, true),
        // Targets, not part of the foreman gate: printed only.
        (CalendarViewKind::Day, "dense-week", dense_week_model, false),
        (CalendarViewKind::Year, "spread", model, false),
    ] {
        let (mount, timings) = measure(cx, view, model);
        eprintln!("calendar {view:?} ({fixture}): 10k mount={mount:?}, gated={gated}");
        if gated && mount > Duration::from_millis(200) {
            failures.push(format!(
                "{view:?} {fixture} mount exceeded 200 ms: {mount:?}"
            ));
        }
        for Timing {
            interaction,
            median,
            p99,
            worst,
        } in timings
        {
            eprintln!(
                "  {interaction:?}: median={median:?}, p99={p99:?}, worst={worst:?}, n={SAMPLES}"
            );
            if gated && p99 > Duration::from_millis(8) {
                failures.push(format!(
                    "{view:?} {fixture} {interaction:?} p99 {p99:?} > 8 ms"
                ));
            }
            if gated && worst > Duration::from_millis(16) {
                failures.push(format!(
                    "{view:?} {fixture} {interaction:?} frame {worst:?} > 16 ms"
                ));
            }
        }
    }
    let after = assert_quiet("after measuring");
    eprintln!("calendar probe load average: {before} before, {after} after");
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
