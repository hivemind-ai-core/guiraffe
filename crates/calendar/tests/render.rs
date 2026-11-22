use std::{rc::Rc, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
    prelude::FluentBuilder, px, size,
};
use guiraffe_calendar::{
    Calendar, CalendarItem, CalendarLocale, CalendarModel, CalendarSource, CalendarState,
    CalendarViewKind, CalendarZone, DragMode, EventId, ItemContent, ItemRenderer, SourceId,
    TimeFormat, init,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct CalendarExample {
    state: Entity<CalendarState>,
    renderer: Option<ItemRenderer>,
}

impl Render for CalendarExample {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            Calendar::new(&self.state)
                .month_chip_renderer(self.renderer.clone())
                .event_block_renderer(self.renderer.clone()),
        )
    }
}

fn custom_renderer() -> ItemRenderer {
    Rc::new(|content: &ItemContent<'_>| {
        Some(
            div()
                .flex()
                .flex_col()
                .text_size(px(12.))
                .child(format!("★ {}", content.title))
                .when(!content.compact, |el| el.child(content.time.clone()))
                .into_any_element(),
        )
    })
}

fn dense_fixture() -> CalendarModel {
    let base = fixture();
    let mut items: Vec<_> = base.items().to_vec();
    for index in 0..12u64 {
        let start = instant_on(19, 13) + Duration::minutes(index as i64 * 5);
        items.push(Arc::new(
            CalendarItem::timed(
                EventId(500 + index),
                SourceId(1 + index % 2),
                format!("Overlap {index}"),
                start,
                start + Duration::minutes(90),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    CalendarModel::new(items, base.sources().to_vec()).unwrap_or_else(|error| panic!("{error}"))
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or(NaiveDate::MIN)
}

fn instant_on(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn instant(hour: u32) -> DateTime<Utc> {
    instant_on(21, hour)
}

fn fixture() -> CalendarModel {
    let sources = [
        CalendarSource::new(SourceId(1), "Work", ColorName::Blue),
        CalendarSource::new(SourceId(2), "Personal", ColorName::Teal),
    ];
    let mut items = Vec::new();
    let titles = [
        "Design review",
        "Focus block",
        "Standup",
        "Client call",
        "Planning",
        "Lunch",
        "Code review",
        "Release",
    ];
    for index in 0..8u64 {
        let start = instant(8 + index as u32 % 8);
        let item = CalendarItem::timed(
            EventId(index + 1),
            SourceId(if index % 2 == 0 { 1 } else { 2 }),
            titles.get(index as usize).copied().unwrap_or("Event"),
            start,
            start + Duration::minutes(30 + (index % 3) as i64 * 30),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        items.push(Arc::new(item));
    }
    for (index, title) in ["Conference", "Family dinner", "Museum"]
        .into_iter()
        .enumerate()
    {
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
    for (index, day) in [2, 5, 9, 14, 18, 27].into_iter().enumerate() {
        let start = instant_on(day, 11);
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
    CalendarModel::new(items, sources).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn every_view_in_both_schemes() -> anyhow::Result<()> {
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (view, name) in [
        (CalendarViewKind::Year, "year"),
        (CalendarViewKind::Month, "month"),
        (CalendarViewKind::Week, "week"),
        (CalendarViewKind::Day, "day"),
    ] {
        for (scheme, scheme_name) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
            let image =
                Snapshotter::new()?.render(size(px(1_200.), px(760.)), move |_window, cx| {
                    guiraffe_components::init(cx);
                    init(cx);
                    apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                    let state =
                        cx.new(|cx| CalendarState::new(fixture(), date(), CalendarZone::Utc, cx));
                    state.update(cx, |state, cx| {
                        state.set_now(instant(11), cx);
                        state.set_view(view, cx);
                    });
                    cx.new(|_| CalendarExample {
                        state,
                        renderer: None,
                    })
                })?;
            assert_snapshot(&format!("{name}/{scheme_name}"), &image, &config)?;
        }
    }
    Ok(())
}

type Setup = fn(&mut CalendarState, &mut gpui_kit::Context<CalendarState>);

/// Knob and state coverage beyond the defaults: overflow
/// open, ISO weeks, Monday start and 24-hour time; dense overlap with a drag
/// preview; custom content; keyboard cursor with a hidden source.
#[test]
fn states_and_knobs_in_both_schemes() -> anyhow::Result<()> {
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    let cases: [(&str, bool, bool, Setup); 4] = [
        ("month-overflow", false, false, |state, cx| {
            state.set_locale(CalendarLocale::EN_GB, cx);
            state.set_show_iso_weeks(true, cx);
            state.set_view(CalendarViewKind::Month, cx);
            state.set_overflow(Some(date()), cx);
        }),
        ("week-dense", true, false, |state, cx| {
            state.set_view(CalendarViewKind::Week, cx);
            state.set_time_format(TimeFormat::Hours24, cx);
            state.set_scroll_minutes(720, cx);
            state.begin_item_drag(EventId(1), DragMode::Move, date(), 480, cx);
            state.update_timed_drag(date(), 480 + 6 * 60, cx);
        }),
        ("day-custom", false, true, |state, cx| {
            state.set_view(CalendarViewKind::Day, cx);
        }),
        ("week-focused", false, false, |state, cx| {
            state.set_view(CalendarViewKind::Week, cx);
            state.set_source_visible(SourceId(2), false, cx);
        }),
    ];
    for (name, dense, custom, setup) in cases {
        for (scheme, scheme_name) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
            let image =
                Snapshotter::new()?.render(size(px(1_200.), px(760.)), move |window, cx| {
                    guiraffe_components::init(cx);
                    init(cx);
                    apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                    let model = if dense { dense_fixture() } else { fixture() };
                    let state =
                        cx.new(|cx| CalendarState::new(model, date(), CalendarZone::Utc, cx));
                    state.update(cx, |state, cx| {
                        state.set_now(instant(11), cx);
                        setup(state, cx);
                    });
                    if name == "week-focused" {
                        let focus = state.read(cx).focused().clone();
                        window.focus(&focus, cx);
                    }
                    cx.new(|_| CalendarExample {
                        state,
                        renderer: custom.then(custom_renderer),
                    })
                })?;
            assert_snapshot(&format!("{name}/{scheme_name}"), &image, &config)?;
        }
    }
    Ok(())
}
