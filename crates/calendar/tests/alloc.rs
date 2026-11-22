//! Allocation gates (spec/27 §Performance, spec/23): the per-frame hot paths that
//! run on every repaint and hour scroll allocate nothing once warm, and a warm
//! repaint of an unchanged calendar allocates the same amount every time.
#![allow(unsafe_code)]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc, Weekday};
use gpui_kit::{AppContext, Context, Entity, IntoElement, Render, TestAppContext, Window};
use guiraffe_calendar::{
    BusinessHours, Calendar, CalendarItem, CalendarLocale, CalendarModel, CalendarSource,
    CalendarState, CalendarViewKind, CalendarZone, EventId, SourceId, TimeFormat, hour_label,
    month_cells, month_chip_plan, row_iso_week, visible_hour_range, week_start,
};
use guiraffe_core::ColorName;

struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}
// SAFETY: forwards to the system allocator; only counts calls on the current thread.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or(NaiveDate::MIN)
}

fn instant(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn model() -> CalendarModel {
    let items = (0..2_000u64).map(|index| {
        let start = instant(1 + (index % 30) as u32, 6 + (index % 14) as u32);
        Arc::new(
            CalendarItem::timed(
                EventId(index + 1),
                SourceId(1 + index % 2),
                "Fixture",
                start,
                start + Duration::minutes(30 + (index % 4) as i64 * 15),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        )
    });
    CalendarModel::new(
        items,
        [
            CalendarSource::new(SourceId(1), "Work", ColorName::Blue),
            CalendarSource::new(SourceId(2), "Personal", ColorName::Teal),
        ],
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn warm_lookup_and_scroll_paths_allocate_nothing() {
    let index = model().index(CalendarZone::Utc);
    let locale = CalendarLocale::EN_GB;
    let hours = BusinessHours::default();
    let cells = month_cells(date(), Weekday::Mon);
    let mut sink = 0usize;
    let run = |sink: &mut usize| {
        for cell in &cells {
            *sink += index.day(cell.date).len() + index.count(cell.date);
            *sink += row_iso_week(cell.date) as usize;
            *sink += locale.short_weekday(Weekday::Wed).len();
        }
        for scroll in (0..=840).step_by(15) {
            let (first, last) = visible_hour_range(scroll, 600, 1);
            for hour in (first / 60) as u32..=((last - 1) / 60) as u32 {
                *sink += hour_label(hour, TimeFormat::Hours12).len();
            }
            for weekday in [Weekday::Mon, Weekday::Sat] {
                *sink += hours
                    .non_working_minutes(weekday)
                    .map(|(s, e)| (e - s) as usize)
                    .sum::<usize>();
            }
        }
        *sink += week_start(date(), Weekday::Sun).map_or(0, |d| d.day0() as usize);
        let (shown, hidden) = month_chip_plan(9, 120.0, 24.0, 20.0, 0.0);
        *sink += shown + hidden;
    };
    run(&mut sink);
    let before = allocations();
    run(&mut sink);
    assert_eq!(
        allocations() - before,
        0,
        "warm lookups and hour-scroll arithmetic allocated"
    );
    assert!(sink > 0);
}

use chrono::Datelike as _;

struct Host {
    state: Entity<CalendarState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Calendar::new(&self.state)
    }
}

/// A warm repaint of an unchanged calendar costs the same every frame (no
/// growth, no deferred paint-sync work) and stays within its recorded budget.
#[gpui_kit::test]
fn warm_repaints_are_stable_and_bounded(cx: &mut TestAppContext) {
    cx.update(guiraffe_calendar::init);
    let (host, vcx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|cx| CalendarState::new(model(), date(), CalendarZone::Utc, cx));
        state.update(cx, |state, cx| state.set_now(instant(21, 11), cx));
        Host { state }
    });
    let state = vcx.update(|_, cx| host.read(cx).state.clone());
    let mut results = Vec::new();
    for view in [
        CalendarViewKind::Year,
        CalendarViewKind::Month,
        CalendarViewKind::Week,
        CalendarViewKind::Day,
    ] {
        vcx.update(|_, cx| state.update(cx, |state, cx| state.set_view(view, cx)));
        vcx.run_until_parked();
        let mut frame = || {
            vcx.update(|window, cx| {
                window.refresh();
                let before = allocations();
                let _ = window.draw(cx);
                allocations() - before
            })
        };
        for _ in 0..3 {
            frame();
        }
        let first = frame();
        let second = frame();
        let third = frame();
        eprintln!("calendar {view:?} warm repaint: {first}, {second}, {third} allocations");
        results.push((view, first, second, third));
    }
    for (view, first, second, third) in results {
        // GPUI's own frame bookkeeping (hash map growth in its per-frame
        // tables) can differ by a handful of allocations between frames; the
        // calendar's share must not grow from frame to frame.
        assert!(
            second.abs_diff(first) <= 4 && third.abs_diff(first) <= 4 && third <= first + 4,
            "{view:?} allocates more on every repaint: {first}, {second}, {third}"
        );
        // Recorded budgets for a forced full rebuild of the 2,000-event fixture
        // (debug, test-support instrumentation on), with ~10% headroom. An
        // unchanged calendar in a cached view is not rebuilt at all; see
        // `cached_calendar_is_not_rebuilt_by_unrelated_repaints`.
        let budget = match view {
            CalendarViewKind::Year => 15_100, // 13,705 measured
            CalendarViewKind::Month => 9_600, // 8,708 measured
            CalendarViewKind::Week => 7_700,  // 6,969 measured
            CalendarViewKind::Day => 3_100,   // 2,803 measured
        };
        assert!(
            first <= budget,
            "{view:?} warm repaint allocated {first}, over its budget of {budget}"
        );
    }
}

struct Counted {
    state: Entity<CalendarState>,
    renders: std::rc::Rc<Cell<usize>>,
}

impl Render for Counted {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        Calendar::new(&self.state)
    }
}

struct Sibling;

impl Render for Sibling {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::ParentElement as _;
        gpui_kit::div().child("sibling")
    }
}

struct Shell {
    calendar: Entity<Counted>,
    sibling: Entity<Sibling>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::{ParentElement as _, Styled as _};
        gpui_kit::div()
            .size_full()
            .child(
                gpui_kit::AnyView::from(self.calendar.clone()).cached(
                    gpui_kit::StyleRefinement::default()
                        .w_full()
                        .h(gpui_kit::px(600.)),
                ),
            )
            .child(
                gpui_kit::AnyView::from(self.sibling.clone()).cached(
                    gpui_kit::StyleRefinement::default()
                        .w_full()
                        .h(gpui_kit::px(20.)),
                ),
            )
    }
}

/// An unchanged calendar does no render work on unrelated repaints, and once
/// its paint-time scroll/timer sync has settled, idle frames stay idle.
#[gpui_kit::test]
fn cached_calendar_is_not_rebuilt_by_unrelated_repaints(cx: &mut TestAppContext) {
    cx.update(guiraffe_calendar::init);
    let renders = std::rc::Rc::new(Cell::new(0));
    let counter = renders.clone();
    let (shell, vcx) = cx.add_window_view(move |_, cx| {
        let state = cx.new(|cx| CalendarState::new(model(), date(), CalendarZone::Utc, cx));
        state.update(cx, |state, cx| {
            state.set_now(instant(21, 11), cx);
            state.set_view(CalendarViewKind::Week, cx);
        });
        Shell {
            calendar: cx.new(|_| Counted {
                state,
                renders: counter,
            }),
            sibling: cx.new(|_| Sibling),
        }
    });
    vcx.run_until_parked();
    let settled = renders.get();
    for _ in 0..10 {
        vcx.update(|_, cx| {
            let sibling = shell.read(cx).sibling.clone();
            sibling.update(cx, |_, cx| cx.notify());
        });
        vcx.run_until_parked();
    }
    assert_eq!(
        renders.get(),
        settled,
        "sibling repaints must reuse the cached calendar"
    );
    // The other half: reuse must not mean never updating.
    vcx.update(|_, cx| {
        let state = shell.read(cx).calendar.read(cx).state.clone();
        state.update(cx, |state, cx| state.navigate(1, cx));
    });
    vcx.run_until_parked();
    assert!(
        renders.get() > settled,
        "a cached calendar must repaint when its state changes"
    );
}
