//! The picker control behind [`DatePicker`], [`DateRangePicker`], [`TimePicker`],
//! [`DateTimePicker`] and [`DateTimeRangePicker`].
//!
//! Built from Kit parts: Kit's [`CalendarState`] (selection, month/year views, disabled
//! matcher) rendered through the unstyled `gpui_kit::base::Calendar`, Kit's segmented
//! `TimeFieldState`/`base::TimeField` for times, Kit `Button`s for presets, and Kit's
//! positioning via GPUI `anchored`. GUIraffe owns the trigger, the popover, the theme
//! tokens and API conventions §12 behaviour, which Kit's `DatePicker` facade cannot be configured to
//! follow (see the crate docs for the gaps).

use std::{cell::Cell, rc::Rc, sync::Arc};

use chrono::{Datelike, NaiveDate, NaiveTime, Weekday};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    FontFeatures, FontWeight, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, Subscription,
    TestSupportExt, Window, actions, anchored,
    assets::IconName,
    base::{
        Calendar as BaseCalendar, CalendarEvent, CalendarItemKind, CalendarState, CalendarView,
        Date, HourCycle, Matcher, TimeField as BaseTimeField, TimeFieldEvent, TimeFieldState,
        TimePrecision, TimeSegment,
    },
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
    },
    deferred, div, point,
    prelude::*,
    px,
};
use guiraffe_components::close_button;
use guiraffe_core::{
    A11yDisabledExt, ColorName, GuiraffeTheme, Rgb, Size, StateKind, StyleContext, Variant,
    focus_ring, kit_size, ring_visible,
};

use super::{
    locale::{DateLocale, HourFormat, format_pattern},
    logic::{
        CursorMove, DatePredicate, DatePreset, DateRange, DateRules, DateTimeRange, Parts,
        PickerMode, PickerValue, clamp_time, move_cursor, preview_range, visible_days,
    },
    wrapper::{InputVariant, InputWrapper, frame_colors},
};

/// Pick one date.
pub type DatePicker = Picker<NaiveDate>;
/// Pick an inclusive range of dates, optionally from presets.
pub type DateRangePicker = Picker<DateRange>;
/// Edit a time of day (segmented hour/minute/second/AM-PM field).
pub type TimePicker = Picker<NaiveTime>;
/// Pick a date and a time of day.
pub type DateTimePicker = Picker<chrono::NaiveDateTime>;
/// Pick a range with a start and an end time.
pub type DateTimeRangePicker = Picker<DateTimeRange>;

actions!(
    guiraffe_dates,
    [
        Confirm, Toggle, MoveLeft, MoveRight, MoveUp, MoveDown, PrevMonth, NextMonth, PrevYear,
        NextYear, WeekStart, WeekEnd, Dismiss, Clear
    ]
);

/// Key context of a picker trigger (and its popover).
pub const PICKER_CONTEXT: &str = "GrDatePicker";

/// Register the picker key bindings (idempotent).
pub fn init(cx: &mut App) {
    let c = Some(PICKER_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("space", Toggle, c),
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("pageup", PrevMonth, c),
        KeyBinding::new("pagedown", NextMonth, c),
        KeyBinding::new("shift-pageup", PrevYear, c),
        KeyBinding::new("shift-pagedown", NextYear, c),
        KeyBinding::new("home", WeekStart, c),
        KeyBinding::new("end", WeekEnd, c),
        KeyBinding::new("escape", Dismiss, c),
        KeyBinding::new("backspace", Clear, c),
        KeyBinding::new("delete", Clear, c),
    ]);
}

/// What a picker reports.
#[derive(Debug, Clone, PartialEq)]
pub enum PickerEvent<V> {
    /// The user picked, cleared or edited the value. In controlled mode this is a request:
    /// the picker shows the new value only once the owner calls [`Picker::set_value`].
    Change(Option<V>),
    /// The popover opened or closed (or, with a controlled `open`, asks to).
    OpenChange(bool),
}

type ChangeFn<V> = Rc<dyn Fn(&Option<V>, &mut Window, &mut App)>;
type OpenFn = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// Mantine-style props of every picker. Unset visual props come from the theme and the
/// inherited `StyleContext`.
#[derive(Clone)]
pub struct PickerProps<V: PickerValue> {
    /// `Some`: controlled — the owner holds the value and updates it on
    /// [`PickerEvent::Change`] with [`Picker::set_value`]. Never together with
    /// `default_value`.
    pub value: Option<Option<V>>,
    /// The initial value of an uncontrolled picker.
    pub default_value: Option<V>,
    /// `Some`: the popover's open state is controlled (see [`PickerEvent::OpenChange`]).
    pub open: Option<bool>,
    pub placeholder: Option<SharedString>,
    pub label: Option<SharedString>,
    pub description: Option<SharedString>,
    pub error: Option<SharedString>,
    pub required: bool,
    pub min_date: Option<NaiveDate>,
    pub max_date: Option<NaiveDate>,
    /// Dates that cannot be picked (`true` = excluded), e.g. weekends or holidays.
    pub exclude_date: Option<DatePredicate>,
    pub min_time: Option<NaiveTime>,
    pub max_time: Option<NaiveTime>,
    /// The time a date gets before the user edits it (date-time pickers). Default `00:00`.
    pub default_time: NaiveTime,
    /// `None`: the locale's.
    pub first_day_of_week: Option<Weekday>,
    pub locale: DateLocale,
    /// `None`: the locale's clock.
    pub hour_format: Option<HourFormat>,
    pub with_seconds: bool,
    /// Display pattern of the value ([`format_pattern`] tokens); `None`: the locale's.
    pub value_format: Option<SharedString>,
    pub clearable: bool,
    pub presets: Vec<DatePreset>,
    /// Months shown side by side (range pickers like 2).
    pub number_of_months: usize,
    pub size: Option<Size>,
    pub variant: InputVariant,
    pub radius: Option<Size>,
    /// Selected-day color; `None`: the theme primary.
    pub color: Option<ColorName>,
    pub disabled: bool,
    pub read_only: bool,
    /// Tab stop outside a focus scope (`None`: only inside `focus_scope`, API conventions §12.3).
    pub focusable: Option<bool>,
    /// The "today" the calendar marks and presets resolve from; `None`: the local clock.
    pub today: Option<NaiveDate>,
    on_change: Option<ChangeFn<V>>,
    on_open_change: Option<OpenFn>,
}

impl<V: PickerValue> Default for PickerProps<V> {
    fn default() -> Self {
        Self {
            value: None,
            default_value: None,
            open: None,
            placeholder: None,
            label: None,
            description: None,
            error: None,
            required: false,
            min_date: None,
            max_date: None,
            exclude_date: None,
            min_time: None,
            max_time: None,
            default_time: NaiveTime::MIN,
            first_day_of_week: None,
            locale: DateLocale::default(),
            hour_format: None,
            with_seconds: false,
            value_format: None,
            clearable: false,
            presets: Vec::new(),
            number_of_months: 1,
            size: None,
            variant: InputVariant::Default,
            radius: None,
            color: None,
            disabled: false,
            read_only: false,
            focusable: None,
            today: None,
            on_change: None,
            on_open_change: None,
        }
    }
}

macro_rules! setters {
    ($($name:ident : $t:ty => $field:ident = $wrap:expr),* $(,)?) => {
        $(
            pub fn $name(mut self, v: $t) -> Self {
                self.$field = $wrap(v);
                self
            }
        )*
    };
}

fn some<T>(v: T) -> Option<T> {
    Some(v)
}
fn same<T>(v: T) -> T {
    v
}

impl<V: PickerValue> PickerProps<V> {
    pub fn new() -> Self {
        Self::default()
    }
    setters!(
        default_value: V => default_value = some,
        open: bool => open = some,
        required: bool => required = same,
        min_date: NaiveDate => min_date = some,
        max_date: NaiveDate => max_date = some,
        min_time: NaiveTime => min_time = some,
        max_time: NaiveTime => max_time = some,
        default_time: NaiveTime => default_time = same,
        first_day_of_week: Weekday => first_day_of_week = some,
        locale: DateLocale => locale = same,
        hour_format: HourFormat => hour_format = some,
        with_seconds: bool => with_seconds = same,
        clearable: bool => clearable = same,
        presets: Vec<DatePreset> => presets = same,
        number_of_months: usize => number_of_months = same,
        size: Size => size = some,
        variant: InputVariant => variant = same,
        radius: Size => radius = some,
        color: ColorName => color = some,
        disabled: bool => disabled = same,
        read_only: bool => read_only = same,
        focusable: bool => focusable = some,
        today: NaiveDate => today = some,
    );
    /// Controlled value (pair with [`Self::on_change`] or a [`PickerEvent`] subscription).
    pub fn value(mut self, v: Option<V>) -> Self {
        self.value = Some(v);
        self
    }
    pub fn placeholder(mut self, v: impl Into<SharedString>) -> Self {
        self.placeholder = Some(v.into());
        self
    }
    pub fn label(mut self, v: impl Into<SharedString>) -> Self {
        self.label = Some(v.into());
        self
    }
    pub fn description(mut self, v: impl Into<SharedString>) -> Self {
        self.description = Some(v.into());
        self
    }
    pub fn error(mut self, v: impl Into<SharedString>) -> Self {
        self.error = Some(v.into());
        self
    }
    pub fn value_format(mut self, v: impl Into<SharedString>) -> Self {
        self.value_format = Some(v.into());
        self
    }
    /// Exclude dates for which `f` returns `true`.
    pub fn exclude_date(mut self, f: impl Fn(NaiveDate) -> bool + Send + Sync + 'static) -> Self {
        self.exclude_date = Some(Arc::new(f));
        self
    }
    pub fn on_change(mut self, f: impl Fn(&Option<V>, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
    pub fn on_open_change(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    fn rules(&self) -> DateRules {
        DateRules::new(self.min_date, self.max_date).with_exclude(self.exclude_date.clone())
    }
    fn first_day(&self) -> Weekday {
        self.first_day_of_week
            .unwrap_or(self.locale.first_day_of_week)
    }
    fn hours(&self) -> HourFormat {
        self.hour_format.unwrap_or(self.locale.hour_format)
    }
}

/// A date and/or time picker (one control, five value types; see the type aliases).
///
/// Interaction (API conventions §12): the trigger is a combo box (AccessKit `ComboBox`, expanded
/// state, the value as its value). A pointer press opens the popover without taking
/// keyboard focus; the trigger is a tab stop only inside a `focus_scope` or with
/// `focusable(true)`. Focused: Enter, Space or ↓ open; arrows move the day cursor, PageUp/
/// PageDown change month (with Shift: year), Home/End go to the week's ends, Enter/Space
/// pick, Escape closes, Backspace/Delete clear (when clearable). Time fields are text-like
/// and take focus on click. Disabled pickers ignore everything; read-only ones show the
/// value but do not open.
///
/// ```no_run
/// use guiraffe_inputs::dates::{DatePicker, DatePreset, DateRangePicker};
/// # fn demo(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
/// let due = DatePicker::new(window, cx, |p| {
///     p.label("Due date").placeholder("Pick a date").clearable(true)
/// });
/// let period = DateRangePicker::new(window, cx, |p| {
///     let presets = DatePreset::standard_ranges(&p.locale);
///     p.label("Period").presets(presets).number_of_months(2)
/// });
/// # let _ = (due, period);
/// # }
/// ```
pub struct Picker<V: PickerValue> {
    props: PickerProps<V>,
    rules: DateRules,
    value: Option<V>,
    /// The selection being edited in the popover.
    pending: Parts,
    open: bool,
    focus: FocusHandle,
    calendar: Entity<CalendarState>,
    start_time: Entity<TimeFieldState>,
    end_time: Entity<TimeFieldState>,
    cursor: NaiveDate,
    hover: Option<NaiveDate>,
    today: NaiveDate,
    /// What the trigger shows (and announces): changes with the value, not every render.
    display: Option<SharedString>,
    preset_labels: Vec<SharedString>,
    /// What the popover last rendered of the calendar, so its notifications only repaint
    /// the picker when something visible changed.
    seen: (i32, u8, CalendarView, Date),
    /// The popover is its own small view: it is laid out deferred (after the main tree), and
    /// as a view its reads of the calendar and time fields stay its own, so a month change
    /// repaints the popover, not the page around the picker (gpui-fast).
    popover: Entity<PopoverView<V>>,
    /// The inherited style context of the last render that saw one (see [`Self::context`]).
    ctx: StyleContext,
    _subs: Vec<Subscription>,
}

impl<V: PickerValue> EventEmitter<PickerEvent<V>> for Picker<V> {}

impl<V: PickerValue> Focusable for Picker<V> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if V::MODE == PickerMode::Time {
            self.start_time.focus_handle(cx)
        } else {
            self.focus.clone()
        }
    }
}

fn hour_cycle(f: HourFormat) -> HourCycle {
    match f {
        HourFormat::H12 => HourCycle::H12,
        HourFormat::H24 => HourCycle::H23,
    }
}

fn calendar_date(mode: PickerMode, p: &Parts) -> Date {
    if mode.is_range() {
        Date::Range(p.start, p.end)
    } else {
        Date::Single(p.start)
    }
}

impl<V: PickerValue> Picker<V> {
    /// Create a picker configured by `configure` (Mantine-style builder on the props).
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        configure: impl FnOnce(PickerProps<V>) -> PickerProps<V>,
    ) -> Entity<Self> {
        Self::with_props(configure(PickerProps::new()), window, cx)
    }

    pub fn with_props(props: PickerProps<V>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let calendar = cx.new(|cx| CalendarState::new(window, cx));
            let start_time = cx.new(|cx| TimeFieldState::new(window, cx));
            let end_time = cx.new(|cx| TimeFieldState::new(window, cx));
            let focus = cx.focus_handle();
            let weak = cx.weak_entity();
            let subs = vec![
                cx.subscribe_in(
                    &calendar,
                    window,
                    |this: &mut Self, _, e: &CalendarEvent, window, cx| {
                        let CalendarEvent::Selected(date) = e;
                        this.on_calendar_selected(*date, window, cx);
                    },
                ),
                cx.observe(&calendar, |this, _, cx| this.on_calendar_changed(cx)),
                cx.subscribe_in(
                    &start_time,
                    window,
                    |this: &mut Self, _, e: &TimeFieldEvent, w, cx| {
                        let TimeFieldEvent::Change(t) = e;
                        this.on_time(false, *t, w, cx);
                    },
                ),
                cx.subscribe_in(
                    &end_time,
                    window,
                    |this: &mut Self, _, e: &TimeFieldEvent, w, cx| {
                        let TimeFieldEvent::Change(t) = e;
                        this.on_time(true, *t, w, cx);
                    },
                ),
                cx.on_blur(
                    &start_time.focus_handle(cx),
                    window,
                    |this: &mut Self, w, cx| {
                        this.sync_children(w, cx);
                    },
                ),
                cx.on_blur(
                    &end_time.focus_handle(cx),
                    window,
                    |this: &mut Self, w, cx| {
                        this.sync_children(w, cx);
                    },
                ),
                cx.on_focus_out(&focus, window, |this: &mut Self, _, window, cx| {
                    if this.open {
                        this.request_open(false, window, cx);
                    }
                }),
            ];
            let today = props
                .today
                .unwrap_or_else(|| chrono::Local::now().date_naive());
            let mut this = Self {
                props: PickerProps::default(),
                rules: DateRules::default(),
                value: None,
                pending: Parts::default(),
                open: false,
                focus,
                calendar,
                start_time,
                end_time,
                cursor: today,
                hover: None,
                today,
                display: None,
                preset_labels: Vec::new(),
                seen: (0, 0, CalendarView::Day, Date::Single(None)),
                ctx: StyleContext::default(),
                popover: cx.new(|_| PopoverView {
                    picker: weak.clone(),
                }),
                _subs: subs,
            };
            debug_assert!(
                props.value.is_none() || props.default_value.is_none(),
                "a picker is controlled (value) or uncontrolled (default_value), never both"
            );
            let initial = props.value.unwrap_or(props.default_value);
            this.open = props.open.unwrap_or(false);
            this.apply_props(props, window, cx);
            this.store_value(initial, window, cx);
            if this.open {
                this.prepare_open(cx);
            }
            this
        })
    }

    /// The current value (the committed one; in controlled mode, the owner's).
    pub fn value(&self) -> Option<V> {
        self.value
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn props(&self) -> &PickerProps<V> {
        &self.props
    }

    /// Set the value without emitting (the owner's update in controlled mode, or a reset).
    /// Values outside the rules are moved onto the nearest pickable dates.
    pub fn set_value(&mut self, value: Option<V>, window: &mut Window, cx: &mut Context<Self>) {
        if self.props.value.is_some() {
            self.props.value = Some(value);
        }
        self.store_value(value, window, cx);
        cx.notify();
    }

    /// Change props after creation (knobs, form state). Re-resolves rules and labels.
    pub fn update_props(
        &mut self,
        f: impl FnOnce(&mut PickerProps<V>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut props = self.props.clone();
        f(&mut props);
        let open = props.open;
        self.apply_props(props, window, cx);
        if let Some(open) = open
            && open != self.open
        {
            self.apply_open(open, window, cx);
        }
        let value = self.value;
        self.store_value(value, window, cx);
        cx.notify();
    }

    pub fn set_disabled(&mut self, v: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.update_props(|p| p.disabled = v, window, cx);
    }

    pub fn set_error(
        &mut self,
        v: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_props(|p| p.error = v, window, cx);
    }

    /// Open or close the popover (the owner's update when `open` is controlled).
    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.props.open.is_some() {
            self.props.open = Some(open);
        }
        if open != self.open {
            self.apply_open(open, window, cx);
        }
    }

    #[doc(hidden)]
    pub fn calendar_for_test(&self) -> Entity<CalendarState> {
        self.calendar.clone()
    }
    #[doc(hidden)]
    pub fn time_fields_for_test(&self) -> (Entity<TimeFieldState>, Entity<TimeFieldState>) {
        (self.start_time.clone(), self.end_time.clone())
    }
    /// The keyboard cursor day in the open calendar.
    pub fn cursor(&self) -> NaiveDate {
        self.cursor
    }

    fn controlled(&self) -> bool {
        self.props.value.is_some()
    }
    fn inert(&self) -> bool {
        self.props.disabled || self.props.read_only
    }

    fn apply_props(&mut self, props: PickerProps<V>, window: &mut Window, cx: &mut Context<Self>) {
        self.rules = props.rules();
        if let Some(today) = props.today {
            self.today = today;
        }
        self.preset_labels = props
            .presets
            .iter()
            .map(|p| SharedString::from(p.label.clone()))
            .collect();
        let rules = self.rules.clone();
        let matcher = Matcher::custom(move |d: &NaiveDate| !rules.allows(*d));
        let years = (
            props.min_date.map_or(self.today.year() - 100, |d| d.year()),
            props.max_date.map_or(self.today.year() + 100, |d| d.year()) + 1,
        );
        self.calendar.update(cx, |s, cx| {
            s.set_disabled_matcher_shared(Some(Rc::new(matcher)));
            s.set_year_range(years, cx);
        });
        let precision = if props.with_seconds {
            TimePrecision::Second
        } else {
            TimePrecision::Minute
        };
        let cycle = hour_cycle(props.hours());
        for field in [&self.start_time, &self.end_time] {
            field.update(cx, |f, cx| {
                f.set_precision(precision, window, cx);
                f.set_hour_cycle(cycle, window, cx);
            });
        }
        self.props = props;
    }

    /// Store `value` (normalized against the rules) and sync the popover parts.
    fn store_value(&mut self, value: Option<V>, window: &mut Window, cx: &mut Context<Self>) {
        let value = value.and_then(|v| v.normalized(&self.rules));
        self.value = value;
        self.pending = value.map(V::to_parts).unwrap_or_default();
        self.sync_children(window, cx);
        self.refresh_display(cx);
    }

    /// Push the pending parts into Kit's calendar and time fields.
    fn sync_children(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let date = calendar_date(V::MODE, &self.pending);
        self.calendar.update(cx, |s, cx| {
            if s.date() != date && s.apply_date(date) {
                cx.notify();
            }
        });
        let default = self.props.default_time;
        let (a, b) = (
            self.pending.start_time.unwrap_or(default),
            self.pending.end_time.unwrap_or(default),
        );
        self.start_time
            .update(cx, |f, cx| f.set_time(a, window, cx));
        self.end_time.update(cx, |f, cx| f.set_time(b, window, cx));
    }

    fn format_parts(&self, date: Option<NaiveDate>, time: Option<NaiveTime>) -> String {
        let l = &self.props.locale;
        let time_fmt = l.time_format(self.props.hours(), self.props.with_seconds);
        let date_fmt = self.props.value_format.as_deref().unwrap_or(l.date_format);
        match (V::MODE, self.props.value_format.is_some()) {
            (PickerMode::Time, false) => format_pattern(time_fmt, None, time, l),
            (PickerMode::DateTime | PickerMode::DateTimeRange, false) => {
                format!(
                    "{} {}",
                    format_pattern(date_fmt, date, None, l),
                    format_pattern(time_fmt, None, time, l)
                )
            }
            _ => format_pattern(date_fmt, date, time, l),
        }
    }

    fn refresh_display(&mut self, cx: &App) {
        let l = &self.props.locale;
        let partial = self.open && V::MODE.is_range();
        let cal = self.calendar.read(cx).date();
        let text = match (self.value, cal) {
            (_, Date::Range(Some(a), None)) if partial => Some(format!(
                "{}{}…",
                self.format_parts(Some(a), self.pending.start_time),
                l.range_separator
            )),
            (Some(v), _) => {
                let p = v.to_parts();
                let start = self.format_parts(p.start, p.start_time);
                Some(if V::MODE.is_range() {
                    format!(
                        "{start}{}{}",
                        l.range_separator,
                        self.format_parts(p.end, p.end_time)
                    )
                } else {
                    start
                })
            }
            (None, _) => None,
        };
        self.display = text.map(SharedString::from);
    }

    fn emit_change(&mut self, next: Option<V>, window: &mut Window, cx: &mut Context<Self>) {
        if next == self.value {
            return;
        }
        if !self.controlled() {
            self.value = next;
        }
        self.refresh_display(cx);
        cx.emit(PickerEvent::Change(next));
        if let Some(f) = self.props.on_change.clone() {
            f(&next, window, cx);
        }
        cx.notify();
    }

    /// Commit the pending parts if they make a complete value.
    fn commit_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = V::from_parts(&self.pending, self.props.default_time) {
            self.emit_change(Some(next), window, cx);
        }
    }

    /// Clear the value (the clear button, Backspace/Delete).
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inert() {
            return;
        }
        self.pending = Parts::default();
        self.sync_children(window, cx);
        self.emit_change(None, window, cx);
        self.refresh_display(cx);
        cx.notify();
    }

    /// Ask to open/close: emits [`PickerEvent::OpenChange`]; applies it unless `open` is
    /// controlled.
    fn request_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open && (self.inert() || !V::MODE.has_calendar()) {
            return;
        }
        if open == self.open {
            return;
        }
        cx.emit(PickerEvent::OpenChange(open));
        if let Some(f) = self.props.on_open_change.clone() {
            f(&open, window, cx);
        }
        if self.props.open.is_none() {
            self.apply_open(open, window, cx);
        }
    }

    fn apply_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open = open;
        self.hover = None;
        if open {
            // Start from the committed value; an abandoned partial range is dropped.
            self.pending = self.value.map(V::to_parts).unwrap_or_default();
            self.sync_children(window, cx);
            self.prepare_open(cx);
        } else {
            // Focus inside the popover (a time field, a preset) goes back to the trigger.
            if self.focus.contains_focused(window, cx) && !self.focus.is_focused(window) {
                window.focus(&self.focus, cx);
            }
            self.pending = self.value.map(V::to_parts).unwrap_or_default();
            self.sync_children(window, cx);
        }
        self.refresh_display(cx);
        cx.notify();
    }

    /// Place the cursor and the visible month for a fresh popover.
    fn prepare_open(&mut self, cx: &mut Context<Self>) {
        let start = self.pending.start.unwrap_or(self.today);
        self.cursor = self.rules.snap(start).unwrap_or(start);
        self.show_month_of(self.cursor, true, cx);
    }

    /// Scroll the calendar so `d` is visible (`reset`: make its month the first one).
    fn show_month_of(&mut self, d: NaiveDate, reset: bool, cx: &mut Context<Self>) {
        let n = self.props.number_of_months.max(1) as i64;
        self.calendar.update(cx, |s, cx| {
            let first = i64::from(s.current_year()) * 12 + i64::from(s.current_month()) - 1;
            let want = i64::from(d.year()) * 12 + i64::from(d.month0());
            let target = if reset || want < first {
                want
            } else if want >= first + n {
                want - n + 1
            } else {
                return;
            };
            let (y, m) = (
                target.div_euclid(12) as i32,
                target.rem_euclid(12) as u8 + 1,
            );
            s.set_view(CalendarView::Day);
            s.select_year(y);
            s.select_month(m);
            cx.notify();
        });
    }

    fn on_calendar_selected(&mut self, date: Date, window: &mut Window, cx: &mut Context<Self>) {
        let (start, end) = (date.start(), date.end());
        let repeat = V::MODE == PickerMode::DateTime && start == self.pending.start;
        self.pending.start = start;
        self.pending.end = if V::MODE.is_range() { end } else { None };
        if let Some(d) = end.or(start) {
            self.cursor = d;
        }
        self.hover = None;
        self.commit_pending(window, cx);
        self.refresh_display(cx);
        // Date pickers close on a pick; date-time pickers stay open for the time, and close
        // when the picked day is picked again (like Kit's).
        if !V::MODE.has_time() || repeat {
            self.request_open(false, window, cx);
        }
        cx.notify();
    }

    fn on_calendar_changed(&mut self, cx: &mut Context<Self>) {
        // Kit's prev/next buttons ignore min/max: keep the visible months in bounds.
        let n = self.props.number_of_months.max(1) as u32;
        let (y, m) = {
            let s = self.calendar.read(cx);
            (s.current_year(), u32::from(s.current_month()))
        };
        let in_bounds = (0..n).any(|i| {
            let k = y * 12 + (m - 1 + i) as i32;
            self.rules
                .month_in_bounds(k.div_euclid(12), k.rem_euclid(12) as u32 + 1)
        });
        if !in_bounds && let Some(edge) = self.rules.min.or(self.rules.max) {
            let edge = if self
                .rules
                .min
                .is_some_and(|min| (y, m) < (min.year(), min.month()))
            {
                self.rules.min.unwrap_or(edge)
            } else {
                self.rules.max.unwrap_or(edge)
            };
            self.show_month_of(edge, true, cx);
        }
        let s = self.calendar.read(cx);
        let seen = (s.current_year(), s.current_month(), s.view(), s.date());
        if seen != self.seen {
            self.seen = seen;
            self.refresh_display(cx);
            cx.notify();
        }
    }

    fn on_time(&mut self, end: bool, t: NaiveTime, window: &mut Window, cx: &mut Context<Self>) {
        let clamped = clamp_time(t, self.props.min_time, self.props.max_time);
        if end {
            self.pending.end_time = Some(clamped);
        } else {
            self.pending.start_time = Some(clamped);
        }
        // The field keeps what is typed (clamping each digit would fight the two-digit
        // buffer); the clamped time is pushed back when it loses focus.
        self.commit_pending(window, cx);
        cx.notify();
    }

    /// Pick preset `i` (popover button).
    pub fn apply_preset(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = self.props.presets.get(i).cloned() else {
            return;
        };
        if self.inert() {
            return;
        }
        match preset.apply(self.today, self.props.first_day(), &self.rules) {
            Err(()) => {}
            Ok(None) => {
                // Custom: start over and leave the calendar open for a manual pick.
                self.pending.start = None;
                self.pending.end = None;
                self.sync_children(window, cx);
                self.cursor = self.rules.snap(self.today).unwrap_or(self.today);
                self.show_month_of(self.cursor, true, cx);
                self.refresh_display(cx);
                cx.notify();
            }
            Ok(Some(r)) => {
                self.pending.start = Some(r.start);
                self.pending.end = V::MODE.is_range().then_some(r.end);
                self.cursor = r.end;
                self.sync_children(window, cx);
                self.show_month_of(r.start, true, cx);
                self.commit_pending(window, cx);
                if !V::MODE.has_time() {
                    self.request_open(false, window, cx);
                }
                self.refresh_display(cx);
                cx.notify();
            }
        }
    }

    fn preset_active(&self, preset: &DatePreset) -> bool {
        let Ok(Some(r)) = preset.apply(self.today, self.props.first_day(), &self.rules) else {
            return false;
        };
        match (self.pending.start, self.pending.end) {
            (Some(s), Some(e)) if V::MODE.is_range() => s == r.start && e == r.end,
            (Some(s), _) if !V::MODE.is_range() => s == r.start && r.start == r.end,
            _ => false,
        }
    }

    // ---------------------------------------------------------------- keyboard

    fn trigger_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    fn time_focused(&self, window: &Window, cx: &App) -> bool {
        self.start_time.focus_handle(cx).is_focused(window)
            || self.end_time.focus_handle(cx).is_focused(window)
    }

    fn activate_cursor(&mut self, cx: &mut Context<Self>) {
        let d = self.cursor;
        if self.rules.allows(d) {
            self.calendar.update(cx, |s, cx| {
                s.activate_date(d, cx);
            });
            // Kit emits `Selected` only for complete values; a range start shows at once.
            self.refresh_display(cx);
            cx.notify();
        }
    }

    fn on_confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.inert() {
            return cx.propagate();
        }
        if !self.open {
            self.request_open(true, window, cx);
        } else if self.trigger_focused(window) {
            self.activate_cursor(cx);
        } else if self.time_focused(window, cx) {
            self.request_open(false, window, cx);
        } else {
            cx.propagate();
        }
    }

    fn on_toggle(&mut self, _: &Toggle, window: &mut Window, cx: &mut Context<Self>) {
        if self.inert() || !self.trigger_focused(window) {
            return cx.propagate();
        }
        if self.open {
            self.activate_cursor(cx);
        } else {
            self.request_open(true, window, cx);
        }
    }

    fn move_by(&mut self, mv: CursorMove, cx: &mut Context<Self>) {
        self.cursor = move_cursor(self.cursor, mv, &self.rules, self.props.first_day());
        self.show_month_of(self.cursor, false, cx);
        if V::MODE.is_range() {
            self.hover = Some(self.cursor);
        }
        cx.notify();
    }

    fn nav(&mut self, mv: CursorMove, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.inert() || !self.trigger_focused(window) {
            return cx.propagate();
        }
        self.move_by(mv, cx);
    }

    fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.request_open(false, window, cx);
        } else {
            cx.propagate();
        }
    }

    fn on_clear(&mut self, _: &Clear, window: &mut Window, cx: &mut Context<Self>) {
        if self.props.clearable && !self.inert() && self.trigger_focused(window) {
            self.clear(window, cx);
        } else {
            cx.propagate();
        }
    }
}

// -------------------------------------------------------------------- rendering

/// Theme-resolved colors for one popover render.
#[derive(Clone, Copy)]
struct Palette {
    text: Rgb,
    dimmed: Rgb,
    hover: Rgb,
    selected: (Rgb, Rgb),
    in_range: (Rgb, Rgb),
    disabled: Rgb,
    ring: Rgb,
}

impl Palette {
    fn new(t: &GuiraffeTheme, color: Option<ColorName>) -> Self {
        let s = t.surfaces();
        let sel = t.state(StateKind::Selected, Variant::Filled, color);
        let light = t.variant(color, Variant::Light);
        let hover = t.press_colors(Variant::Subtle, Some(ColorName::Gray)).hover;
        Self {
            text: s.text,
            dimmed: t
                .state(StateKind::Dimmed, Variant::Default, None)
                .foreground,
            hover: hover.background,
            selected: (sel.background, sel.foreground),
            in_range: (light.background.over(s.body), light.foreground),
            disabled: t
                .state(StateKind::Disabled, Variant::Default, None)
                .foreground,
            ring: t.state(StateKind::FocusRing, Variant::Filled, color).border,
        }
    }
}

/// Everything one calendar render needs to style its items.
struct CalendarLook {
    palette: Palette,
    cell: f32,
    font: f32,
    radius: f32,
    days: Vec<NaiveDate>,
    counter: Cell<usize>,
    cursor: Option<NaiveDate>,
    today: NaiveDate,
    preview: Option<DateRange>,
    prev_disabled: bool,
    next_disabled: bool,
    locale: DateLocale,
}

fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

impl<V: PickerValue> Picker<V> {
    /// Resolve the inherited `StyleContext` for this render.
    ///
    /// gpui-fast re-renders a notified view on its own, outside its ancestors' layout, so
    /// the context providers (`focus_scope`, `provide`) are not on the stack and
    /// `StyleContext::current()` reads the default. A default read therefore keeps the
    /// context of the last render that ran inside the tree; a picker moved from a
    /// non-default context into a default one is the only case this gets wrong.
    fn context(&mut self) {
        let now = StyleContext::current();
        if now != StyleContext::default() {
            self.ctx = now;
        }
    }

    fn size(&self) -> Size {
        self.props.size.unwrap_or(self.ctx.control_size)
    }

    fn radius(&self, cx: &App) -> f32 {
        self.props
            .radius
            .unwrap_or(GuiraffeTheme::global(cx).default_radius)
            .radius()
    }

    fn placeholder(&self) -> SharedString {
        self.props.placeholder.clone().unwrap_or_else(|| {
            let l = &self.props.locale.labels;
            match V::MODE {
                PickerMode::DateRange | PickerMode::DateTimeRange => l.range_placeholder,
                PickerMode::Time => l.time_placeholder,
                _ => l.date_placeholder,
            }
            .into()
        })
    }

    fn accessible_name(&self) -> SharedString {
        self.props
            .label
            .clone()
            .unwrap_or_else(|| self.placeholder())
    }

    fn render_calendar(&self, this: &Entity<Self>, window: &Window, cx: &App) -> AnyElement {
        let size = self.size();
        let t = GuiraffeTheme::global(cx);
        let n = self.props.number_of_months.max(1);
        let first_day = self.props.first_day();
        let (y, m) = {
            let s = self.calendar.read(cx);
            (s.current_year(), u32::from(s.current_month()))
        };
        let month_ok = |k: i32| {
            self.rules
                .month_in_bounds(k.div_euclid(12), k.rem_euclid(12) as u32 + 1)
        };
        let k = y * 12 + m as i32 - 1;
        let preview = match (self.calendar.read(cx).date(), self.hover) {
            (Date::Range(Some(a), None), Some(h)) => preview_range(a, h),
            _ => None,
        };
        let keyboard = ring_visible(&self.focus, window);
        let look = Rc::new(CalendarLook {
            palette: Palette::new(t, self.props.color),
            cell: size.control_height(),
            font: size.font_size() * t.scale,
            radius: self.radius(cx),
            days: visible_days(y, m, n, first_day),
            counter: Cell::new(0),
            cursor: keyboard.then_some(self.cursor),
            today: self.today,
            preview,
            prev_disabled: !month_ok(k - 1),
            next_disabled: !month_ok(k + n as i32),
            locale: self.props.locale.clone(),
        });
        let labels = look.clone();
        let weak = this.downgrade();
        let range = V::MODE.is_range();
        BaseCalendar::new(("gr-calendar", this.entity_id()), &self.calendar)
            .number_of_months(n)
            .first_day_of_week(first_day)
            .gap(px(2.))
            .label(move |kind, value| {
                let l = &labels.locale;
                match kind {
                    CalendarItemKind::Previous => l.labels.previous_month.into(),
                    CalendarItemKind::Next => l.labels.next_month.into(),
                    CalendarItemKind::MonthToggle => l.month(value as u32).into(),
                    CalendarItemKind::Month => l
                        .months_short
                        .get((value as usize).wrapping_sub(1))
                        .copied()
                        .unwrap_or("")
                        .into(),
                    CalendarItemKind::Weekday => l.weekday_short(value as u32).into(),
                    _ => value.to_string().into(),
                }
            })
            .item(move |item, st, _, _| {
                let l = &look;
                let p = l.palette;
                let h = Rgb::to_hsla;
                let r = px(l.radius);
                let base = item
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(r)
                    .border_1()
                    .border_color(h(Rgb::TRANSPARENT))
                    .text_size(px(l.font))
                    .text_color(h(p.text));
                let pressable = |el: gpui_kit::base::CalendarItem, disabled: bool| {
                    if disabled {
                        el.text_color(h(p.disabled)).cursor_not_allowed()
                    } else {
                        el.cursor_pointer().hover(move |s| s.bg(h(p.hover)))
                    }
                };
                match st.kind() {
                    CalendarItemKind::Day => {
                        let i = l.counter.get();
                        l.counter.set(i + 1);
                        let date = l.days.get(i).copied();
                        let disabled = st.is_disabled();
                        let selected = st.is_active();
                        let in_range = !selected
                            && (st.is_in_range()
                                || date.is_some_and(|d| l.preview.is_some_and(|r| r.contains(d))));
                        let mut el = base
                            .size(px(l.cell))
                            .font_features(tabular())
                            .role(Role::Button)
                            .aria_selected(selected);
                        if let Some(d) = date {
                            el = el.aria_label(SharedString::from(format_pattern(
                                l.locale.long_date_format,
                                Some(d),
                                None,
                                &l.locale,
                            )));
                            if d == l.today {
                                el = el.font_weight(FontWeight::BOLD);
                                if !selected {
                                    el = el.border_color(h(p.dimmed));
                                }
                            }
                        }
                        el = if selected {
                            el.bg(h(p.selected.0))
                                .text_color(h(p.selected.1))
                                .border_color(h(p.selected.0))
                                .cursor_pointer()
                        } else if disabled {
                            // Disabled is not color alone: struck through, not-allowed cursor.
                            el.line_through()
                                .text_color(h(p.disabled))
                                .cursor_not_allowed()
                        } else if in_range {
                            el.bg(h(p.in_range.0))
                                .text_color(h(p.in_range.1))
                                .cursor_pointer()
                        } else if st.is_muted() {
                            pressable(el, false).text_color(h(p.dimmed))
                        } else {
                            pressable(el, false)
                        };
                        if range && !disabled {
                            let weak = weak.clone();
                            el = el.on_hover(move |hovered, _, cx| {
                                let _ = weak.update(cx, |this: &mut Picker<V>, cx| {
                                    let next = if *hovered { date } else { None };
                                    if this.hover != next && (next.is_some() || this.hover == date)
                                    {
                                        this.hover = next;
                                        cx.notify();
                                    }
                                });
                            });
                        }
                        let ring = date.is_some() && date == l.cursor;
                        focus_ring(el, ring, r, p.ring).into_any_element()
                    }
                    CalendarItemKind::Weekday => base
                        .size(px(l.cell))
                        .text_size(px((l.font * 0.8).round()))
                        .text_color(h(p.dimmed))
                        .into_any_element(),
                    CalendarItemKind::Previous | CalendarItemKind::Next => {
                        let prev = st.kind() == CalendarItemKind::Previous;
                        let disabled = st.is_disabled()
                            || if prev {
                                l.prev_disabled
                            } else {
                                l.next_disabled
                            };
                        let icon = if prev {
                            IconName::ChevronLeft
                        } else {
                            IconName::ChevronRight
                        };
                        pressable(
                            base.clear_children()
                                .size(px((l.cell * 0.8).round()))
                                .role(Role::Button)
                                .child(icon),
                            disabled,
                        )
                        .into_any_element()
                    }
                    CalendarItemKind::MonthToggle | CalendarItemKind::YearToggle => pressable(
                        base.h(px((l.cell * 0.8).round()))
                            .px(px(l.cell / 4.0))
                            .font_weight(FontWeight::MEDIUM)
                            .role(Role::Button)
                            .aria_expanded(st.is_active()),
                        false,
                    )
                    .into_any_element(),
                    CalendarItemKind::Month | CalendarItemKind::Year => {
                        let el = base.w_full().h(px(l.cell)).role(Role::Button);
                        if st.is_active() {
                            el.bg(h(p.selected.0))
                                .text_color(h(p.selected.1))
                                .aria_selected(true)
                                .into_any_element()
                        } else {
                            pressable(el, false).into_any_element()
                        }
                    }
                }
            })
            .into_any_element()
    }

    fn render_time_field(
        &self,
        id: &'static str,
        state: &Entity<TimeFieldState>,
        empty: bool,
        compact: bool,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let t = GuiraffeTheme::global(cx);
        let size = self.size();
        let focus = state.focus_handle(cx);
        let focused = focus.is_focused(window);
        let disabled = self.props.disabled;
        let invalid = self.props.error.is_some() && !compact;
        let fc = frame_colors(t, self.props.variant, focused, invalid, disabled);
        let seg = t.variant(self.props.color, Variant::Light);
        let r = px(self.radius(cx));
        let h = Rgb::to_hsla;
        let (sel_bg, sel_fg) = (seg.background.over(fc.background), seg.foreground);
        let placeholder = fc.placeholder;
        let height = if compact {
            (size.control_height() * 0.85).round()
        } else {
            size.control_height()
        };
        let field = BaseTimeField::new(id, state)
            .disabled(disabled || self.props.read_only)
            .flex()
            .items_center()
            .h_full()
            .flex_1()
            .px(px((size.spacing() * 0.75).round() - 2.))
            .render_segment(move |segment, st, _, _| {
                let segment = segment.px(px(2.)).rounded(px(2.));
                let segment = if empty {
                    segment
                        .clear_children()
                        .child("--")
                        .text_color(h(placeholder))
                } else {
                    segment
                };
                segment
                    .when(st.segment() == TimeSegment::Period, |s| s.ml(px(4.)))
                    .when(st.is_selected(), |s| s.bg(h(sel_bg)).text_color(h(sel_fg)))
                    .into_any_element()
            });
        let frame = div()
            .h(px(height))
            .flex()
            .items_center()
            .rounded(r)
            .border_1()
            .border_color(h(fc.border))
            .bg(h(fc.background))
            .text_color(h(fc.foreground))
            .text_size(px(size.font_size() * t.scale))
            .font_features(tabular())
            .when(disabled, |el| el.cursor_not_allowed())
            .when(!disabled, |el| el.cursor_text())
            .child(field);
        let ring = t
            .state(StateKind::FocusRing, Variant::Filled, self.props.color)
            .border;
        focus_ring(frame, !disabled && ring_visible(&focus, window), r, ring).into_any_element()
    }

    fn render_popover(&self, this: &Entity<Self>, window: &Window, cx: &App) -> AnyElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let size = self.size();
        let pad = px(size.spacing());
        let gap = px((size.spacing() / 2.0).round());
        let r = px(t.default_radius.radius());
        let tab = self.props.focusable.unwrap_or(self.ctx.keyboard_nav);
        let shadows: Vec<_> = t
            .shadow(Size::Md)
            .iter()
            .map(|l| gpui_kit::BoxShadow {
                color: Rgb::BLACK.alpha(l.alpha).to_hsla(),
                offset: point(px(0.), px(l.y)),
                blur_radius: px(l.blur),
                spread_radius: px(l.spread),
                inset: false,
            })
            .collect();
        let entity = this.clone();
        let light = t.variant(self.props.color, Variant::Light);
        let (active_bg, active_fg) = (light.background.over(s.body), light.foreground);
        let presets = (!self.props.presets.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .pr(gap)
                .border_r_1()
                .border_color(s.border.to_hsla())
                .children(self.props.presets.iter().enumerate().map(|(i, p)| {
                    let allowed = p
                        .apply(self.today, self.props.first_day(), &self.rules)
                        .is_ok();
                    let active = self.preset_active(p);
                    let e = entity.clone();
                    Button::new(("gr-preset", i))
                        .ghost()
                        .with_size(kit_size(Size::Sm.min(size)))
                        .label(self.preset_labels.get(i).cloned().unwrap_or_default())
                        .w_full()
                        .selected(active)
                        // Selected is also announced (pressed), not only painted.
                        .toggled(active)
                        // Kit's ghost "selected" fill is near-invisible in dark; use the
                        // theme's light variant so the active preset reads in both schemes.
                        .when(active, |b| {
                            b.bg(active_bg.to_hsla()).text_color(active_fg.to_hsla())
                        })
                        .disabled(!allowed)
                        .tab_stop(tab && allowed)
                        .on_click(move |_, window, cx| {
                            e.update(cx, |this, cx| this.apply_preset(i, window, cx));
                        })
                        .into_any_element()
                }))
        });
        let l = &self.props.locale.labels;
        let dimmed = t
            .state(StateKind::Dimmed, Variant::Default, None)
            .foreground;
        let label = |text: &'static str| {
            div()
                .text_size(px((size.font_size() * 0.86).round()))
                .text_color(dimmed.to_hsla())
                .child(text)
        };
        let times = V::MODE.has_time().then(|| {
            let row = |title: &'static str, el: AnyElement| {
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(gap)
                    .child(label(title))
                    .child(div().w(px(size.control_height() * 3.2)).child(el))
            };
            let range = V::MODE.is_range();
            let start =
                self.render_time_field("gr-start-time", &self.start_time, false, true, window, cx);
            let e = entity.clone();
            div()
                .flex()
                .flex_col()
                .gap(gap)
                .pt(gap)
                .border_t_1()
                .border_color(s.border.to_hsla())
                .child(row(if range { l.start_time } else { l.time }, start))
                .when(range, |el| {
                    let end = self.render_time_field(
                        "gr-end-time",
                        &self.end_time,
                        false,
                        true,
                        window,
                        cx,
                    );
                    el.child(row(l.end_time, end))
                })
                .child(
                    div().flex().justify_end().child(
                        Button::new("gr-done")
                            .primary()
                            .with_size(kit_size(Size::Sm.min(size)))
                            .label(l.done)
                            .tab_stop(tab)
                            .on_click(move |_, window, cx| {
                                e.update(cx, |this, cx| this.request_open(false, window, cx));
                            }),
                    ),
                )
        });
        let surface = div()
            .id("gr-picker-popover")
            .test_support()
            .debug_selector(|| "picker-popover".into())
            .occlude()
            .mt(px(4.))
            .p(pad)
            .flex()
            .gap(gap)
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .border_1()
            .border_color(s.border.to_hsla())
            .rounded(r)
            .shadow(shadows)
            // Pointer presses inside the popover never take keyboard focus (API conventions §12.3);
            // the time fields focus themselves, like text inputs.
            .capture_any_mouse_down(|_, window, _| window.prevent_default())
            .on_mouse_down_out({
                let e = entity.clone();
                move |_, window, cx| e.update(cx, |p, cx| p.request_open(false, window, cx))
            })
            .children(presets)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(gap)
                    .child(self.render_calendar(this, window, cx))
                    .children(times),
            );
        surface.into_any_element()
    }

    fn render_trigger(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = GuiraffeTheme::global(cx);
        let size = self.size();
        let disabled = self.props.disabled;
        let inert = self.inert();
        let fc = frame_colors(
            t,
            self.props.variant,
            self.open,
            self.props.error.is_some(),
            disabled,
        );
        let h = Rgb::to_hsla;
        let r = px(self.radius(cx));
        let has_value = self.display.is_some();
        let show_clear = self.props.clearable && has_value && !inert;
        let entity = cx.entity();
        let icon = div()
            .flex_none()
            .text_size(px(size.icon_size()))
            .text_color(h(fc.placeholder))
            .child(IconName::Calendar);
        let frame = div()
            .id("gr-picker-trigger")
            .a11y_disabled(disabled)
            .test_support()
            .h(px(size.control_height()))
            .px(px((size.spacing() * 0.75).round()))
            .flex()
            .items_center()
            .gap(px((size.spacing() / 2.0).round()))
            .rounded(r)
            .border_1()
            .border_color(h(fc.border))
            .bg(h(fc.background))
            .text_size(px(size.font_size() * t.scale))
            .map(|el| match (disabled, inert) {
                (true, _) => el.cursor_not_allowed(),
                (false, true) => el.cursor_default(),
                _ => el.cursor_pointer(),
            })
            // A pointer press opens without taking keyboard focus (API conventions §12.3).
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .when(!self.open && !inert, |el| {
                el.on_click(cx.listener(|this, _, window, cx| this.request_open(true, window, cx)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .truncate()
                    .text_color(h(if has_value {
                        fc.foreground
                    } else {
                        fc.placeholder
                    }))
                    .when(V::MODE.has_time(), |el| el.font_features(tabular()))
                    .child(self.display.clone().unwrap_or_else(|| self.placeholder())),
            )
            .map(|el| {
                if show_clear {
                    el.child(
                        close_button("gr-picker-clear")
                            .label(self.props.locale.labels.clear)
                            .size(size.smaller())
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                entity.update(cx, |this, cx| this.clear(window, cx));
                            }),
                    )
                } else {
                    el.child(icon)
                }
            });
        let ring = t
            .state(StateKind::FocusRing, Variant::Filled, self.props.color)
            .border;
        let frame = focus_ring(
            frame,
            !disabled && ring_visible(&self.focus, window),
            r,
            ring,
        );
        div()
            .relative()
            .child(frame)
            .when(self.open, |el| {
                el.child(
                    div().absolute().top_full().left_0().child(
                        deferred(
                            anchored()
                                .snap_to_window_with_margin(px(8.))
                                .child(self.popover.clone()),
                        )
                        .with_priority(1),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_time_input(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let size = self.size();
        let empty = self.value.is_none();
        let field = self.render_time_field("gr-time", &self.start_time, empty, false, window, cx);
        let show_clear = self.props.clearable && !empty && !self.inert();
        let entity = cx.entity();
        div()
            .relative()
            .flex()
            .items_center()
            .child(div().flex_1().child(field))
            .when(show_clear, |el| {
                el.child(
                    div()
                        .absolute()
                        .right(px((size.spacing() / 2.0).round()))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(
                            close_button("gr-picker-clear")
                                .label(self.props.locale.labels.clear)
                                .size(size.smaller())
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    entity.update(cx, |this, cx| this.clear(window, cx));
                                }),
                        ),
                )
            })
            .into_any_element()
    }
}

trait SizeStep {
    fn smaller(self) -> Size;
}
impl SizeStep for Size {
    fn smaller(self) -> Size {
        match self {
            Size::Xs | Size::Sm => Size::Xs,
            Size::Md => Size::Sm,
            Size::Lg => Size::Md,
            Size::Xl => Size::Lg,
        }
    }
}

impl<V: PickerValue> Render for Picker<V> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.context();
        let size = self.size();
        let disabled = self.props.disabled;
        let input = if V::MODE == PickerMode::Time {
            self.render_time_input(window, cx)
        } else {
            self.render_trigger(window, cx)
        };
        let wrapped = InputWrapper::new(input)
            .size(size)
            .required(self.props.required)
            .error(self.props.error.clone())
            .when_some(self.props.label.clone(), |w, l| w.label(l))
            .when_some(self.props.description.clone(), |w, d| w.description(d));
        let root = div()
            .id(("gr-picker", cx.entity_id()))
            .a11y_disabled(disabled);
        if V::MODE == PickerMode::Time {
            return root.test_support().child(wrapped);
        }
        let tab = !disabled && self.props.focusable.unwrap_or(self.ctx.keyboard_nav);
        root.test_support()
            .role(Role::ComboBox)
            .aria_label(self.accessible_name())
            .aria_expanded(self.open)
            .when_some(self.display.clone(), |el, v| el.aria_value(v))
            .track_focus(&self.focus.clone().tab_stop(tab))
            // Pressing the label or the wrapper never takes focus either.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .key_context(PICKER_CONTEXT)
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_toggle))
            .on_action(
                cx.listener(|this, _: &MoveLeft, w, cx| this.nav(CursorMove::Days(-1), w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveRight, w, cx| this.nav(CursorMove::Days(1), w, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveUp, w, cx| this.nav(CursorMove::Days(-7), w, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, w, cx| {
                if !this.open && !this.inert() && this.trigger_focused(w) {
                    this.request_open(true, w, cx);
                } else {
                    this.nav(CursorMove::Days(7), w, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &PrevMonth, w, cx| this.nav(CursorMove::Months(-1), w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &NextMonth, w, cx| this.nav(CursorMove::Months(1), w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PrevYear, w, cx| this.nav(CursorMove::Years(-1), w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &NextYear, w, cx| this.nav(CursorMove::Years(1), w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &WeekStart, w, cx| this.nav(CursorMove::WeekStart, w, cx)),
            )
            .on_action(cx.listener(|this, _: &WeekEnd, w, cx| this.nav(CursorMove::WeekEnd, w, cx)))
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::on_clear))
            .child(wrapped)
    }
}

/// The popover surface of a [`Picker`] (see `Picker::popover`).
struct PopoverView<V: PickerValue> {
    picker: gpui_kit::WeakEntity<Picker<V>>,
}

impl<V: PickerValue> Render for PopoverView<V> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(picker) = self.picker.upgrade() else {
            return div().into_any_element();
        };
        picker.read(cx).render_popover(&picker, window, cx)
    }
}
