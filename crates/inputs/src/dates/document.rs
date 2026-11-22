//! DataUi adapters for [`Picker`] (all five value types, chosen by `mode`) and
//! [`InputWrapper`].
//!
//! The picker stays uncontrolled inside: document `value` and `open` are applied when the
//! document changes them, and user edits are reported through `change` / `open-change`
//! (which also update a bound `value`).

use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, IntoElement, ParentElement, RenderOnce, SharedString,
    Subscription, Window, div,
};
use guiraffe_core::{
    Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{
    DateLocale, DatePreset, DateRange, DateTimeRange, HourFormat, InputVariant, InputWrapper,
    Picker, PickerEvent, PickerProps, PickerValue, PresetRange,
};
use crate::widget_specs;

pub(super) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: &widget_specs::PICKER,
                schema: guiraffe_core::widget_schema!(&widget_specs::PICKER),
                render: |node, _, _, _| match node.prop(0).as_str() {
                    Some("date-range") => PickerDocument::<DateRange>::new(node).into_any_element(),
                    Some("time") => PickerDocument::<NaiveTime>::new(node).into_any_element(),
                    Some("date-time") => {
                        PickerDocument::<NaiveDateTime>::new(node).into_any_element()
                    }
                    Some("date-time-range") => {
                        PickerDocument::<DateTimeRange>::new(node).into_any_element()
                    }
                    _ => PickerDocument::<NaiveDate>::new(node).into_any_element(),
                },
            },
            WidgetRegistration {
                spec: &widget_specs::INPUT_WRAPPER,
                schema: guiraffe_core::widget_schema!(&widget_specs::INPUT_WRAPPER),
                render: |node, children, _, _| wrapper(node, children),
            },
        ],
    );
}

fn wrapper(node: &dyn WidgetNode, children: Vec<AnyElement>) -> AnyElement {
    let text = |index| {
        node.prop(index)
            .as_str()
            .map(|text| SharedString::from(text.to_owned()))
    };
    let mut wrapper = InputWrapper::new(div().children(children))
        .error(text(2))
        .required(node.prop(3).as_bool().unwrap_or_default())
        .size(node.size(4).unwrap_or(Size::Sm));
    if let Some(label) = text(0) {
        wrapper = wrapper.label(label);
    }
    if let Some(description) = text(1) {
        wrapper = wrapper.description(description);
    }
    wrapper.into_any_element()
}

// ------------------------------------------------------------------ values

fn date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text, "%H:%M:%S")
        .ok()
        .or_else(|| NaiveTime::parse_from_str(text, "%H:%M").ok())
}

/// A local date-time; a bare date takes midnight.
fn date_time(text: &str) -> Option<NaiveDateTime> {
    match text.split_once('T') {
        Some((day, clock)) => Some(date(day)?.and_time(time(clock)?)),
        None => Some(date(text)?.and_time(NaiveTime::MIN)),
    }
}

fn time_text(value: NaiveTime, seconds: bool) -> String {
    value
        .format(if seconds { "%H:%M:%S" } else { "%H:%M" })
        .to_string()
}

fn pair(value: &Value) -> Option<(&str, &str)> {
    Some((value.list_get(0)?.as_str()?, value.list_get(1)?.as_str()?))
}

/// Document form of a picker value (`seconds`: include seconds in times).
trait DocumentValue: PickerValue {
    fn parse(value: &Value) -> Option<Self>;
    fn to_value(self, seconds: bool) -> Value;
}

impl DocumentValue for NaiveDate {
    fn parse(value: &Value) -> Option<Self> {
        date(value.as_str()?)
    }
    fn to_value(self, _: bool) -> Value {
        self.format("%Y-%m-%d").to_string().into()
    }
}

impl DocumentValue for NaiveTime {
    fn parse(value: &Value) -> Option<Self> {
        time(value.as_str()?)
    }
    fn to_value(self, seconds: bool) -> Value {
        time_text(self, seconds).into()
    }
}

impl DocumentValue for NaiveDateTime {
    fn parse(value: &Value) -> Option<Self> {
        date_time(value.as_str()?)
    }
    fn to_value(self, seconds: bool) -> Value {
        format!(
            "{}T{}",
            self.date().format("%Y-%m-%d"),
            time_text(self.time(), seconds)
        )
        .into()
    }
}

impl DocumentValue for DateRange {
    fn parse(value: &Value) -> Option<Self> {
        let (start, end) = pair(value)?;
        Some(DateRange::new(date(start)?, date(end)?))
    }
    fn to_value(self, seconds: bool) -> Value {
        Value::List(Arc::from([
            self.start.to_value(seconds),
            self.end.to_value(seconds),
        ]))
    }
}

impl DocumentValue for DateTimeRange {
    fn parse(value: &Value) -> Option<Self> {
        let (start, end) = pair(value)?;
        Some(DateTimeRange::new(date_time(start)?, date_time(end)?))
    }
    fn to_value(self, seconds: bool) -> Value {
        Value::List(Arc::from([
            self.start.to_value(seconds),
            self.end.to_value(seconds),
        ]))
    }
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

fn parse_presets(value: &Value) -> Vec<DatePreset> {
    items(value)
        .filter_map(|preset| {
            let label = field(preset, "label").as_str()?;
            let days = || {
                field(preset, "days")
                    .as_f64()
                    .filter(|days| (1.0..=36500.0).contains(days) && days.fract() == 0.0)
                    .map(|days| days as u32)
            };
            let bound = |name| field(preset, name).as_str().and_then(date);
            let range = match field(preset, "range").as_str()? {
                "today" => PresetRange::Today,
                "yesterday" => PresetRange::Yesterday,
                "tomorrow" => PresetRange::Tomorrow,
                "last-days" => PresetRange::LastDays(days()?),
                "next-days" => PresetRange::NextDays(days()?),
                "this-week" => PresetRange::ThisWeek,
                "last-week" => PresetRange::LastWeek,
                "this-month" => PresetRange::ThisMonth,
                "last-month" => PresetRange::LastMonth,
                "this-year" => PresetRange::ThisYear,
                "fixed" => PresetRange::Fixed(bound("start")?, bound("end")?),
                "custom" => PresetRange::Custom,
                _ => return None,
            };
            Some(DatePreset::new(label, range))
        })
        .collect()
}

fn weekday(value: &Value) -> Option<Weekday> {
    Some(match value.as_str()? {
        "sun" => Weekday::Sun,
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        _ => return None,
    })
}

fn locale(value: &Value) -> DateLocale {
    match value.as_str() {
        Some("en-GB") => DateLocale::en_gb(),
        Some("de") => DateLocale::de(),
        Some("fr") => DateLocale::fr(),
        _ => DateLocale::en_us(),
    }
}

// ------------------------------------------------------------------ picker

/// Every document prop except `mode`, `value`, `default-value` and `open`, as raw values so
/// changes are cheap to detect, plus the resolved theme tokens.
#[derive(Clone, PartialEq)]
struct Options {
    raw: Vec<Value>,
    size: Option<Size>,
    radius: Option<Size>,
    color: Option<guiraffe_core::ColorName>,
}

impl Options {
    const FIRST: usize = 4;

    fn from_node(node: &dyn WidgetNode) -> Self {
        Self {
            raw: (Self::FIRST..widget_specs::PICKER.props.len())
                .map(|index| node.prop(index).clone())
                .collect(),
            size: node.size(23),
            radius: node.size(25),
            color: node.color(26),
        }
    }

    fn get(&self, index: usize) -> &Value {
        self.raw.get(index - Self::FIRST).unwrap_or(&Value::Null)
    }

    fn text(&self, index: usize) -> Option<SharedString> {
        self.get(index)
            .as_str()
            .map(|text| SharedString::from(text.to_owned()))
    }

    fn flag(&self, index: usize) -> bool {
        self.get(index).as_bool().unwrap_or_default()
    }

    /// Write these options onto `props` (value, default value and open are left alone).
    fn apply<V: PickerValue>(&self, props: &mut PickerProps<V>) {
        let excluded: Vec<NaiveDate> = items(self.get(11))
            .filter_map(Value::as_str)
            .filter_map(date)
            .collect();
        props.placeholder = self.text(4);
        props.label = self.text(5);
        props.description = self.text(6);
        props.error = self.text(7);
        props.required = self.flag(8);
        props.min_date = self.get(9).as_str().and_then(date);
        props.max_date = self.get(10).as_str().and_then(date);
        props.exclude_date = (!excluded.is_empty())
            .then(|| Arc::new(move |day: NaiveDate| excluded.contains(&day)) as _);
        props.min_time = self.get(12).as_str().and_then(time);
        props.max_time = self.get(13).as_str().and_then(time);
        props.default_time = self
            .get(14)
            .as_str()
            .and_then(time)
            .unwrap_or(NaiveTime::MIN);
        props.first_day_of_week = weekday(self.get(15));
        props.locale = locale(self.get(16));
        props.hour_format = match self.get(17).as_str() {
            Some("h12") => Some(HourFormat::H12),
            Some("h24") => Some(HourFormat::H24),
            _ => None,
        };
        props.with_seconds = self.flag(18);
        props.value_format = self.text(19);
        props.clearable = self.flag(20);
        props.presets = parse_presets(self.get(21));
        props.number_of_months = self
            .get(22)
            .as_f64()
            .filter(|months| months.fract() == 0.0)
            .map_or(1, |months| months.clamp(1.0, 12.0) as usize);
        props.size = self.size;
        props.variant = match self.get(24).as_str() {
            Some("filled") => InputVariant::Filled,
            Some("unstyled") => InputVariant::Unstyled,
            _ => InputVariant::Default,
        };
        props.radius = self.radius;
        props.color = self.color;
        props.disabled = self.flag(27);
        props.read_only = self.flag(28);
        props.focusable = self.get(29).as_bool();
        props.today = self.get(30).as_str().and_then(date);
    }
}

#[derive(IntoElement)]
struct PickerDocument<V: DocumentValue> {
    key: ElementId,
    value: Value,
    default_value: Option<V>,
    open: Option<bool>,
    options: Options,
    handlers: [Option<EventHandler>; 2],
}

impl<V: DocumentValue> PickerDocument<V> {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            value: node.prop(1).clone(),
            default_value: V::parse(node.prop(2)),
            open: node.prop(3).as_bool(),
            options: Options::from_node(node),
            handlers: std::array::from_fn(|index| node.handler(index)),
        }
    }
}

struct PickerState<V: DocumentValue> {
    picker: Entity<Picker<V>>,
    value: Option<Value>,
    open: Option<bool>,
    options: Option<Options>,
    handlers: [Option<EventHandler>; 2],
    _subscription: Subscription,
}

impl<V: DocumentValue> RenderOnce for PickerDocument<V> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let mut props = PickerProps::<V>::new();
            props.default_value = self.default_value;
            self.options.apply(&mut props);
            let picker = Picker::with_props(props, window, cx);
            let subscription = cx.subscribe_in(
                &picker,
                window,
                |state: &mut PickerState<V>, picker, event, window, cx| {
                    let (index, payload) = match event {
                        PickerEvent::Change(value) => {
                            let seconds = picker.read(cx).props().with_seconds;
                            (0, value.map_or(Value::Null, |v| v.to_value(seconds)))
                        }
                        PickerEvent::OpenChange(open) => (1, Value::Bool(*open)),
                    };
                    if let Some(handler) = &state.handlers[index] {
                        handler(payload, window, cx);
                    }
                },
            );
            PickerState {
                picker,
                value: None,
                open: None,
                options: Some(self.options.clone()),
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            if state.options.as_ref() != Some(&self.options) {
                let options = self.options;
                state.picker.update(cx, |picker, cx| {
                    picker.update_props(|props| options.apply(props), window, cx)
                });
                state.options = Some(options);
            }
            // `null` leaves an uncontrolled picker alone until the document sets a value.
            if state.value.as_ref() != Some(&self.value) {
                if self.value != Value::Null || state.value.is_some() {
                    let value = V::parse(&self.value);
                    state
                        .picker
                        .update(cx, |picker, cx| picker.set_value(value, window, cx));
                }
                state.value = Some(self.value);
            }
            if state.open != self.open {
                if let Some(open) = self.open {
                    state
                        .picker
                        .update(cx, |picker, cx| picker.set_open(open, window, cx));
                }
                state.open = self.open;
            }
            state.picker.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_values_round_trip_for_every_mode() {
        let day = Value::from("2026-11-21");
        let parsed = NaiveDate::parse(&day);
        assert_eq!(parsed.map(|v| v.to_value(false)), Some(day));

        let clock = NaiveTime::parse(&"13:30".into());
        assert_eq!(clock.map(|v| v.to_value(false)), Some("13:30".into()));
        assert_eq!(clock.map(|v| v.to_value(true)), Some("13:30:00".into()));

        let moment = NaiveDateTime::parse(&"2026-11-21T08:05:09".into());
        assert_eq!(
            moment.map(|v| v.to_value(true)),
            Some("2026-11-21T08:05:09".into())
        );
        // A bare date is midnight.
        assert_eq!(
            NaiveDateTime::parse(&"2026-11-21".into()).map(|v| v.time()),
            Some(NaiveTime::MIN)
        );

        let range = Value::List(Arc::from(["2026-11-30".into(), "2026-11-01".into()]));
        let parsed = DateRange::parse(&range);
        assert_eq!(
            parsed.map(|v| v.to_value(false)),
            Some(Value::List(Arc::from([
                "2026-11-01".into(),
                "2026-11-30".into()
            ])))
        );
        assert!(DateTimeRange::parse(&"2026-11-01".into()).is_none());
        assert!(NaiveDate::parse(&"21/11/2026".into()).is_none());
    }

    #[test]
    fn presets_parse_relative_and_fixed_ranges() {
        let record = |fields: &[(&str, Value)]| {
            Value::Record(Arc::new(
                fields
                    .iter()
                    .map(|(name, value)| ((*name).into(), value.clone()))
                    .collect(),
            ))
        };
        let presets = Value::List(Arc::from([
            record(&[
                ("label", "Last week".into()),
                ("range", "last-days".into()),
                ("days", Value::Num(7.)),
            ]),
            record(&[
                ("label", "Q4".into()),
                ("range", "fixed".into()),
                ("start", "2026-10-01".into()),
                ("end", "2026-12-31".into()),
            ]),
            // Missing day count: skipped.
            record(&[("label", "Next".into()), ("range", "next-days".into())]),
        ]));
        let parsed = parse_presets(&presets);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].range, PresetRange::LastDays(7));
        assert_eq!(
            parsed[1].range,
            PresetRange::Fixed(
                NaiveDate::from_ymd_opt(2026, 10, 1).unwrap_or_default(),
                NaiveDate::from_ymd_opt(2026, 12, 31).unwrap_or_default()
            )
        );
    }
}
