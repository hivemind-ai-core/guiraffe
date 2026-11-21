use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_calendar::{
    Calendar, CalendarItem, CalendarModel, CalendarSource, CalendarState, CalendarViewKind,
    CalendarZone, EventId, SourceId, init,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct CalendarExample {
    state: Entity<CalendarState>,
}

impl Render for CalendarExample {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(Calendar::new(&self.state))
    }
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or(NaiveDate::MIN)
}

fn instant(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, hour, 0, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn fixture() -> CalendarModel {
    let sources = [
        CalendarSource::new(SourceId(1), "Work", ColorName::Blue),
        CalendarSource::new(SourceId(2), "Personal", ColorName::Teal),
    ];
    let mut items = Vec::new();
    for index in 0..8u64 {
        let start = instant(8 + index as u32 % 8);
        let item = CalendarItem::timed(
            EventId(index + 1),
            SourceId(if index % 2 == 0 { 1 } else { 2 }),
            format!("Event {index}"),
            start,
            start + Duration::minutes(30 + (index % 3) as i64 * 30),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        items.push(Arc::new(item));
    }
    for index in 0..3u64 {
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
                    cx.new(|_| CalendarExample { state })
                })?;
            assert_snapshot(&format!("{name}/{scheme_name}"), &image, &config)?;
        }
    }
    Ok(())
}
