use std::{rc::Rc, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc, Weekday};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement,
    SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};
use guiraffe_calendar::{
    Calendar, CalendarItem, CalendarModel, CalendarSource, CalendarState, CalendarViewKind,
    CalendarZone, EventId, ItemContent, ItemRenderer, SourceId, TimeFormat,
};
use guiraffe_core::{ColorName, Density, StylePatch, provide};

use crate::catalog::Entry;
use crate::knobs::{Knob, KnobValues, playground_with_update_preview_first};

pub fn entries() -> Vec<Entry> {
    [
        (
            "Calendar year",
            "Twelve compact month grids with event-count tints and markers.",
            CalendarViewKind::Year,
            year as crate::catalog::DemoFn,
        ),
        (
            "Calendar month",
            "Six-week month with source-colored chips and +N overflow.",
            CalendarViewKind::Month,
            month,
        ),
        (
            "Calendar week",
            "Seven-day virtualized hours, all-day lane, overlap lanes, now-line, and editing.",
            CalendarViewKind::Week,
            week,
        ),
        (
            "Calendar day",
            "Single-day full-width schedule using the shared hour grid and editing model.",
            CalendarViewKind::Day,
            day,
        ),
    ]
    .into_iter()
    .map(|(name, summary, _, demo)| Entry {
        krate: "calendar",
        name,
        summary,
        covers: &["Calendar"],
        demo,
    })
    .collect()
}

const KNOBS: &[Knob] = &[
    Knob::toggle("Editable").default(1),
    Knob::toggle("Personal source").default(1),
    Knob::toggle("Custom content"),
    Knob::new("Locale", &["en-US", "en-GB"]),
    Knob::new("Week start", &["locale", "Sunday", "Monday"]),
    Knob::new("Time", &["locale", "12h", "24h"]),
    Knob::toggle("ISO weeks"),
    Knob::toggle("Business hours").default(1),
    Knob::new("Initial scroll", &["06:00", "08:00", "12:00"]).default(1),
    Knob::new("Zone", &["UTC", "New York", "London"]),
    Knob::new("Density", &["compact", "comfortable", "spacious"]).default(1),
    Knob::toggle("Now line").default(1),
    Knob::new("Events", &["default", "empty", "dense overlap"]),
    Knob::new(
        "View date",
        &["Nov 21", "Nov 1 (DST fold)", "Mar 8 (DST gap)"],
    ),
];

const EDITABLE: usize = 0;
const PERSONAL: usize = 1;
const CUSTOM: usize = 2;
const LOCALE: usize = 3;
const WEEK_START: usize = 4;
const TIME: usize = 5;
const ISO: usize = 6;
const BUSINESS: usize = 7;
const SCROLL: usize = 8;
const ZONE: usize = 9;
const DENSITY: usize = 10;
const NOW: usize = 11;
const EVENTS: usize = 12;
const VIEW_DATE: usize = 13;

fn year(window: &mut Window, cx: &mut App) -> AnyView {
    demo(CalendarViewKind::Year, window, cx)
}
fn month(window: &mut Window, cx: &mut App) -> AnyView {
    demo(CalendarViewKind::Month, window, cx)
}
fn week(window: &mut Window, cx: &mut App) -> AnyView {
    demo(CalendarViewKind::Week, window, cx)
}
fn day(window: &mut Window, cx: &mut App) -> AnyView {
    demo(CalendarViewKind::Day, window, cx)
}

fn demo(view: CalendarViewKind, window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update_preview_first(
        "Interactive calendar",
        KNOBS,
        move |_, cx| {
            let state = cx.new(|cx| CalendarState::new(fixture(), date(), CalendarZone::Utc, cx));
            state.update(cx, |state, cx| {
                state.set_now(instant(11, 30), cx);
                state.set_view(view, cx);
            });
            state
        },
        apply,
        render,
        window,
        cx,
    )
}

/// Knob changes only: applied once per change, never during render, so the
/// user's scroll position and navigation survive repaints.
fn apply(state: &Entity<CalendarState>, values: &KnobValues, cx: &mut App) {
    state.update(cx, |state, cx| {
        let events = values.index(EVENTS);
        let model = match events {
            1 => CalendarModel::new([], sources()),
            2 => dense_fixture(),
            _ => Ok(fixture()),
        };
        if let Ok(model) = model
            && (state.model().items().len() != model.items().len())
        {
            state.set_model(model, cx);
        }
        state.set_editable(values.on(EDITABLE), cx);
        state.set_source_visible(SourceId(2), values.on(PERSONAL), cx);
        state.set_locale(
            if values.index(LOCALE) == 1 {
                guiraffe_calendar::CalendarLocale::EN_GB
            } else {
                guiraffe_calendar::CalendarLocale::EN_US
            },
            cx,
        );
        state.set_first_weekday(
            match values.index(WEEK_START) {
                1 => Some(Weekday::Sun),
                2 => Some(Weekday::Mon),
                _ => None,
            },
            cx,
        );
        state.set_time_format(
            match values.index(TIME) {
                1 => TimeFormat::Hours12,
                2 => TimeFormat::Hours24,
                _ => TimeFormat::Locale,
            },
            cx,
        );
        state.set_show_iso_weeks(values.on(ISO), cx);
        state.set_show_business_hours(values.on(BUSINESS), cx);
        state.set_scroll_minutes(
            [360, 480, 720]
                .get(values.index(SCROLL))
                .copied()
                .unwrap_or(480),
            cx,
        );
        state.set_zone(
            match values.index(ZONE) {
                1 => CalendarZone::Iana(chrono_tz::America::New_York),
                2 => CalendarZone::Iana(chrono_tz::Europe::London),
                _ => CalendarZone::Utc,
            },
            cx,
        );
        state.set_show_now(values.on(NOW), cx);
        let view_date = match values.index(VIEW_DATE) {
            1 => NaiveDate::from_ymd_opt(2026, 11, 1),
            2 => NaiveDate::from_ymd_opt(2026, 3, 8),
            _ => Some(date()),
        };
        if let Some(view_date) = view_date
            && state.anchor() != view_date
        {
            let view = state.view();
            state.select_date(view_date, cx);
            state.set_view(view, cx);
        }
    });
}

fn render(
    state: &Entity<CalendarState>,
    values: &KnobValues,
    _: &mut Window,
    cx: &mut Context<crate::knobs::Preview<Entity<CalendarState>>>,
) -> AnyElement {
    let custom = values.on(CUSTOM).then(custom_renderer);
    let calendar = Calendar::new(state)
        .month_chip_renderer(custom.clone())
        .event_block_renderer(custom);
    let density = match values.index(DENSITY) {
        0 => Density::Compact,
        2 => Density::Spacious,
        _ => Density::Comfortable,
    };
    let height = match state.read(cx).view() {
        CalendarViewKind::Year => 680.,
        CalendarViewKind::Month => 840.,
        CalendarViewKind::Week | CalendarViewKind::Day => 560.,
    };
    div()
        .h(px(height))
        .min_h_0()
        .child(provide(StylePatch::new().density(density), calendar))
        .into_any_element()
}

fn custom_renderer() -> ItemRenderer {
    Rc::new(|content: &ItemContent<'_>| {
        let time = content.time.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .overflow_hidden()
                .whitespace_nowrap()
                .child("◆")
                .child(SharedString::from(content.title.to_string()))
                .when(!content.compact, |row| row.child(time))
                .into_any_element(),
        )
    })
}

fn sources() -> [CalendarSource; 2] {
    [
        CalendarSource::new(SourceId(1), "Work", ColorName::Blue),
        CalendarSource::new(SourceId(2), "Personal", ColorName::Teal),
    ]
}

fn dense_fixture() -> Result<CalendarModel, guiraffe_calendar::CalendarError> {
    let mut items = fixture().items().to_vec();
    for index in 0..24u64 {
        let start = instant(13, 0) + Duration::minutes(index as i64 * 5);
        items.push(Arc::new(CalendarItem::timed(
            EventId(1_000 + index),
            SourceId(1 + index % 2),
            format!("Overlap {index}"),
            start,
            start + Duration::minutes(90),
        )?));
    }
    CalendarModel::new(items, sources())
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or(NaiveDate::MIN)
}

fn instant_at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, minute, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn instant(hour: u32, minute: u32) -> DateTime<Utc> {
    instant_at(21, hour, minute)
}

fn fixture() -> CalendarModel {
    let sources = sources();
    let mut items = Vec::new();
    let titles = [
        "Design review",
        "Focus block",
        "Team standup",
        "Client call",
        "Project planning",
        "Lunch with Ada",
        "Code review",
        "Release checklist",
        "Research time",
        "Weekly retro",
    ];
    for index in 0..10u64 {
        let start = instant(8 + index as u32 % 9, if index % 2 == 0 { 0 } else { 30 });
        let item = CalendarItem::timed(
            EventId(index + 1),
            SourceId(if index % 2 == 0 { 1 } else { 2 }),
            titles.get(index as usize).copied().unwrap_or("Event"),
            start,
            start + Duration::minutes(45),
        )
        .unwrap_or_else(|error| panic!("{error}"))
        .locked(index == 7);
        items.push(Arc::new(item));
    }
    for (index, title) in ["Conference", "Family dinner"].into_iter().enumerate() {
        items.push(Arc::new(
            CalendarItem::all_day(
                EventId(100 + index as u64),
                SourceId(2),
                title,
                date(),
                date().succ_opt().unwrap_or(NaiveDate::MIN),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    let month_days = [2, 4, 6, 9, 12, 15, 18, 24, 26, 28];
    for (index, day) in month_days.into_iter().enumerate() {
        let start = instant_at(day, 10 + index as u32 % 5, 0);
        items.push(Arc::new(
            CalendarItem::timed(
                EventId(200 + index as u64),
                SourceId(if index % 2 == 0 { 1 } else { 2 }),
                titles.get(index).copied().unwrap_or("Event"),
                start,
                start + Duration::hours(1),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    for month in 1..=12u64 {
        let date = NaiveDate::from_ymd_opt(2026, month as u32, 15).unwrap_or_else(date);
        items.push(Arc::new(
            CalendarItem::all_day(
                EventId(300 + month),
                SourceId(1),
                "Monthly review",
                date,
                date.succ_opt().unwrap_or(NaiveDate::MIN),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    // DST fixtures (New York): the repeated 01:00-02:00 hour on Nov 1 and the
    // skipped 02:00 hour on Mar 8 2026.
    for (id, title, start, minutes) in [
        (
            400,
            "First 1:30 (EDT)",
            Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0),
            30,
        ),
        (
            401,
            "Second 1:30 (EST)",
            Utc.with_ymd_and_hms(2026, 11, 1, 6, 30, 0),
            30,
        ),
        (
            402,
            "Across the gap",
            Utc.with_ymd_and_hms(2026, 3, 8, 6, 30, 0),
            60,
        ),
    ] {
        let start = start.single().unwrap_or(DateTime::UNIX_EPOCH);
        items.push(Arc::new(
            CalendarItem::timed(
                EventId(id),
                SourceId(1),
                title,
                start,
                start + Duration::minutes(minutes),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    CalendarModel::new(items, sources).unwrap_or_else(|error| panic!("{error}"))
}
