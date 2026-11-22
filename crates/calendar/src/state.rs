use std::rc::Rc;

use chrono::{DateTime, Days, Duration, NaiveDate, Timelike, Utc};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, ScrollHandle, SharedString, Task,
    actions,
};

use crate::{
    BusinessHours, CalendarIndex, CalendarLocale, CalendarModel, CalendarViewKind, CalendarZone,
    EventId, ItemTime, ResizeEdge, SourceId, TimeFormat, move_all_day, move_timed, navigate,
    resize_all_day, resize_timed, visible_hour_range, week_start,
};

actions!(
    guiraffe_calendar,
    [
        CalendarToday,
        CalendarLeft,
        CalendarRight,
        CalendarUp,
        CalendarDown,
        CalendarShiftLeft,
        CalendarShiftRight,
        CalendarShiftUp,
        CalendarShiftDown,
        CalendarPageUp,
        CalendarPageDown,
        CalendarActivate,
        CalendarPickUp,
        CalendarCancel,
    ]
);

pub const KEY_CONTEXT: &str = "GrCalendar";

pub fn register_keys(cx: &mut App) {
    use gpui_kit::KeyBinding;
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("shift-left", CalendarShiftLeft, context),
        KeyBinding::new("shift-right", CalendarShiftRight, context),
        KeyBinding::new("shift-up", CalendarShiftUp, context),
        KeyBinding::new("shift-down", CalendarShiftDown, context),
        KeyBinding::new("home", CalendarToday, context),
        KeyBinding::new("left", CalendarLeft, context),
        KeyBinding::new("right", CalendarRight, context),
        KeyBinding::new("up", CalendarUp, context),
        KeyBinding::new("down", CalendarDown, context),
        KeyBinding::new("pageup", CalendarPageUp, context),
        KeyBinding::new("pagedown", CalendarPageDown, context),
        KeyBinding::new("enter", CalendarActivate, context),
        KeyBinding::new("space", CalendarPickUp, context),
        KeyBinding::new("escape", CalendarCancel, context),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Create,
    Move,
    Resize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarChange {
    pub kind: ChangeKind,
    pub item: Option<EventId>,
    pub source: SourceId,
    pub time: ItemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarEvent {
    Navigated {
        date: NaiveDate,
        view: CalendarViewKind,
    },
    ViewChanged(CalendarViewKind),
    DateSelected(NaiveDate),
    ItemSelected(EventId),
    OverflowRequested(NaiveDate),
    EditRequested(CalendarChange),
    SourceVisibilityChanged {
        source: SourceId,
        visible: bool,
    },
    Announcement(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Create,
    Move,
    Resize(ResizeEdge),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragPreview {
    pub mode: DragMode,
    pub item: Option<EventId>,
    pub source: SourceId,
    pub original: ItemTime,
    pub proposed: ItemTime,
    pub(crate) origin_date: NaiveDate,
    pub(crate) origin_minute: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarCursor {
    pub date: NaiveDate,
    pub minute: i32,
}

pub struct CalendarState {
    pub(crate) model: CalendarModel,
    pub(crate) index: CalendarIndex,
    pub(crate) view: CalendarViewKind,
    pub(crate) anchor: NaiveDate,
    pub(crate) zone: CalendarZone,
    pub(crate) locale: CalendarLocale,
    pub(crate) first_weekday: Option<chrono::Weekday>,
    pub(crate) time_format: TimeFormat,
    pub(crate) business_hours: BusinessHours,
    pub(crate) show_business_hours: bool,
    pub(crate) show_iso_weeks: bool,
    pub(crate) show_now: bool,
    pub(crate) live_now: bool,
    pub(crate) now: DateTime<Utc>,
    pub(crate) editable: bool,
    pub(crate) focusable: Option<bool>,
    pub(crate) focus: FocusHandle,
    pub(crate) view_switcher: Entity<guiraffe_components::SegmentedControl>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) scroll_minutes: i32,
    pub(crate) selected_date: NaiveDate,
    pub(crate) selected_item: Option<EventId>,
    pub(crate) cursor: CalendarCursor,
    pub(crate) overflow: Option<NaiveDate>,
    pub(crate) drag: Option<DragPreview>,
    /// Keyboard pick-up/create session driving `drag`; arrows accumulate here.
    pub(crate) keyboard: Option<KeyboardEdit>,
    pub(crate) pointer_origin_y: Option<f32>,
    /// Highlighted row of the open overflow list.
    pub(crate) overflow_cursor: usize,
    /// Height of the hour viewport in minutes, measured after paint.
    pub(crate) viewport_minutes: i32,
    /// Live clock; replaceable in tests.
    pub(crate) clock: fn() -> DateTime<Utc>,
    /// The minute timer. Armed after a paint that shows the now-line, cleared by
    /// every tick, so a hidden or unmounted calendar stops waking up.
    pub(crate) now_task: Option<Task<()>>,
    /// Accessible labels reused across repaints; cleared when they could change.
    pub(crate) labels: LabelCache,
}

/// Interned per-date and per-event accessible text, so an unchanged repaint
/// clones reference-counted strings instead of formatting them again.
#[derive(Default)]
pub(crate) struct LabelCache {
    dates: std::cell::RefCell<std::collections::HashMap<DateKey, (SharedString, SharedString)>>,
    events: std::cell::RefCell<std::collections::HashMap<(EventId, NaiveDate), EventLabels>>,
    /// Per-day timed layout keyed by date and lane limit. Rebuilt only after a
    /// model, zone or source-visibility change, never on drag or scroll frames.
    layouts: std::cell::RefCell<std::collections::HashMap<(NaiveDate, usize), Rc<DayLayout>>>,
}

/// A day column's overlap layout over the whole day: positioned blocks with the
/// index of their item in the day's index slice, and the collapsed overflow.
pub(crate) struct DayLayout {
    pub(crate) blocks: Vec<(usize, crate::PositionedEvent)>,
    pub(crate) hidden: usize,
    pub(crate) hidden_top: i32,
}

#[derive(Clone)]
pub(crate) struct EventLabels {
    pub(crate) id: SharedString,
    pub(crate) time: SharedString,
    pub(crate) label: SharedString,
}

/// A date cell's kind (element id prefix) and date.
type DateKey = (&'static str, NaiveDate);

/// Bound on interned entries; the cache is dropped and refilled past it.
const LABEL_CACHE_LIMIT: usize = 4_096;

impl LabelCache {
    pub(crate) fn clear(&self) {
        self.dates.borrow_mut().clear();
        self.events.borrow_mut().clear();
        self.layouts.borrow_mut().clear();
    }

    pub(crate) fn day_layout(
        &self,
        key: (NaiveDate, usize),
        build: impl FnOnce() -> DayLayout,
    ) -> Rc<DayLayout> {
        if let Some(layout) = self.layouts.borrow().get(&key) {
            return layout.clone();
        }
        let layout = Rc::new(build());
        let mut layouts = self.layouts.borrow_mut();
        if layouts.len() >= LABEL_CACHE_LIMIT {
            layouts.clear();
        }
        layouts.insert(key, layout.clone());
        layout
    }

    /// `(element id "{kind}-{date}", accessible label)` for a date cell.
    pub(crate) fn date(
        &self,
        kind: &'static str,
        date: NaiveDate,
        build: impl FnOnce() -> String,
    ) -> (SharedString, SharedString) {
        if let Some(entry) = self.dates.borrow().get(&(kind, date)) {
            return entry.clone();
        }
        let entry = (
            SharedString::from(format!("{kind}-{date}")),
            SharedString::from(build()),
        );
        let mut dates = self.dates.borrow_mut();
        if dates.len() >= LABEL_CACHE_LIMIT {
            dates.clear();
        }
        dates.insert((kind, date), entry.clone());
        entry
    }

    pub(crate) fn event(
        &self,
        key: (EventId, NaiveDate),
        build: impl FnOnce() -> (String, String),
    ) -> EventLabels {
        if let Some(labels) = self.events.borrow().get(&key) {
            return labels.clone();
        }
        let (time, label) = build();
        let labels = EventLabels {
            id: format!("timed-event-{}", key.0.0).into(),
            time: time.into(),
            label: label.into(),
        };
        let mut events = self.events.borrow_mut();
        if events.len() >= LABEL_CACHE_LIMIT {
            events.clear();
        }
        events.insert(key, labels.clone());
        labels
    }
}

/// Accumulated keyboard edit offsets, applied to the original item time so
/// repeated presses never drift.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyboardEdit {
    pub(crate) move_minutes: i64,
    pub(crate) resize_minutes: i64,
}

impl EventEmitter<CalendarEvent> for CalendarState {}

impl CalendarState {
    pub fn new(
        model: CalendarModel,
        anchor: NaiveDate,
        zone: CalendarZone,
        cx: &mut Context<Self>,
    ) -> Self {
        let index = model.index(zone);
        let focus = cx.focus_handle();
        let locale = CalendarLocale::default();
        let view_switcher = guiraffe_components::SegmentedControl::new(locale.labels.views, cx);
        view_switcher.update(cx, |switcher, cx| switcher.select(1, cx));
        cx.subscribe(&view_switcher, |state, _, event, cx| {
            let guiraffe_components::SegmentedEvent::Changed(index) = event;
            let view = match index {
                0 => CalendarViewKind::Year,
                1 => CalendarViewKind::Month,
                2 => CalendarViewKind::Week,
                _ => CalendarViewKind::Day,
            };
            state.set_view(view, cx);
        })
        .detach();
        let scroll = ScrollHandle::new();
        scroll.set_offset(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-480.)));
        let clock: fn() -> DateTime<Utc> = Utc::now;
        let now = clock();
        let mut state = Self {
            model,
            index,
            view: CalendarViewKind::Month,
            anchor,
            zone,
            locale,
            first_weekday: None,
            time_format: TimeFormat::Locale,
            business_hours: BusinessHours::default(),
            show_business_hours: true,
            show_iso_weeks: false,
            show_now: true,
            live_now: true,
            now,
            editable: true,
            focusable: None,
            focus,
            view_switcher,
            scroll,
            scroll_minutes: 480,
            selected_date: anchor,
            selected_item: None,
            cursor: CalendarCursor {
                date: anchor,
                minute: 480,
            },
            overflow: None,
            drag: None,
            keyboard: None,
            pointer_origin_y: None,
            overflow_cursor: 0,
            viewport_minutes: DEFAULT_VIEWPORT_MINUTES,
            clock,
            now_task: None,
            labels: LabelCache::default(),
        };
        state.reconcile_now_timer(cx);
        state
    }

    pub fn state(
        model: CalendarModel,
        anchor: NaiveDate,
        zone: CalendarZone,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(model, anchor, zone, cx))
    }

    pub fn model(&self) -> &CalendarModel {
        &self.model
    }
    pub fn view(&self) -> CalendarViewKind {
        self.view
    }
    pub fn anchor(&self) -> NaiveDate {
        self.anchor
    }
    pub fn selected_date(&self) -> NaiveDate {
        self.selected_date
    }
    pub fn selected_item(&self) -> Option<EventId> {
        self.selected_item
    }
    pub fn drag(&self) -> Option<&DragPreview> {
        self.drag.as_ref()
    }
    pub fn focused(&self) -> &FocusHandle {
        &self.focus
    }
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }
    pub fn now(&self) -> DateTime<Utc> {
        self.now
    }
    pub fn locale(&self) -> &CalendarLocale {
        &self.locale
    }
    pub fn time_format(&self) -> TimeFormat {
        self.locale.resolved_time_format(self.time_format)
    }
    pub fn first_weekday(&self) -> chrono::Weekday {
        self.first_weekday.unwrap_or(self.locale.first_weekday)
    }
    pub fn editable(&self) -> bool {
        self.editable
    }
    pub fn focusable(&self) -> Option<bool> {
        self.focusable
    }
    pub fn business_hours(&self) -> &BusinessHours {
        &self.business_hours
    }
    pub fn show_business_hours(&self) -> bool {
        self.show_business_hours
    }
    pub fn show_iso_weeks(&self) -> bool {
        self.show_iso_weeks
    }
    pub fn show_now(&self) -> bool {
        self.show_now
    }
    pub fn overflow(&self) -> Option<NaiveDate> {
        self.overflow
    }
    /// The roving keyboard cursor (date, and minute in Week/Day).
    pub fn cursor(&self) -> CalendarCursor {
        self.cursor
    }
    /// Whether a keyboard pick-up or keyboard create is in progress.
    pub fn keyboard_editing(&self) -> bool {
        self.keyboard.is_some()
    }
    /// First visible minute of the Week/Day hour grid.
    pub fn scroll_minutes(&self) -> i32 {
        self.scroll_minutes
    }
    /// Whether the live now-line minute timer is armed.
    pub fn now_timer_active(&self) -> bool {
        self.now_task.is_some()
    }

    pub fn set_view(&mut self, view: CalendarViewKind, cx: &mut Context<Self>) {
        if self.view == view {
            return;
        }
        self.cancel(cx);
        self.view = view;
        let selected = match view {
            CalendarViewKind::Year => 0,
            CalendarViewKind::Month => 1,
            CalendarViewKind::Week => 2,
            CalendarViewKind::Day => 3,
        };
        self.view_switcher
            .update(cx, |switcher, cx| switcher.select(selected, cx));
        self.overflow = None;
        cx.emit(CalendarEvent::ViewChanged(view));
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn navigate(&mut self, direction: i32, cx: &mut Context<Self>) {
        let next = navigate(self.anchor, self.view, direction);
        if next == self.anchor {
            return;
        }
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        self.anchor = next;
        self.selected_date = next;
        self.cursor.date = next;
        self.overflow = None;
        cx.emit(CalendarEvent::Navigated {
            date: next,
            view: self.view,
        });
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn today(&mut self, cx: &mut Context<Self>) {
        let today = self.zone.local(self.now).date_naive();
        self.anchor = today;
        self.selected_date = today;
        self.cursor.date = today;
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        self.overflow = None;
        cx.emit(CalendarEvent::Navigated {
            date: today,
            view: self.view,
        });
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn select_date(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        self.anchor = date;
        self.selected_date = date;
        self.cursor.date = date;
        self.selected_item = None;
        self.overflow = None;
        cx.emit(CalendarEvent::DateSelected(date));
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn select_item(&mut self, id: EventId, cx: &mut Context<Self>) {
        if self.model.item(id).is_none() {
            return;
        }
        self.selected_item = Some(id);
        cx.emit(CalendarEvent::ItemSelected(id));
        cx.notify();
    }

    pub fn set_overflow(&mut self, date: Option<NaiveDate>, cx: &mut Context<Self>) {
        if self.overflow == date {
            return;
        }
        self.overflow = date;
        self.overflow_cursor = 0;
        if let Some(date) = date {
            cx.emit(CalendarEvent::OverflowRequested(date));
        }
        cx.notify();
    }

    pub fn set_model(&mut self, model: CalendarModel, cx: &mut Context<Self>) {
        self.index = model.index(self.zone);
        self.labels.clear();
        self.model = model;
        if self.selected_item.is_some_and(|id| !self.visible_item(id)) {
            self.selected_item = None;
        }
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn set_zone(&mut self, zone: CalendarZone, cx: &mut Context<Self>) {
        if self.zone == zone {
            return;
        }
        self.zone = zone;
        self.labels.clear();
        self.index = self.model.index(zone);
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn set_locale(&mut self, locale: CalendarLocale, cx: &mut Context<Self>) {
        if self.locale == locale {
            return;
        }
        self.locale = locale;
        self.labels.clear();
        cx.notify();
    }

    pub fn set_first_weekday(&mut self, weekday: Option<chrono::Weekday>, cx: &mut Context<Self>) {
        if self.first_weekday == weekday {
            return;
        }
        self.first_weekday = weekday;
        cx.notify();
    }

    pub fn set_time_format(&mut self, format: TimeFormat, cx: &mut Context<Self>) {
        if self.time_format == format {
            return;
        }
        self.time_format = format;
        self.labels.clear();
        cx.notify();
    }

    pub fn set_business_hours(&mut self, hours: BusinessHours, cx: &mut Context<Self>) {
        self.business_hours = hours;
        cx.notify();
    }

    pub fn set_show_business_hours(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.show_business_hours == show {
            return;
        }
        self.show_business_hours = show;
        cx.notify();
    }

    pub fn set_show_iso_weeks(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.show_iso_weeks == show {
            return;
        }
        self.show_iso_weeks = show;
        cx.notify();
    }

    pub fn set_show_now(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.show_now == show {
            return;
        }
        self.show_now = show;
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    /// Inject a fixed `now`; switches off the live clock and its timer.
    pub fn set_now(&mut self, now: DateTime<Utc>, cx: &mut Context<Self>) {
        self.live_now = false;
        self.now_task = None;
        if self.now == now {
            return;
        }
        self.now = now;
        cx.notify();
    }

    /// Replace the live clock (tests and deterministic hosts).
    #[doc(hidden)]
    pub fn set_clock(&mut self, clock: fn() -> DateTime<Utc>, cx: &mut Context<Self>) {
        self.clock = clock;
        if self.live_now {
            self.now = clock();
            cx.notify();
        }
    }

    pub fn set_live_now(&mut self, live: bool, cx: &mut Context<Self>) {
        if self.live_now == live {
            return;
        }
        self.live_now = live;
        if live {
            self.now = (self.clock)();
        }
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn set_editable(&mut self, editable: bool, cx: &mut Context<Self>) {
        if self.editable == editable {
            return;
        }
        self.editable = editable;
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        cx.notify();
    }

    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        if self.focusable == focusable {
            return;
        }
        self.focusable = focusable;
        cx.notify();
    }

    /// Scroll the Week/Day hour grid so `minutes` is the first visible minute.
    pub fn set_scroll_minutes(&mut self, minutes: i32, cx: &mut Context<Self>) {
        let minutes = minutes.clamp(0, (1_440 - self.viewport_minutes).max(0));
        if self.scroll_minutes == minutes {
            return;
        }
        self.scroll_minutes = minutes;
        self.scroll.set_offset(gpui_kit::point(
            gpui_kit::px(0.),
            gpui_kit::px(-(minutes as f32)),
        ));
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn set_source_visible(&mut self, source: SourceId, visible: bool, cx: &mut Context<Self>) {
        let Some(_) = self.model.set_source_visible(source, visible) else {
            return;
        };
        self.labels.clear();
        self.index = self.model.index(self.zone);
        if self.selected_item.is_some_and(|id| !self.visible_item(id)) {
            self.selected_item = None;
        }
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        cx.emit(CalendarEvent::SourceVisibilityChanged { source, visible });
        cx.notify();
    }

    fn visible_item(&self, id: EventId) -> bool {
        let Some(item) = self.model.item(id) else {
            return false;
        };
        self.model
            .source(item.source)
            .is_some_and(|source| source.visible)
    }

    fn default_source(&self) -> Option<SourceId> {
        self.model
            .sources()
            .iter()
            .find(|source| source.visible)
            .map(|source| source.id)
    }

    pub fn begin_all_day_create(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        if !self.editable || self.drag.is_some() || self.keyboard.is_some() {
            return;
        }
        let Some(source) = self.default_source() else {
            return;
        };
        let Some(end_exclusive) = date.succ_opt() else {
            return;
        };
        let time = ItemTime::AllDay {
            start: date,
            end_exclusive,
        };
        self.drag = Some(DragPreview {
            mode: DragMode::Create,
            item: None,
            source,
            original: time.clone(),
            proposed: time,
            origin_date: date,
            origin_minute: 0,
        });
        cx.notify();
    }

    pub fn begin_create(&mut self, date: NaiveDate, minute: i32, cx: &mut Context<Self>) {
        if !self.editable || self.keyboard.is_some() {
            return;
        }
        let Some(source) = self.default_source() else {
            return;
        };
        let minute = minute.clamp(0, 1_425) / 15 * 15;
        let Some(start) = local_instant(self.zone, date, minute) else {
            return;
        };
        let Some(end) = start.checked_add_signed(Duration::hours(1)) else {
            return;
        };
        let time = ItemTime::Timed { start, end };
        self.drag = Some(DragPreview {
            mode: DragMode::Create,
            item: None,
            source,
            original: time.clone(),
            proposed: time,
            origin_date: date,
            origin_minute: minute,
        });
        cx.notify();
    }

    pub fn begin_item_drag(
        &mut self,
        id: EventId,
        mode: DragMode,
        date: NaiveDate,
        minute: i32,
        cx: &mut Context<Self>,
    ) {
        if !self.editable || self.keyboard.is_some() || !self.visible_item(id) {
            return;
        }
        let Some(item) = self.model.item(id) else {
            return;
        };
        if item.locked {
            return;
        }
        self.drag = Some(DragPreview {
            mode,
            item: Some(id),
            source: item.source,
            original: item.time.clone(),
            proposed: item.time.clone(),
            origin_date: date,
            origin_minute: minute,
        });
        cx.notify();
    }

    pub fn update_timed_drag(&mut self, date: NaiveDate, minute: i32, cx: &mut Context<Self>) {
        if self.keyboard.is_some() || self.drag.is_none() {
            return;
        }
        if minute < 0 {
            self.set_scroll_minutes(self.scroll_minutes - 15, cx);
        } else if minute > 1_440 {
            self.set_scroll_minutes(self.scroll_minutes + 15, cx);
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let delta = date
            .signed_duration_since(drag.origin_date)
            .num_days()
            .saturating_mul(1_440)
            + i64::from(minute - drag.origin_minute);
        let proposed = match (&drag.original, drag.mode) {
            (ItemTime::Timed { start, end }, DragMode::Move) => {
                move_timed(*start, *end, delta).map(|(start, end)| ItemTime::Timed { start, end })
            }
            (
                ItemTime::Timed { start, end },
                DragMode::Create | DragMode::Resize(ResizeEdge::End),
            ) => resize_timed(*start, *end, ResizeEdge::End, delta)
                .map(|(start, end)| ItemTime::Timed { start, end }),
            (ItemTime::Timed { start, end }, DragMode::Resize(ResizeEdge::Start)) => {
                resize_timed(*start, *end, ResizeEdge::Start, delta)
                    .map(|(start, end)| ItemTime::Timed { start, end })
            }
            (ItemTime::AllDay { .. }, _) => None,
        };
        if let Some(proposed) = proposed {
            drag.proposed = proposed;
            cx.notify();
        }
    }

    pub fn update_day_drag(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        if self.keyboard.is_some() {
            return;
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let delta = date.signed_duration_since(drag.origin_date).num_days();
        let proposed = match (&drag.original, drag.mode) {
            (
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
                DragMode::Move,
            ) => move_all_day(*start, *end_exclusive, delta).map(|(start, end_exclusive)| {
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                }
            }),
            (
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
                DragMode::Create | DragMode::Resize(ResizeEdge::End),
            ) => resize_all_day(*start, *end_exclusive, ResizeEdge::End, delta).map(
                |(start, end_exclusive)| ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
            ),
            (
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
                DragMode::Resize(ResizeEdge::Start),
            ) => resize_all_day(*start, *end_exclusive, ResizeEdge::Start, delta).map(
                |(start, end_exclusive)| ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
            ),
            (ItemTime::Timed { .. }, _) => None,
        };
        if let Some(proposed) = proposed
            && drag.proposed != proposed
        {
            drag.proposed = proposed;
            cx.notify();
        }
    }

    /// Commit the active pointer or keyboard preview as one proposed change.
    ///
    /// A gesture without effect emits nothing: an unchanged move or resize is a
    /// click, and an all-day create without travel selects its date instead.
    pub fn commit_drag(&mut self, cx: &mut Context<Self>) -> Option<CalendarChange> {
        let drag = self.drag.take()?;
        let keyboard = self.keyboard.take();
        self.pointer_origin_y = None;
        cx.notify();
        let unchanged = drag.proposed == drag.original;
        let kind = match drag.mode {
            DragMode::Create => ChangeKind::Create,
            DragMode::Move => ChangeKind::Move,
            DragMode::Resize(_) => ChangeKind::Resize,
        };
        let kind = match keyboard {
            Some(edit) if kind != ChangeKind::Create && edit.move_minutes == 0 => {
                ChangeKind::Resize
            }
            Some(_) if kind != ChangeKind::Create => ChangeKind::Move,
            _ => kind,
        };
        if unchanged && kind != ChangeKind::Create {
            return None;
        }
        if unchanged
            && keyboard.is_none()
            && let ItemTime::AllDay { start, .. } = drag.original
        {
            self.select_date(start, cx);
            return None;
        }
        let change = CalendarChange {
            kind,
            item: drag.item,
            source: drag.source,
            time: drag.proposed,
        };
        cx.emit(CalendarEvent::EditRequested(change.clone()));
        cx.emit(CalendarEvent::Announcement(
            match kind {
                ChangeKind::Create => "New event proposed",
                ChangeKind::Move => "Event move proposed",
                ChangeKind::Resize => "Event resize proposed",
            }
            .into(),
        ));
        Some(change)
    }

    /// Escape: drop any preview (restoring the original) and close overflow.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.drag.is_none() && self.keyboard.is_none() && self.overflow.is_none() {
            return;
        }
        let editing = self.drag.is_some();
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard = None;
        self.overflow = None;
        if editing {
            cx.emit(CalendarEvent::Announcement(
                "Calendar edit cancelled".into(),
            ));
        }
        cx.notify();
    }

    /// Space: pick up the selected editable item, or start a keyboard create at
    /// the cursor; a second Space (or Enter) drops it as one proposed change.
    pub fn pick_up(&mut self, cx: &mut Context<Self>) -> Option<CalendarChange> {
        if self.keyboard.is_some() {
            return self.commit_drag(cx);
        }
        if !self.editable || self.drag.is_some() || self.view == CalendarViewKind::Year {
            return None;
        }
        let picked = self
            .selected_item
            .filter(|id| self.visible_item(*id))
            .and_then(|id| self.model.item(id))
            .map(|item| (item.id, item.source, item.time.clone(), item.locked));
        match picked {
            Some((_, _, _, true)) => {
                cx.emit(CalendarEvent::Announcement("Event is locked".into()));
                return None;
            }
            Some((id, source, time, false)) => {
                self.drag = Some(DragPreview {
                    mode: DragMode::Move,
                    item: Some(id),
                    source,
                    original: time.clone(),
                    proposed: time,
                    origin_date: self.cursor.date,
                    origin_minute: self.cursor.minute,
                });
                cx.emit(CalendarEvent::Announcement("Event picked up".into()));
            }
            None => {
                let date = self.cursor.date;
                let minute = self.cursor.minute;
                if self.view == CalendarViewKind::Month {
                    self.begin_all_day_create(date, cx);
                } else {
                    self.begin_create(date, minute, cx);
                }
                self.drag.as_ref()?;
                cx.emit(CalendarEvent::Announcement("New event started".into()));
            }
        }
        self.keyboard = Some(KeyboardEdit::default());
        cx.notify();
        None
    }

    /// Arrow keys. `shift` selects the all-day/resize variant from the spec table.
    pub fn arrow(&mut self, dx: i32, dy: i32, shift: bool, cx: &mut Context<Self>) {
        if self.overflow.is_some() {
            if dy != 0 {
                let count = self.overflow.map_or(0, |date| self.index.day(date).len());
                let last = count.saturating_sub(1);
                self.overflow_cursor = if dy < 0 {
                    self.overflow_cursor.saturating_sub(1)
                } else {
                    (self.overflow_cursor + 1).min(last)
                };
                cx.notify();
            }
            return;
        }
        if self.keyboard.is_some() {
            self.keyboard_nudge(dx, dy, shift, cx);
            return;
        }
        let timed = matches!(self.view, CalendarViewKind::Week | CalendarViewKind::Day);
        if timed && !shift {
            // Left/Right and Up/Down both step the timed cursor by fifteen minutes.
            let step = if dx != 0 { dx } else { dy };
            let minute = (self.cursor.minute + step * 15).clamp(0, 1_425);
            if minute != self.cursor.minute {
                self.cursor.minute = minute;
                self.scroll_cursor_into_view(cx);
                cx.notify();
            }
            return;
        }
        let days = if timed {
            i64::from(dx)
        } else {
            i64::from(dx) + if shift { 0 } else { i64::from(dy) * 7 }
        };
        if days != 0 {
            self.move_cursor_days(days, cx);
        }
    }

    fn keyboard_nudge(&mut self, dx: i32, dy: i32, shift: bool, cx: &mut Context<Self>) {
        let (Some(mut edit), Some(drag)) = (self.keyboard, self.drag.as_ref()) else {
            return;
        };
        let all_day = matches!(drag.original, ItemTime::AllDay { .. });
        let delta = match (all_day, dx, dy) {
            (_, dx, 0) if dx != 0 => i64::from(dx) * 1_440,
            (true, _, dy) => i64::from(dy) * 7 * 1_440,
            (false, _, dy) => i64::from(dy) * 15,
        };
        if shift {
            edit.resize_minutes += delta;
        } else {
            edit.move_minutes += delta;
        }
        let Some(proposed) = keyboard_proposal(&drag.original, edit) else {
            return;
        };
        if let Some(drag) = self.drag.as_mut() {
            drag.proposed = proposed;
        }
        self.keyboard = Some(edit);
        cx.notify();
    }

    /// Enter: commit an active preview, choose from the overflow list, or
    /// activate the cursor's date/item.
    pub fn activate(&mut self, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            self.commit_drag(cx);
            return;
        }
        if let Some(date) = self.overflow {
            let id = self
                .index
                .day(date)
                .get(self.overflow_cursor)
                .map(|item| item.id);
            self.set_overflow(None, cx);
            if let Some(id) = id {
                self.select_item(id, cx);
            }
            return;
        }
        let date = self.cursor.date;
        match self.view {
            CalendarViewKind::Year => {
                self.select_date(date, cx);
                self.set_view(CalendarViewKind::Day, cx);
            }
            CalendarViewKind::Month => {
                if self.index.day(date).is_empty() {
                    self.select_date(date, cx);
                } else {
                    self.set_overflow(Some(date), cx);
                }
            }
            CalendarViewKind::Week | CalendarViewKind::Day => {
                let slot = (self.cursor.minute * 60, (self.cursor.minute + 15) * 60);
                let zone = self.zone;
                let covering: Vec<EventId> = self
                    .index
                    .day(date)
                    .iter()
                    .filter(|item| {
                        crate::seconds_in_display_day(item, date, zone)
                            .is_some_and(|(start, end)| start < slot.1 && end > slot.0)
                    })
                    .map(|item| item.id)
                    .collect();
                let next = match self
                    .selected_item
                    .and_then(|id| covering.iter().position(|item| *item == id))
                {
                    Some(position) => covering.get(position + 1).or(covering.first()),
                    None => covering.first(),
                };
                match next {
                    Some(id) => self.select_item(*id, cx),
                    None => self.select_date(date, cx),
                }
            }
        }
    }

    /// Home: move the cursor to today without changing the view kind.
    pub fn cursor_to_today(&mut self, cx: &mut Context<Self>) {
        self.today(cx);
    }

    fn move_cursor_days(&mut self, days: i64, cx: &mut Context<Self>) {
        let date = if days < 0 {
            self.cursor
                .date
                .checked_sub_days(Days::new(days.unsigned_abs()))
        } else {
            self.cursor.date.checked_add_days(Days::new(days as u64))
        };
        let Some(date) = date else {
            return;
        };
        let before = self.visible_key(self.anchor);
        self.cursor.date = date;
        self.selected_date = date;
        self.anchor = date;
        if self.visible_key(date) != before {
            self.overflow = None;
            cx.emit(CalendarEvent::Navigated {
                date,
                view: self.view,
            });
            self.reconcile_now_timer(cx);
        }
        cx.notify();
    }

    /// Identity of the visible range containing `date` in the current view.
    fn visible_key(&self, date: NaiveDate) -> (i32, u32) {
        use chrono::Datelike;
        match self.view {
            CalendarViewKind::Year => (date.year(), 0),
            CalendarViewKind::Month => (date.year(), date.month()),
            CalendarViewKind::Week => week_start(date, self.first_weekday())
                .map_or((date.year(), date.ordinal()), |first| {
                    (first.year(), first.ordinal())
                }),
            CalendarViewKind::Day => (date.year(), date.ordinal()),
        }
    }

    fn scroll_cursor_into_view(&mut self, cx: &mut Context<Self>) {
        let minute = self.cursor.minute;
        if minute < self.scroll_minutes {
            self.set_scroll_minutes(minute, cx);
        } else if minute + 15 > self.scroll_minutes + self.viewport_minutes {
            self.set_scroll_minutes(minute + 15 - self.viewport_minutes, cx);
        }
    }

    /// Programmatic cursor movement: `horizontal` days and `vertical` weeks in
    /// Year/Month, `vertical` fifteen-minute steps in Week/Day.
    pub fn move_cursor(&mut self, horizontal: i32, vertical: i32, cx: &mut Context<Self>) {
        if horizontal != 0 {
            self.move_cursor_days(i64::from(horizontal), cx);
        }
        if vertical != 0 {
            self.arrow(0, vertical, false, cx);
        }
    }

    fn now_line_visible(&self) -> bool {
        if !self.show_now || !matches!(self.view, CalendarViewKind::Week | CalendarViewKind::Day) {
            return false;
        }
        let local = self.zone.local(self.now);
        let today = local.date_naive();
        let visible = match self.view {
            CalendarViewKind::Week => {
                week_start(self.anchor, self.first_weekday()).is_some_and(|start| {
                    start <= today && today.signed_duration_since(start).num_days() < 7
                })
            }
            _ => self.anchor == today,
        };
        if !visible {
            return false;
        }
        let minute = (local.hour() * 60 + local.minute()) as i32;
        let (start, end) = visible_hour_range(self.scroll_minutes, self.viewport_minutes, 0);
        start <= minute && minute <= end
    }

    /// Whether a live minute timer should run for the current state.
    fn wants_now_timer(&self) -> bool {
        self.live_now && self.now_line_visible()
    }

    /// Stop the timer when the state no longer shows a live now-line. Arming
    /// only happens from [`Self::sync_after_paint`], i.e. when a frame really
    /// painted the hour grid.
    fn reconcile_now_timer(&mut self, _: &mut Context<Self>) {
        if !self.wants_now_timer() {
            self.now_task = None;
        }
    }

    /// Cheap per-paint check: does the painted hour grid disagree with the
    /// state (scroll position, viewport) or need a timer? Never allocates.
    pub(crate) fn needs_paint_sync(&self, scroll: i32, viewport: i32, on_screen: bool) -> bool {
        scroll != self.scroll_minutes
            || viewport != self.viewport_minutes
            || (self.now_task.is_some() && !on_screen)
            || (on_screen && self.now_task.is_none() && self.wants_now_timer())
    }

    /// Record the painted scroll position and viewport, then arm or stop the
    /// minute timer according to whether the now-line is visible on screen.
    pub(crate) fn sync_after_paint(
        &mut self,
        scroll: i32,
        viewport: i32,
        on_screen: bool,
        cx: &mut Context<Self>,
    ) {
        self.scroll_minutes = scroll.clamp(0, 1_440);
        self.viewport_minutes = viewport.clamp(0, 1_440);
        if !on_screen || !self.live_now {
            self.now_task = None;
            return;
        }
        let now = (self.clock)();
        if now.timestamp() / 60 != self.now.timestamp() / 60 {
            // Shown again after being hidden: catch up once.
            self.now = now;
            cx.notify();
        }
        if !self.wants_now_timer() {
            self.now_task = None;
            return;
        }
        if self.now_task.is_none() {
            self.arm_now_timer(cx);
        }
    }

    fn arm_now_timer(&mut self, cx: &mut Context<Self>) {
        let now = (self.clock)();
        let delay = std::time::Duration::from_secs(60 - u64::from(now.second() % 60));
        self.now_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                // One tick: refresh and notify. The next paint re-arms; a hidden
                // calendar does not paint, so the timer stops here.
                this.now_task = None;
                this.now = (this.clock)();
                cx.notify();
            });
        }));
    }
}

/// Default hour viewport before the first paint measures it.
pub(crate) const DEFAULT_VIEWPORT_MINUTES: i32 = 600;

fn keyboard_proposal(original: &ItemTime, edit: KeyboardEdit) -> Option<ItemTime> {
    match original {
        ItemTime::Timed { start, end } => {
            let (start, end) = move_timed(*start, *end, edit.move_minutes)?;
            let (start, end) = if edit.resize_minutes == 0 {
                (start, end)
            } else {
                resize_timed(start, end, ResizeEdge::End, edit.resize_minutes)?
            };
            Some(ItemTime::Timed { start, end })
        }
        ItemTime::AllDay {
            start,
            end_exclusive,
        } => {
            let (start, end_exclusive) =
                move_all_day(*start, *end_exclusive, edit.move_minutes.div_euclid(1_440))?;
            let (start, end_exclusive) = if edit.resize_minutes == 0 {
                (start, end_exclusive)
            } else {
                resize_all_day(
                    start,
                    end_exclusive,
                    ResizeEdge::End,
                    edit.resize_minutes.div_euclid(1_440),
                )?
            };
            Some(ItemTime::AllDay {
                start,
                end_exclusive,
            })
        }
    }
}

fn local_instant(zone: CalendarZone, date: NaiveDate, minute: i32) -> Option<DateTime<Utc>> {
    let hour = u32::try_from(minute.div_euclid(60)).ok()?;
    let minute = u32::try_from(minute.rem_euclid(60)).ok()?;
    let naive = date.and_hms_opt(hour, minute, 0)?;
    zone.single_from_local(naive)
}
