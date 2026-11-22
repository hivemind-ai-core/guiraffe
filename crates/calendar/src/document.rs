//! Document adapter for the calendar views.

use std::sync::Arc;

use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc, Weekday};
use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window,
};
use guiraffe_core::{
    ColorName,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{
    Calendar, CalendarChange, CalendarEvent, CalendarItem, CalendarLocale, CalendarModel,
    CalendarSource, CalendarState, CalendarViewKind, CalendarZone, ChangeKind, EventId, ItemTime,
    SourceId, TimeFormat, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| CalendarDocument::new(node).into_any_element(),
        }],
    );
}

// Positions in the spec's `events`.
const NAVIGATE: usize = 0;
const VIEW_CHANGE: usize = 1;
const DATE_SELECT: usize = 2;
const ITEM_SELECT: usize = 3;
const OVERFLOW: usize = 4;
const EDIT_REQUEST: usize = 5;
const SOURCE_VISIBILITY: usize = 6;
const ANNOUNCE: usize = 7;
const ERROR: usize = 8;
const EVENTS: usize = 9;

const MAX_SAFE: f64 = ((1_u64 << 53) - 1) as f64;

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn field<'a>(value: &'a Value, name: &'static str) -> &'a Value {
    value.field(&[name.into()])
}

fn id(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|v| (0.0..=MAX_SAFE).contains(v) && v.fract() == 0.0)
        .map(|v| v as u64)
}

fn color(value: &Value) -> Result<Option<ColorName>, String> {
    match value.as_str() {
        Some(name) => ColorName::from_name(name)
            .map(Some)
            .ok_or_else(|| format!("unknown color \"{name}\"")),
        None => Ok(None),
    }
}

fn parse_sources(value: &Value) -> Result<Vec<CalendarSource>, String> {
    items(value)
        .enumerate()
        .map(|(index, record)| {
            let invalid = |what: &str| format!("sources[{index}]: {what}");
            let id = id(field(record, "id")).ok_or_else(|| invalid("id must be an integer"))?;
            let name = field(record, "name")
                .as_str()
                .ok_or_else(|| invalid("name must be a string"))?;
            let color = color(field(record, "color"))
                .map_err(|problem| invalid(&problem))?
                .unwrap_or_default();
            let mut source = CalendarSource::new(SourceId(id), name, color);
            source.visible = field(record, "visible").as_bool().unwrap_or(true);
            Ok(source)
        })
        .collect()
}

/// Timed records use RFC 3339 instants; all-day records use ISO dates with an exclusive end.
fn parse_items(value: &Value) -> Result<Vec<Arc<CalendarItem>>, String> {
    items(value)
        .enumerate()
        .map(|(index, record)| {
            let invalid = |what: &str| format!("items[{index}]: {what}");
            let integer = |name: &'static str| {
                id(field(record, name))
                    .ok_or_else(|| invalid(&format!("{name} must be an integer")))
            };
            let text = |name: &'static str| {
                field(record, name)
                    .as_str()
                    .ok_or_else(|| invalid(&format!("{name} must be a string")))
            };
            let (event, source) = (EventId(integer("id")?), SourceId(integer("source")?));
            let (title, start, end) = (text("title")?, text("start")?, text("end")?);
            let item = if field(record, "allDay").as_bool().unwrap_or(false) {
                let date = |text: &str| {
                    text.parse::<NaiveDate>()
                        .map_err(|_| invalid("all-day start and end must be YYYY-MM-DD dates"))
                };
                CalendarItem::all_day(event, source, title, date(start)?, date(end)?)
            } else {
                let instant = |text: &str| {
                    parse_instant(text)
                        .ok_or_else(|| invalid("timed start and end must be RFC 3339 instants"))
                };
                CalendarItem::timed(event, source, title, instant(start)?, instant(end)?)
            };
            let mut item = item
                .map_err(|problem| invalid(&problem.to_string()))?
                .locked(field(record, "locked").as_bool().unwrap_or(false));
            item.color = color(field(record, "color")).map_err(|problem| invalid(&problem))?;
            if let Some(icon) = field(record, "icon").as_str() {
                item = item.icon(icon);
            }
            Ok(Arc::new(item))
        })
        .collect()
}

fn parse_instant(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// `UTC`, an IANA name, or a fixed `+HH:MM` offset.
fn parse_zone(text: &str) -> Option<CalendarZone> {
    if text == "UTC" {
        return Some(CalendarZone::Utc);
    }
    text.parse::<chrono_tz::Tz>()
        .map(CalendarZone::Iana)
        .ok()
        .or_else(|| text.parse::<FixedOffset>().map(CalendarZone::Fixed).ok())
}

fn view_name(view: CalendarViewKind) -> &'static str {
    match view {
        CalendarViewKind::Year => "year",
        CalendarViewKind::Month => "month",
        CalendarViewKind::Week => "week",
        CalendarViewKind::Day => "day",
    }
}

fn record<const N: usize>(fields: [(&'static str, Value); N]) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    ))
}

fn change_payload(change: &CalendarChange) -> Value {
    let kind = match change.kind {
        ChangeKind::Create => "create",
        ChangeKind::Move => "move",
        ChangeKind::Resize => "resize",
    };
    let (all_day, start, end) = match &change.time {
        ItemTime::Timed { start, end } => (
            false,
            start.to_rfc3339_opts(SecondsFormat::Secs, true),
            end.to_rfc3339_opts(SecondsFormat::Secs, true),
        ),
        ItemTime::AllDay {
            start,
            end_exclusive,
        } => (true, start.to_string(), end_exclusive.to_string()),
    };
    record([
        ("kind", kind.into()),
        (
            "item",
            change
                .item
                .map_or(Value::Null, |id| Value::Num(id.0 as f64)),
        ),
        ("source", Value::Num(change.source.0 as f64)),
        ("allDay", all_day.into()),
        ("start", start.into()),
        ("end", end.into()),
    ])
}

fn payload(event: &CalendarEvent) -> (usize, Value) {
    match event {
        CalendarEvent::Navigated { date, .. } => (NAVIGATE, date.to_string().into()),
        CalendarEvent::ViewChanged(view) => (VIEW_CHANGE, view_name(*view).into()),
        CalendarEvent::DateSelected(date) => (DATE_SELECT, date.to_string().into()),
        CalendarEvent::ItemSelected(id) => (ITEM_SELECT, Value::Num(id.0 as f64)),
        CalendarEvent::OverflowRequested(date) => (OVERFLOW, date.to_string().into()),
        CalendarEvent::EditRequested(change) => (EDIT_REQUEST, change_payload(change)),
        CalendarEvent::SourceVisibilityChanged { source, visible } => (
            SOURCE_VISIBILITY,
            record([
                ("source", Value::Num(source.0 as f64)),
                ("visible", (*visible).into()),
            ]),
        ),
        CalendarEvent::Announcement(text) => (ANNOUNCE, text.clone().into()),
    }
}

/// The document's props, in the spec's order.
#[derive(Clone, PartialEq)]
struct Props {
    items: Value,
    sources: Value,
    view: CalendarViewKind,
    date: Value,
    now: Value,
    zone: Value,
    locale: CalendarLocale,
    week_start: Option<Weekday>,
    time_format: TimeFormat,
    iso_weeks: bool,
    business_hours: bool,
    now_line: bool,
    scroll_minutes: i32,
    editable: bool,
    focusable: Option<bool>,
    show_toolbar: bool,
}

impl Props {
    fn new(node: &dyn WidgetNode) -> Self {
        let flag = |index: usize, default: bool| node.prop(index).as_bool().unwrap_or(default);
        Self {
            items: node.prop(0).clone(),
            sources: node.prop(1).clone(),
            view: match node.prop(2).as_str() {
                Some("year") => CalendarViewKind::Year,
                Some("week") => CalendarViewKind::Week,
                Some("day") => CalendarViewKind::Day,
                _ => CalendarViewKind::Month,
            },
            date: node.prop(3).clone(),
            now: node.prop(4).clone(),
            zone: node.prop(5).clone(),
            locale: match node.prop(6).as_str() {
                Some("en-GB") => CalendarLocale::EN_GB,
                _ => CalendarLocale::EN_US,
            },
            // "locale" is not a weekday, so it parses to `None`.
            week_start: node.prop(7).as_str().and_then(|day| day.parse().ok()),
            time_format: match node.prop(8).as_str() {
                Some("12h") => TimeFormat::Hours12,
                Some("24h") => TimeFormat::Hours24,
                _ => TimeFormat::Locale,
            },
            iso_weeks: flag(9, false),
            business_hours: flag(10, true),
            now_line: flag(11, true),
            scroll_minutes: node
                .prop(12)
                .as_f64()
                .map_or(480, |minutes| minutes.clamp(0., 1_440.) as i32),
            editable: flag(13, true),
            focusable: node.prop(14).as_bool(),
            show_toolbar: flag(15, true),
        }
    }

    /// Apply only what changed, so one prop does not reset the navigation, scroll
    /// position or source visibility the user chose. Returns what was rejected.
    fn apply(
        &self,
        old: Option<&Self>,
        state: &mut CalendarState,
        cx: &mut Context<CalendarState>,
    ) -> Vec<String> {
        macro_rules! changed {
            ($field:ident) => {
                old.is_none_or(|old| old.$field != self.$field)
            };
        }
        let mut problems = Vec::new();

        // Before the date, whose default is today in the display zone.
        if changed!(zone) {
            let text = self.zone.as_str().unwrap_or("UTC");
            match parse_zone(text) {
                Some(zone) => state.set_zone(zone, cx),
                None => problems.push(format!("zone: unknown time zone \"{text}\"")),
            }
        }
        if changed!(now) {
            match self.now.as_str().map(|text| (text, parse_instant(text))) {
                Some((_, Some(now))) => state.set_now(now, cx),
                Some((text, None)) => {
                    problems.push(format!("now: \"{text}\" is not an RFC 3339 instant"));
                }
                None => state.set_live_now(true, cx),
            }
        }
        if changed!(items) || changed!(sources) {
            let sources = if changed!(sources) {
                parse_sources(&self.sources)
            } else {
                // Keep the visibility toggles the user made.
                Ok(state.model().sources().to_vec())
            };
            let model = sources.and_then(|sources| {
                CalendarModel::new(parse_items(&self.items)?, sources)
                    .map_err(|problem| problem.to_string())
            });
            match model {
                Ok(model) => state.set_model(model, cx),
                Err(problem) => problems.push(problem),
            }
        }
        if changed!(view) {
            state.show_view(self.view, cx);
        }
        if changed!(date) {
            match self
                .date
                .as_str()
                .map(|text| (text, text.parse::<NaiveDate>()))
            {
                Some((_, Ok(date))) => state.set_date(date, cx),
                Some((text, Err(_))) => problems.push(format!("date: \"{text}\" is not a date")),
                None if old.is_none() => {
                    let today = state.zone.local(state.now()).date_naive();
                    state.set_date(today, cx);
                }
                None => {}
            }
        }
        if changed!(locale) {
            state.set_locale(self.locale.clone(), cx);
        }
        if changed!(week_start) {
            state.set_first_weekday(self.week_start, cx);
        }
        if changed!(time_format) {
            state.set_time_format(self.time_format, cx);
        }
        if changed!(iso_weeks) {
            state.set_show_iso_weeks(self.iso_weeks, cx);
        }
        if changed!(business_hours) {
            state.set_show_business_hours(self.business_hours, cx);
        }
        if changed!(now_line) {
            state.set_show_now(self.now_line, cx);
        }
        if changed!(scroll_minutes) {
            state.set_scroll_minutes(self.scroll_minutes, cx);
        }
        if changed!(editable) {
            state.set_editable(self.editable, cx);
        }
        if changed!(focusable) {
            state.set_focusable(self.focusable, cx);
        }
        problems
    }
}

#[derive(IntoElement)]
struct CalendarDocument {
    key: ElementId,
    props: Props,
    handlers: [Option<EventHandler>; EVENTS],
}

impl CalendarDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            props: Props::new(node),
            handlers: std::array::from_fn(|index| node.handler(index)),
        }
    }
}

struct DocumentState {
    view: Entity<CalendarState>,
    props: Option<Props>,
    handlers: [Option<EventHandler>; EVENTS],
    _subscription: Subscription,
}

impl RenderOnce for CalendarDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let show_toolbar = self.props.show_toolbar;
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            // Start empty; the props are installed below like any later change.
            let view = cx.new(|cx| {
                CalendarState::new(
                    CalendarModel::default(),
                    Utc::now().date_naive(),
                    CalendarZone::Utc,
                    cx,
                )
            });
            let subscription = cx.subscribe_in(
                &view,
                window,
                |state: &mut DocumentState, _, event: &CalendarEvent, window, cx| {
                    let (index, value) = payload(event);
                    if let Some(handler) = &state.handlers[index] {
                        handler(value, window, cx);
                    }
                },
            );
            DocumentState {
                view,
                props: None,
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        let (view, error) = state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            let mut error = None;
            if state.props.as_ref() != Some(&self.props) {
                let problems = state.view.update(cx, |view, cx| {
                    self.props.apply(state.props.as_ref(), view, cx)
                });
                state.props = Some(self.props);
                if !problems.is_empty() {
                    error = state.handlers[ERROR].clone().zip(Some(problems.join("; ")));
                }
            }
            (state.view.clone(), error)
        });
        if let Some((handler, message)) = error {
            window.defer(cx, move |window, cx| {
                handler(Value::from(message), window, cx)
            });
        }
        Calendar::new(&view).show_toolbar(show_toolbar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(values: impl IntoIterator<Item = Value>) -> Value {
        Value::List(values.into_iter().collect())
    }

    fn item(start: &'static str, end: &'static str, all_day: bool) -> Value {
        record([
            ("id", Value::Num(7.)),
            ("source", Value::Num(1.)),
            ("title", "Review".into()),
            ("start", start.into()),
            ("end", end.into()),
            ("allDay", all_day.into()),
        ])
    }

    #[test]
    fn records_parse_both_time_kinds_and_report_bad_ones() {
        let timed = item(
            "2026-11-21T09:00:00+01:00",
            "2026-11-21T10:00:00+01:00",
            false,
        );
        let parsed = parse_items(&list([timed])).unwrap_or_else(|error| panic!("{error}"));
        let ItemTime::Timed { start, .. } = parsed[0].time else {
            panic!("timed")
        };
        assert_eq!(Some(start), parse_instant("2026-11-21T08:00:00Z"));
        assert!(parse_items(&list([item("2026-11-21", "2026-11-22", true)])).is_ok());
        for bad in [
            item("2026-11-21", "2026-11-22", false),
            item("2026-11-22", "2026-11-21", true),
            item("soon", "later", true),
            record([("id", Value::Num(1.5))]),
        ] {
            assert!(parse_items(&list([bad])).is_err_and(|error| error.starts_with("items[0]")));
        }
        let hidden = record([
            ("id", Value::Num(1.)),
            ("name", "Work".into()),
            ("color", "teal".into()),
            ("visible", false.into()),
        ]);
        let sources = parse_sources(&list([hidden])).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            (sources[0].color, sources[0].visible),
            (ColorName::Teal, false)
        );
    }

    #[test]
    fn zones_accept_utc_iana_and_fixed_offsets() {
        assert_eq!(parse_zone("UTC"), Some(CalendarZone::Utc));
        assert!(matches!(
            parse_zone("Europe/London"),
            Some(CalendarZone::Iana(_))
        ));
        assert!(matches!(parse_zone("+05:30"), Some(CalendarZone::Fixed(_))));
        assert_eq!(parse_zone("sample"), None);
    }

    #[test]
    fn handler_positions_match_the_descriptor() {
        let ids: Vec<_> = widget_specs::CALENDAR
            .events
            .iter()
            .map(|event| event.id)
            .collect();
        assert_eq!(ids.len(), EVENTS);
        for (index, id) in [
            (NAVIGATE, "navigate"),
            (VIEW_CHANGE, "view-change"),
            (DATE_SELECT, "date-select"),
            (ITEM_SELECT, "item-select"),
            (OVERFLOW, "overflow"),
            (EDIT_REQUEST, "edit-request"),
            (SOURCE_VISIBILITY, "source-visibility"),
            (ANNOUNCE, "announce"),
            (ERROR, "error"),
        ] {
            assert_eq!(ids[index], id);
        }
    }

    fn props() -> Props {
        Props {
            items: Value::Null,
            sources: list([record([("id", Value::Num(1.)), ("name", "Work".into())])]),
            view: CalendarViewKind::Month,
            date: "2026-11-21".into(),
            now: "2026-11-21T11:00:00Z".into(),
            zone: "UTC".into(),
            locale: CalendarLocale::EN_US,
            week_start: None,
            time_format: TimeFormat::Locale,
            iso_weeks: false,
            business_hours: true,
            now_line: true,
            scroll_minutes: 480,
            editable: true,
            focusable: None,
            show_toolbar: true,
        }
    }

    #[gpui_kit::test]
    fn changing_one_prop_keeps_user_state_and_echoes_no_events(cx: &mut gpui_kit::TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| {
            CalendarState::state(
                CalendarModel::default(),
                NaiveDate::MIN,
                CalendarZone::Utc,
                cx,
            )
        });
        let first = props();
        cx.update(|cx| {
            state.update(cx, |state, cx| {
                assert!(first.apply(None, state, cx).is_empty());
                state.navigate(1, cx);
                state.set_source_visible(SourceId(1), false, cx);
            })
        });
        cx.run_until_parked();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        cx.update(|cx| {
            cx.subscribe(&state, move |_, event: &CalendarEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        let mut next = first.clone();
        next.view = CalendarViewKind::Week;
        next.items = list([item("2026-11-21", "2026-11-22", true)]);
        cx.update(|cx| {
            state.update(cx, |state, cx| {
                assert!(next.apply(Some(&first), state, cx).is_empty());
            })
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let state = state.read(cx);
            assert_eq!(
                state.anchor().to_string(),
                "2026-12-21",
                "navigation is kept"
            );
            assert!(!state.model().sources()[0].visible, "toggles are kept");
            assert_eq!(state.view(), CalendarViewKind::Week);
        });
        assert!(
            seen.borrow().is_empty(),
            "props must not echo: {:?}",
            seen.borrow()
        );
    }
}
