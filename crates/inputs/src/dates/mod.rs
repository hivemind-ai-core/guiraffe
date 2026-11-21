//! Date and time pickers: [`DatePicker`], [`DateRangePicker`], [`TimePicker`],
//! [`DateTimePicker`] and [`DateTimeRangePicker`], with Mantine props (controlled or
//! uncontrolled value, min/max, excluded dates, presets, locale, 12/24 h, clearable,
//! size/variant/radius from the theme) and the shared [`InputWrapper`].
//!
//! They compose Kit 0.9.1's time widgets (`CalendarState` + the unstyled base `Calendar`,
//! `TimeFieldState` + base `TimeField`, `Button`) rather than wrapping Kit's `DatePicker`
//! facade, because that facade cannot be configured to follow GUIraffe's conventions:
//! - it takes keyboard focus on pointer press and is always a tab stop (spec/00 §12.3);
//! - its open state is private (no controlled `open`/`on_open_change`, no Space/↓ to open);
//! - its calendar has no keyboard day navigation;
//! - disabled is `opacity(0.5)` and the focus style changes the border (§12.1, §12.2);
//! - min/max, the disabled matcher and the first weekday are construction-only builders;
//! - names come from the global `rust-i18n` locale, not per picker;
//! - a range cannot carry times.

pub mod locale;
pub mod logic;
pub mod picker;
pub mod wrapper;

pub use locale::{DateLocale, HourFormat, PickerLabels, format_pattern};
pub use logic::{
    CursorMove, DatePredicate, DatePreset, DateRange, DateRules, DateTimeRange, Parts, PickerMode,
    PickerValue, PresetRange, clamp_time, month_grid, move_cursor, pick_range, preview_range,
    visible_days, week_start,
};
pub use picker::{
    DatePicker, DateRangePicker, DateTimePicker, DateTimeRangePicker, PICKER_CONTEXT, Picker,
    PickerEvent, PickerProps, TimePicker,
};
pub use wrapper::{FrameColors, InputVariant, InputWrapper, error_color, frame_colors};

/// Register the picker key bindings.
pub fn init(cx: &mut gpui_kit::App) {
    picker::init(cx);
}
