use std::{rc::Rc, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc, Weekday};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement,
    SharedString, Styled, Window, div, px,
};
use guiraffe_calendar::{
    Calendar, CalendarItem, CalendarModel, CalendarSource, CalendarState, CalendarViewKind,
    CalendarZone, EventId, ItemRenderer, SourceId, TimeFormat,
};
use guiraffe_core::{ColorName, Density, StylePatch, provide};

use crate::catalog::Entry;
use crate::knobs::{Knob, KnobValues, playground_with};

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
];

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
    playground_with(
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
        render,
        window,
        cx,
    )
}

fn render(
    state: &Entity<CalendarState>,
    values: &KnobValues,
    _: &mut Window,
    cx: &mut Context<crate::knobs::Preview<Entity<CalendarState>>>,
) -> AnyElement {
    state.update(cx, |state, cx| {
        state.set_editable(values.on(0), cx);
        state.set_source_visible(SourceId(2), values.on(1), cx);
        state.set_locale(
            if values.index(3) == 1 {
                guiraffe_calendar::CalendarLocale::EN_GB
            } else {
                guiraffe_calendar::CalendarLocale::EN_US
            },
            cx,
        );
        state.set_first_weekday(
            match values.index(4) {
                1 => Some(Weekday::Sun),
                2 => Some(Weekday::Mon),
                _ => None,
            },
            cx,
        );
        state.set_time_format(
            match values.index(5) {
                1 => TimeFormat::Hours12,
                2 => TimeFormat::Hours24,
                _ => TimeFormat::Locale,
            },
            cx,
        );
        state.set_show_iso_weeks(values.on(6), cx);
        state.set_show_business_hours(values.on(7), cx);
        state.set_scroll_minutes(
            [360, 480, 720].get(values.index(8)).copied().unwrap_or(480),
            cx,
        );
        state.set_zone(
            match values.index(9) {
                1 => CalendarZone::Iana(chrono_tz::America::New_York),
                2 => CalendarZone::Iana(chrono_tz::Europe::London),
                _ => CalendarZone::Utc,
            },
            cx,
        );
        state.set_show_now(values.on(11), cx);
    });
    let custom = values.on(2).then(custom_renderer);
    let calendar = Calendar::new(state)
        .month_chip_renderer(custom.clone())
        .event_block_renderer(custom);
    let density = match values.index(10) {
        0 => Density::Compact,
        2 => Density::Spacious,
        _ => Density::Comfortable,
    };
    div()
        .h(px(600.))
        .min_h_0()
        .child(provide(StylePatch::new().density(density), calendar))
        .into_any_element()
}

fn custom_renderer() -> ItemRenderer {
    Rc::new(|_, title: SharedString, time: SharedString| {
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child("◆")
                .child(time)
                .child(title)
                .into_any_element(),
        )
    })
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or(NaiveDate::MIN)
}

fn instant(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, hour, minute, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn fixture() -> CalendarModel {
    let sources = [
        CalendarSource::new(SourceId(1), "Work", ColorName::Blue),
        CalendarSource::new(SourceId(2), "Personal", ColorName::Teal),
    ];
    let mut items = Vec::new();
    for index in 0..10u64 {
        let start = instant(8 + index as u32 % 9, if index % 2 == 0 { 0 } else { 30 });
        let item = CalendarItem::timed(
            EventId(index + 1),
            SourceId(if index % 2 == 0 { 1 } else { 2 }),
            format!("Event {index}"),
            start,
            start + Duration::minutes(45),
        )
        .unwrap_or_else(|error| panic!("{error}"))
        .locked(index == 7);
        items.push(Arc::new(item));
    }
    for index in 0..4u64 {
        items.push(Arc::new(
            CalendarItem::all_day(
                EventId(100 + index),
                SourceId(2),
                format!("All-day {index}"),
                date(),
                date().succ_opt().unwrap_or(NaiveDate::MIN),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    CalendarModel::new(items, sources).unwrap_or_else(|error| panic!("{error}"))
}
