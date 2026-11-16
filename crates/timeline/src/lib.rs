//! GUIraffe timeline: time-based editors on one shared axis (spec/16 Foundation D).

pub mod axis;
pub mod clips;
pub mod curves;
pub mod gantt;
pub mod gantt_view;

pub use axis::{Axis, TimeFormat};
pub use gantt::{ScheduleError, Scheduled, Task, schedule};
pub use gantt_view::{GanttEvent, GanttView};
