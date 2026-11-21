//! Native date and time controls for screen documents.

use chrono::{Datelike as _, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use gpui_kit::{
    App, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
    component::{
        Disableable, FocusableExt, Sizable,
        calendar::Matcher,
        date_picker::{DatePicker, DatePickerEvent, DatePickerState, DateRangePreset, DateTime},
        time_field::{HourCycle, TimeField, TimeFieldEvent, TimeFieldState, TimePrecision},
    },
    px,
};
use guiraffe_core::{
    Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::widget_specs;

pub(super) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[5],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[5]),
                render: |node, _, _, _| DateDocument::new(node).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[6],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[6]),
                render: |node, _, _, _| TimeDocument::new(node).into_any_element(),
            },
        ],
    );
}

fn precision(value: &Value) -> Option<TimePrecision> {
    match value.as_str() {
        Some("minute") => Some(TimePrecision::Minute),
        Some("second") => Some(TimePrecision::Second),
        _ => None,
    }
}

fn hour_cycle(value: &Value) -> HourCycle {
    if value.as_str() == Some("h12") {
        HourCycle::H12
    } else {
        HourCycle::H23
    }
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .ok()
        .or_else(|| NaiveTime::parse_from_str(value, "%H:%M").ok())
}

fn parse_date(value: &Value) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.as_str()?, "%Y-%m-%d").ok()
}

fn parse_value(value: &Value, range: bool, default_time: NaiveTime) -> DateTime {
    let parse = |value: &Value| {
        let text = value.as_str()?;
        NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
            .ok()
            .or_else(|| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M").ok())
            .or_else(|| parse_date(value).map(|date| date.and_time(default_time)))
    };
    if range {
        DateTime::Range(
            value.list_get(0).and_then(parse),
            value.list_get(1).and_then(parse),
        )
    } else {
        DateTime::Single(parse(value))
    }
}

fn serialize_date(value: DateTime, precision: Option<TimePrecision>) -> Value {
    let format = |date: Option<NaiveDateTime>| {
        let format = match precision {
            Some(TimePrecision::Second) => "%Y-%m-%dT%H:%M:%S",
            Some(TimePrecision::Minute) => "%Y-%m-%dT%H:%M",
            None => "%Y-%m-%d",
        };
        date.map_or(Value::Null, |date| date.format(format).to_string().into())
    };
    match value {
        DateTime::Single(value) => format(value),
        DateTime::Range(start, end) => Value::List(vec![format(start), format(end)].into()),
    }
}

#[derive(Clone, PartialEq)]
struct DateConfig {
    range: bool,
    precision: Option<TimePrecision>,
    hour_cycle: HourCycle,
    date_format: Option<SharedString>,
    default_time: NaiveTime,
    months: usize,
    weekday: Weekday,
    disabled_weekdays: Vec<u32>,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
    years: Option<(i32, i32)>,
}

impl DateConfig {
    fn new(node: &dyn WidgetNode) -> Self {
        let range = node.prop(1).as_bool().unwrap_or_default();
        let years = node.prop(17);
        let year = |i| {
            years
                .list_get(i)
                .and_then(Value::as_f64)
                .filter(|v| *v >= 1.0 && *v <= 10000.0 && v.fract() == 0.0)
                .map(|v| v as i32)
        };
        let days = node.prop(14);
        Self {
            range,
            precision: if range {
                None
            } else {
                precision(node.prop(10))
            },
            hour_cycle: hour_cycle(node.prop(11)),
            date_format: node
                .prop(9)
                .as_str()
                .filter(|format| {
                    !chrono::format::StrftimeItems::new(format)
                        .any(|item| item == chrono::format::Item::Error)
                })
                .map(|format| format.to_owned().into()),
            default_time: node
                .prop(12)
                .as_str()
                .and_then(parse_time)
                .unwrap_or(NaiveTime::MIN),
            months: node.prop(8).as_f64().unwrap_or(1.0).clamp(1.0, 12.0) as usize,
            weekday: match node.prop(13).as_str() {
                Some("mon") => Weekday::Mon,
                Some("tue") => Weekday::Tue,
                Some("wed") => Weekday::Wed,
                Some("thu") => Weekday::Thu,
                Some("fri") => Weekday::Fri,
                Some("sat") => Weekday::Sat,
                _ => Weekday::Sun,
            },
            disabled_weekdays: (0..days.list_len().unwrap_or_default())
                .filter_map(|i| days.list_get(i)?.as_f64())
                .filter(|v| (0.0..=6.0).contains(v) && v.fract() == 0.0)
                .map(|v| v as u32)
                .collect(),
            min_date: parse_date(node.prop(15)),
            max_date: parse_date(node.prop(16)),
            years: year(0).zip(year(1)).filter(|(start, end)| start < end),
        }
    }
}

#[derive(IntoElement)]
struct DateDocument {
    key: ElementId,
    config: DateConfig,
    value: Option<DateTime>,
    placeholder: SharedString,
    cleanable: bool,
    appearance: bool,
    disabled: bool,
    focus_ring: bool,
    size: Option<Size>,
    presets: Value,
    change: Option<EventHandler>,
}

impl DateDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let config = DateConfig::new(node);
        let value = node
            .is_set(0)
            .then(|| parse_value(node.prop(0), config.range, config.default_time));
        Self {
            key: node.state_id(),
            config,
            value,
            placeholder: node.prop(2).display(),
            cleanable: node.prop(3).as_bool().unwrap_or(false),
            appearance: node.prop(4).as_bool().unwrap_or(true),
            disabled: node.prop(5).as_bool().unwrap_or(false),
            size: node.size(6),
            focus_ring: node.prop(7).as_bool().unwrap_or(true),
            presets: node.prop(18).clone(),
            change: node.handler(0),
        }
    }
}

struct DateState {
    picker: Entity<DatePickerState>,
    config: DateConfig,
    change: Option<EventHandler>,
    subscription: Subscription,
}

fn build_date(
    config: &DateConfig,
    window: &mut Window,
    cx: &mut gpui_kit::Context<DateState>,
) -> (Entity<DatePickerState>, Subscription) {
    use gpui_kit::AppContext as _;
    let picker = cx.new(|cx| {
        let state = if config.range {
            DatePickerState::range(window, cx)
        } else {
            DatePickerState::new(window, cx)
        };
        let mut state = state
            .hour_cycle(config.hour_cycle)
            .default_time(config.default_time)
            .number_of_months(config.months)
            .first_day_of_week(config.weekday);
        if let Some(format) = &config.date_format {
            state = state.date_format(format.clone());
        }
        if let Some(precision) = config.precision {
            state = state.time_precision(precision);
        }
        if let Some(years) = config.years {
            state.set_year_range(years, cx);
        }
        if config.min_date.is_some()
            || config.max_date.is_some()
            || !config.disabled_weekdays.is_empty()
        {
            let min = config.min_date;
            let max = config.max_date;
            let weekdays = config.disabled_weekdays.clone();
            state = state.disabled_matcher(Matcher::custom(move |date| {
                min.is_some_and(|min| *date < min)
                    || max.is_some_and(|max| *date > max)
                    || weekdays.contains(&date.weekday().num_days_from_sunday())
            }));
        }
        state
    });
    let subscription = cx.subscribe_in(
        &picker,
        window,
        |state: &mut DateState, _, event, window, cx| {
            let DatePickerEvent::Change(value) = event;
            if let Some(handler) = &state.change {
                handler(serialize_date(*value, state.config.precision), window, cx);
            }
        },
    );
    (picker, subscription)
}

impl RenderOnce for DateDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let (picker, subscription) = build_date(&self.config, window, cx);
            DateState {
                picker,
                config: self.config.clone(),
                change: None,
                subscription,
            }
        });
        let picker = state.update(cx, |state, cx| {
            let previous = (state.config != self.config).then(|| state.picker.read(cx).date_time());
            if previous.is_some() {
                state.config = self.config.clone();
                (state.picker, state.subscription) = build_date(&state.config, window, cx);
            }
            state.change = self.change;
            if let Some(mut value) = self.value.or(previous) {
                value = if state.config.range {
                    DateTime::Range(value.start(), value.end())
                } else {
                    DateTime::Single(value.start())
                };
                if state.picker.read(cx).date_time() != value {
                    state
                        .picker
                        .update(cx, |picker, cx| picker.set_date_time(value, window, cx));
                }
            }
            state.picker.clone()
        });
        let label = ["label".into()];
        let value = ["value".into()];
        let presets = (0..self.presets.list_len().unwrap_or_default())
            .filter_map(|i| self.presets.list_get(i))
            .filter_map(|preset| {
                Some(DateRangePreset::date_time(
                    preset.field(&label).as_str()?.to_owned(),
                    parse_value(
                        preset.field(&value),
                        self.config.range,
                        self.config.default_time,
                    ),
                ))
            })
            .collect();
        let mut picker = DatePicker::new(&picker)
            .placeholder(self.placeholder)
            .cleanable(self.cleanable)
            .appearance(self.appearance)
            .disabled(self.disabled)
            .focus_ring(self.focus_ring)
            .number_of_months(self.config.months)
            .presets(presets);
        if let Some(size) = self.size {
            picker = picker.with_size(px(size.control_height()));
        }
        picker
    }
}

#[derive(IntoElement)]
struct TimeDocument {
    key: ElementId,
    value: Option<NaiveTime>,
    precision: TimePrecision,
    hour_cycle: HourCycle,
    disabled: bool,
    invalid: bool,
    size: Option<Size>,
    change: Option<EventHandler>,
}

impl TimeDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            value: node.prop(0).as_str().and_then(parse_time),
            precision: precision(node.prop(1)).unwrap_or_default(),
            hour_cycle: hour_cycle(node.prop(2)),
            disabled: node.prop(3).as_bool().unwrap_or(false),
            invalid: node.prop(4).as_bool().unwrap_or(false),
            size: node.size(5),
            change: node.handler(0),
        }
    }
}

struct TimeState {
    editor: Entity<TimeFieldState>,
    precision: TimePrecision,
    change: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for TimeDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui_kit::AppContext as _;
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let editor = cx.new(|cx| {
                TimeFieldState::new(window, cx)
                    .precision(self.precision)
                    .hour_cycle(self.hour_cycle)
            });
            let subscription = cx.subscribe_in(
                &editor,
                window,
                |state: &mut TimeState, _, event, window, cx| {
                    let TimeFieldEvent::Change(value) = event;
                    if let Some(handler) = &state.change {
                        let format = state.precision.format(HourCycle::H23);
                        handler(value.format(format).to_string().into(), window, cx);
                    }
                },
            );
            TimeState {
                editor,
                precision: self.precision,
                change: None,
                _subscription: subscription,
            }
        });
        let editor = state.update(cx, |state, cx| {
            state.precision = self.precision;
            state.change = self.change;
            state.editor.update(cx, |editor, cx| {
                editor.set_precision(self.precision, window, cx);
                editor.set_hour_cycle(self.hour_cycle, window, cx);
                if let Some(time) = self.value {
                    editor.set_time(time, window, cx);
                }
            });
            state.editor.clone()
        });
        let mut field = TimeField::new(&editor)
            .disabled(self.disabled)
            .invalid(self.invalid);
        if let Some(size) = self.size {
            field = field.with_size(px(size.control_height()));
        }
        field
    }
}
