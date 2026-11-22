use chrono::{DateTime, Days, Duration, NaiveDate, Timelike, Utc};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, ScrollHandle, Task, actions,
};

use crate::{
    BusinessHours, CalendarIndex, CalendarLocale, CalendarModel, CalendarViewKind, CalendarZone,
    EventId, ItemTime, ResizeEdge, SourceId, TimeFormat, move_all_day, move_timed, navigate,
    resize_all_day, resize_timed, visible_hour_range, week_dates,
};

actions!(
    guiraffe_calendar,
    [
        CalendarPrevious,
        CalendarNext,
        CalendarToday,
        CalendarLeft,
        CalendarRight,
        CalendarUp,
        CalendarDown,
        CalendarPageUp,
        CalendarPageDown,
        CalendarActivate,
        CalendarPickUp,
        CalendarCancel,
        CalendarResizeStart,
        CalendarResizeEnd,
    ]
);

pub const KEY_CONTEXT: &str = "GrCalendar";

pub fn register_keys(cx: &mut App) {
    use gpui_kit::KeyBinding;
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("shift-left", CalendarPrevious, context),
        KeyBinding::new("shift-right", CalendarNext, context),
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
        KeyBinding::new("shift-up", CalendarResizeStart, context),
        KeyBinding::new("shift-down", CalendarResizeEnd, context),
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
    pub(crate) keyboard_edit: Option<(EventId, DragMode)>,
    pub(crate) pointer_origin_y: Option<f32>,
    pub(crate) now_task: Option<Task<()>>,
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
        let view_switcher =
            guiraffe_components::SegmentedControl::new(["Year", "Month", "Week", "Day"], cx);
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
        let now = Utc::now();
        let mut state = Self {
            model,
            index,
            view: CalendarViewKind::Month,
            anchor,
            zone,
            locale: CalendarLocale::default(),
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
            keyboard_edit: None,
            pointer_origin_y: None,
            now_task: None,
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
        self.keyboard_edit = None;
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
        self.keyboard_edit = None;
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
        if let Some(date) = date {
            cx.emit(CalendarEvent::OverflowRequested(date));
        }
        cx.notify();
    }

    pub fn set_model(&mut self, model: CalendarModel, cx: &mut Context<Self>) {
        self.index = model.index(self.zone);
        self.model = model;
        if self.selected_item.is_some_and(|id| !self.visible_item(id)) {
            self.selected_item = None;
        }
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard_edit = None;
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn set_zone(&mut self, zone: CalendarZone, cx: &mut Context<Self>) {
        if self.zone == zone {
            return;
        }
        self.zone = zone;
        self.index = self.model.index(zone);
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard_edit = None;
        cx.notify();
        self.reconcile_now_timer(cx);
    }

    pub fn set_locale(&mut self, locale: CalendarLocale, cx: &mut Context<Self>) {
        if self.locale == locale {
            return;
        }
        self.locale = locale;
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

    pub fn set_now(&mut self, now: DateTime<Utc>, cx: &mut Context<Self>) {
        self.live_now = false;
        self.now = now;
        self.now_task = None;
        cx.notify();
    }

    pub fn set_live_now(&mut self, live: bool, cx: &mut Context<Self>) {
        if self.live_now == live {
            return;
        }
        self.live_now = live;
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
        self.keyboard_edit = None;
        cx.notify();
    }

    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        if self.focusable == focusable {
            return;
        }
        self.focusable = focusable;
        cx.notify();
    }

    pub fn set_scroll_minutes(&mut self, minutes: i32, cx: &mut Context<Self>) {
        let minutes = minutes.clamp(0, 840);
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
        self.index = self.model.index(self.zone);
        if self.selected_item.is_some_and(|id| !self.visible_item(id)) {
            self.selected_item = None;
        }
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard_edit = None;
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
        if !self.editable || self.drag.is_some() {
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
        if !self.editable {
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
        if !self.editable {
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
        if let Some(proposed) = proposed {
            drag.proposed = proposed;
            cx.notify();
        }
    }

    pub fn commit_drag(&mut self, cx: &mut Context<Self>) -> Option<CalendarChange> {
        let drag = self.drag.take()?;
        self.pointer_origin_y = None;
        let change = CalendarChange {
            kind: match drag.mode {
                DragMode::Create => ChangeKind::Create,
                DragMode::Move => ChangeKind::Move,
                DragMode::Resize(_) => ChangeKind::Resize,
            },
            item: drag.item,
            source: drag.source,
            time: drag.proposed,
        };
        cx.emit(CalendarEvent::EditRequested(change.clone()));
        cx.emit(CalendarEvent::Announcement("Calendar edit proposed".into()));
        cx.notify();
        Some(change)
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.drag.is_none() && self.keyboard_edit.is_none() && self.overflow.is_none() {
            return;
        }
        self.drag = None;
        self.pointer_origin_y = None;
        self.keyboard_edit = None;
        self.overflow = None;
        cx.emit(CalendarEvent::Announcement(
            "Calendar edit cancelled".into(),
        ));
        cx.notify();
    }

    pub fn keyboard_move_selected(
        &mut self,
        delta: i64,
        edge: Option<ResizeEdge>,
        cx: &mut Context<Self>,
    ) -> Option<CalendarChange> {
        if !self.editable {
            return None;
        }
        let item = self
            .selected_item
            .and_then(|id| self.model.item(id).filter(|item| !item.locked))?;
        let (time, kind) = match (&item.time, edge) {
            (ItemTime::Timed { start, end }, None) => (
                move_timed(*start, *end, delta).map(|(start, end)| ItemTime::Timed { start, end }),
                ChangeKind::Move,
            ),
            (ItemTime::Timed { start, end }, Some(edge)) => (
                resize_timed(*start, *end, edge, delta)
                    .map(|(start, end)| ItemTime::Timed { start, end }),
                ChangeKind::Resize,
            ),
            (
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
                None,
            ) => (
                move_all_day(*start, *end_exclusive, delta.div_euclid(1_440)).map(
                    |(start, end_exclusive)| ItemTime::AllDay {
                        start,
                        end_exclusive,
                    },
                ),
                ChangeKind::Move,
            ),
            (
                ItemTime::AllDay {
                    start,
                    end_exclusive,
                },
                Some(edge),
            ) => (
                resize_all_day(*start, *end_exclusive, edge, delta.div_euclid(1_440)).map(
                    |(start, end_exclusive)| ItemTime::AllDay {
                        start,
                        end_exclusive,
                    },
                ),
                ChangeKind::Resize,
            ),
        };
        let time = time?;
        let change = CalendarChange {
            kind,
            item: Some(item.id),
            source: item.source,
            time,
        };
        cx.emit(CalendarEvent::EditRequested(change.clone()));
        cx.notify();
        Some(change)
    }

    pub fn move_cursor(&mut self, horizontal: i32, vertical: i32, cx: &mut Context<Self>) {
        let days = match self.view {
            CalendarViewKind::Year | CalendarViewKind::Month => {
                i64::from(horizontal) + i64::from(vertical) * 7
            }
            CalendarViewKind::Week => i64::from(horizontal),
            CalendarViewKind::Day => i64::from(horizontal),
        };
        let date = if days < 0 {
            self.cursor
                .date
                .checked_sub_days(Days::new(days.unsigned_abs()))
        } else {
            self.cursor.date.checked_add_days(Days::new(days as u64))
        };
        if let Some(date) = date {
            self.cursor.date = date;
            self.selected_date = date;
            self.anchor = date;
        }
        if matches!(self.view, CalendarViewKind::Week | CalendarViewKind::Day) {
            self.cursor.minute = (self.cursor.minute + vertical * 15).clamp(0, 1_425);
        }
        cx.notify();
    }

    fn now_line_visible(&self) -> bool {
        if !self.show_now || !matches!(self.view, CalendarViewKind::Week | CalendarViewKind::Day) {
            return false;
        }
        let local = self.zone.local(self.now);
        let today = local.date_naive();
        let dates = match self.view {
            CalendarViewKind::Week => week_dates(self.anchor, self.first_weekday()),
            CalendarViewKind::Day => vec![self.anchor],
            _ => Vec::new(),
        };
        if !dates.contains(&today) {
            return false;
        }
        let minute = (local.hour() * 60 + local.minute()) as i32;
        let (start, end) = visible_hour_range(self.scroll_minutes, 600, 0);
        start <= minute && minute <= end
    }

    fn reconcile_now_timer(&mut self, cx: &mut Context<Self>) {
        self.now_task = None;
        if !self.live_now || !self.now_line_visible() {
            return;
        }
        let delay = std::time::Duration::from_secs(60 - u64::from(Utc::now().second()));
        self.now_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                this.now = Utc::now();
                cx.notify();
                this.reconcile_now_timer(cx);
            });
        }));
    }
}

fn local_instant(zone: CalendarZone, date: NaiveDate, minute: i32) -> Option<DateTime<Utc>> {
    let hour = u32::try_from(minute.div_euclid(60)).ok()?;
    let minute = u32::try_from(minute.rem_euclid(60)).ok()?;
    let naive = date.and_hms_opt(hour, minute, 0)?;
    zone.single_from_local(naive)
}
