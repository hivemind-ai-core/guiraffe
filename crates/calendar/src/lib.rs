//! Year, month, week, and day calendar views.
//!
//! The model is independent of GPUI and safely shares immutable event storage:
//!
//! ```
//! use chrono::{DateTime, Utc};
//! use guiraffe_calendar::{CalendarItem, EventId, SourceId, CalendarSource, CalendarModel};
//! use guiraffe_core::ColorName;
//! let start = DateTime::from_timestamp(0, 0).unwrap_or(DateTime::UNIX_EPOCH);
//! let item = guiraffe_calendar::CalendarItem::timed(
//!     EventId(1), SourceId(1), "Review", start, start + chrono::Duration::hours(1),
//! )?;
//! let model = CalendarModel::new(
//!     [std::sync::Arc::new(item)],
//!     [CalendarSource::new(SourceId(1), "Work", ColorName::Blue)],
//! )?;
//! assert_eq!(model.items().len(), 1);
//! # Ok::<(), guiraffe_calendar::CalendarError>(())
//! ```

pub mod calendar;
mod document;
pub mod edit;
pub mod model;
mod state;
mod view;
use guiraffe_core::widget_spec;
mod widget_specs;

pub use state::{
    CalendarChange, CalendarCursor, CalendarEvent, CalendarState, ChangeKind, DragMode, DragPreview,
};
pub use view::{Calendar, CalendarProps, ItemContent, ItemRenderer, default_hour_height};

pub use calendar::{
    BusinessHours, CalendarLabels, CalendarLocale, CalendarViewKind, DateCell, TimeFormat,
    WorkingHours, format_hour, format_local_time, format_time_range, hour_label,
    initial_scroll_minutes, iso_week, minute_of_day, month_cells, navigate, row_iso_week,
    week_dates, week_start,
};
pub use edit::{
    PositionedEvent, ResizeEdge, TimedSlice, chip_capacity, collapse_lanes, layout_overlaps,
    month_chip_plan, move_all_day, move_timed, resize_all_day, resize_timed, snap_minutes,
    visible_hour_range,
};
pub use model::{
    CalendarError, CalendarIndex, CalendarItem, CalendarModel, CalendarSource, CalendarZone,
    EventId, ItemTime, SourceId, seconds_in_display_day, time_seconds_in_display_day,
};

/// Register theme dependencies and calendar keyboard bindings. Idempotent.
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    state::register_keys(cx);
    document::register(cx);
}

#[cfg(test)]
mod gpui_tests;
