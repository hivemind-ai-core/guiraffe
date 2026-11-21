//! guiraffe-inputs date & time pickers: one entry per picker, each with knobs for its main
//! props and an event log. The demo clock is pinned to Wednesday 11 March 2026 so the
//! calendar and the presets look the same on every run (and in the goldens).

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use gpui_kit::{
    AnyElement, AnyView, App, Context, Entity, IntoElement, ParentElement, Styled, Window, div, px,
};
use guiraffe_core::{ColorName, Size};
use guiraffe_inputs::dates::{
    DateLocale, DatePredicate, DatePreset, DateRange, DateTimeRange, HourFormat, InputVariant,
    Picker, PickerEvent, PickerProps, PickerValue,
};

use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{Applied, Knob, KnobValues, Preview, playground_with};

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "inputs",
            name: "DatePicker",
            summary: "One date from a calendar popover: min/max, excluded days, locale and week start, clearable, label/description/error, size/variant/radius from the theme; full keyboard operation.",
            covers: &["Picker", "InputWrapper"],
            demo: date,
        },
        Entry {
            krate: "inputs",
            name: "DateRangePicker",
            summary: "A date range with configurable presets (Today, Last 7 days, This month, Custom…), one or two months, hover preview.",
            covers: &["Picker"],
            demo: range,
        },
        Entry {
            krate: "inputs",
            name: "TimePicker",
            summary: "A segmented time field: 12 or 24 hours, optional seconds, min/max time, clearable; type digits or step with arrows.",
            covers: &["Picker"],
            demo: time,
        },
        Entry {
            krate: "inputs",
            name: "DateTimePicker",
            summary: "A date and a time: the popover stays open after the day is picked so the time can be set.",
            covers: &["Picker"],
            demo: date_time,
        },
        Entry {
            krate: "inputs",
            name: "DateTimeRangePicker",
            summary: "A range with a start and an end time.",
            covers: &["Picker"],
            demo: date_time_range,
        },
    ]
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
}
fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default()
}
/// The demo clock (see the module docs).
fn today() -> NaiveDate {
    d(2026, 3, 11)
}

const LOCALES: &[&str] = &["en-US", "en-GB", "de", "fr"];
fn locale(i: usize) -> DateLocale {
    match i {
        1 => DateLocale::en_gb(),
        2 => DateLocale::de(),
        3 => DateLocale::fr(),
        _ => DateLocale::en_us(),
    }
}
const VARIANTS: &[&str] = &["default", "filled", "unstyled"];
fn variant(i: usize) -> InputVariant {
    match i {
        1 => InputVariant::Filled,
        2 => InputVariant::Unstyled,
        _ => InputVariant::Default,
    }
}
fn weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// A picker beside its event log.
fn with_log(widget: impl IntoElement, log: &Entity<EventLog>, width: f32) -> AnyElement {
    div()
        .flex()
        .gap(px(Size::Xl.spacing()))
        .child(div().w(px(width)).child(widget))
        .child(div().flex_1().child(log.clone()))
        .into_any_element()
}

/// The picker, its log and the knob values last pushed into it.
type State<V> = (Entity<Picker<V>>, Entity<EventLog>, Applied);

fn state<V: PickerValue>(
    window: &mut Window,
    cx: &mut App,
    configure: impl FnOnce(PickerProps<V>) -> PickerProps<V>,
    fmt: fn(&V) -> String,
) -> State<V> {
    let picker = Picker::<V>::new(window, cx, |p| configure(p.today(today())));
    let log = EventLog::new(cx);
    EventLog::watch_with(
        &log,
        &picker,
        move |e: &PickerEvent<V>| match e {
            PickerEvent::Change(Some(v)) => format!("Change({})", fmt(v)).into(),
            PickerEvent::Change(None) => "Change(cleared)".into(),
            PickerEvent::OpenChange(open) => format!("OpenChange({open})").into(),
        },
        cx,
    );
    (picker, log, Applied::default())
}

/// Push the knob values into the picker when they changed (not on every render).
/// Returns whether they did.
fn apply<V: PickerValue>(
    (picker, _, applied): &State<V>,
    v: &KnobValues,
    window: &mut Window,
    cx: &mut Context<Preview<State<V>>>,
    f: impl FnOnce(&mut PickerProps<V>, &KnobValues),
) -> bool {
    let changed = applied.changed(v);
    if changed {
        picker.update(cx, |p, cx| p.update_props(|props| f(props, v), window, cx));
    }
    changed
}

fn date(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live DatePicker (demo clock: Wed 11 Mar 2026)",
        &[
            Knob::size(),
            Knob::new("Variant", VARIANTS),
            Knob::radius(),
            Knob::new("Locale", LOCALES),
            Knob::toggle("Clearable").default(1),
            Knob::toggle("This month only"),
            Knob::toggle("No weekends"),
            Knob::toggle("Error"),
            Knob::toggle("Disabled"),
            Knob::toggle("Read-only"),
        ],
        |window, cx| {
            state::<NaiveDate>(
                window,
                cx,
                |p| {
                    p.label("Due date")
                        .description("When the invoice must be paid")
                        .required(true)
                        .default_value(d(2026, 3, 16))
                },
                |v| v.to_string(),
            )
        },
        |s, v, window, cx| {
            apply(s, v, window, cx, |p, v| {
                p.size = Some(v.size(0));
                p.variant = variant(v.index(1));
                p.radius = Some(v.size(2));
                p.locale = locale(v.index(3));
                p.clearable = v.on(4);
                p.min_date = v.on(5).then(|| d(2026, 3, 1));
                p.max_date = v.on(5).then(|| d(2026, 3, 31));
                p.exclude_date = v
                    .on(6)
                    .then(|| std::sync::Arc::new(weekend) as DatePredicate);
                p.error = v.on(7).then(|| "Pick a weekday in March".into());
                p.disabled = v.on(8);
                p.read_only = v.on(9);
            });
            with_log(s.0.clone(), &s.1, 320.)
        },
        window,
        cx,
    )
}

fn range(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live DateRangePicker (open; demo clock: Wed 11 Mar 2026)",
        &[
            Knob::new("Months", &["1", "2"]),
            Knob::toggle("Presets").default(1),
            Knob::toggle("Open").default(1),
            Knob::toggle("Disabled"),
        ],
        |window, cx| {
            state::<DateRange>(
                window,
                cx,
                |p| {
                    p.label("Reporting period")
                        .clearable(true)
                        .min_date(d(2025, 1, 1))
                        .default_value(DateRange::new(d(2026, 3, 5), d(2026, 3, 11)))
                },
                |r| format!("{} → {}", r.start, r.end),
            )
        },
        |s, v, window, cx| {
            let changed = apply(s, v, window, cx, |p, v| {
                p.number_of_months = v.index(0) + 1;
                p.presets = if v.on(1) {
                    DatePreset::standard_ranges(&p.locale)
                } else {
                    Vec::new()
                };
                p.disabled = v.on(3);
            });
            // Open is applied with the other knobs; the picker still closes itself
            // (outside click, Escape, a pick) and reopens on click.
            if changed {
                let open = v.on(2);
                s.0.update(cx, |p, cx| p.set_open(open, window, cx));
            }
            with_log(s.0.clone(), &s.1, 300.)
        },
        window,
        cx,
    )
}

fn time(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live TimePicker",
        &[
            Knob::size(),
            Knob::new("Clock", &["12 h", "24 h"]),
            Knob::toggle("Seconds"),
            Knob::toggle("Office hours (9–17)"),
            Knob::toggle("Clearable").default(1),
            Knob::toggle("Disabled"),
        ],
        |window, cx| {
            state::<NaiveTime>(
                window,
                cx,
                |p| {
                    p.label("Reminder")
                        .description("Type digits, or step with ↑/↓")
                        .default_value(t(9, 30))
                },
                |v| v.to_string(),
            )
        },
        |s, v, window, cx| {
            apply(s, v, window, cx, |p, v| {
                p.size = Some(v.size(0));
                p.hour_format = Some(if v.index(1) == 0 {
                    HourFormat::H12
                } else {
                    HourFormat::H24
                });
                p.with_seconds = v.on(2);
                p.min_time = v.on(3).then(|| t(9, 0));
                p.max_time = v.on(3).then(|| t(17, 0));
                p.clearable = v.on(4);
                p.disabled = v.on(5);
            });
            with_log(s.0.clone(), &s.1, 260.)
        },
        window,
        cx,
    )
}

fn date_time(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live DateTimePicker (demo clock: Wed 11 Mar 2026)",
        &[
            Knob::new("Clock", &["locale", "12 h", "24 h"]),
            Knob::new("Locale", LOCALES).default(1),
            Knob::toggle("Seconds"),
            Knob::color(),
            Knob::toggle("Disabled"),
        ],
        |window, cx| {
            state::<NaiveDateTime>(
                window,
                cx,
                |p| {
                    p.label("Departure")
                        .clearable(true)
                        .default_time(t(9, 0))
                        .default_value(d(2026, 3, 18).and_time(t(9, 45)))
                },
                |v| v.to_string(),
            )
        },
        |s, v, window, cx| {
            apply(s, v, window, cx, |p, v| {
                p.hour_format = match v.index(0) {
                    1 => Some(HourFormat::H12),
                    2 => Some(HourFormat::H24),
                    _ => None,
                };
                p.locale = locale(v.index(1));
                p.with_seconds = v.on(2);
                p.color = v.color(3);
                p.disabled = v.on(4);
            });
            with_log(s.0.clone(), &s.1, 320.)
        },
        window,
        cx,
    )
}

fn date_time_range(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live DateTimeRangePicker (demo clock: Wed 11 Mar 2026)",
        &[
            Knob::new("Clock", &["12 h", "24 h"]).default(1),
            Knob::new("Months", &["1", "2"]),
            Knob::toggle("Disabled"),
        ],
        |window, cx| {
            state::<DateTimeRange>(
                window,
                cx,
                |p| {
                    p.label("Maintenance window")
                        .clearable(true)
                        .color(ColorName::Teal)
                        .default_time(t(22, 0))
                        .default_value(DateTimeRange::new(
                            d(2026, 3, 13).and_time(t(22, 0)),
                            d(2026, 3, 15).and_time(t(6, 0)),
                        ))
                },
                |r| format!("{} → {}", r.start, r.end),
            )
        },
        |s, v, window, cx| {
            apply(s, v, window, cx, |p, v| {
                p.hour_format = Some(if v.index(0) == 0 {
                    HourFormat::H12
                } else {
                    HourFormat::H24
                });
                p.number_of_months = v.index(1) + 1;
                p.disabled = v.on(2);
            });
            with_log(s.0.clone(), &s.1, 380.)
        },
        window,
        cx,
    )
}
