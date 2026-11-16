//! GUIraffe timeline: time-based editors on one shared axis (spec/16 Foundation D).

pub mod axis;
pub mod clips;
pub mod curves;
pub mod gantt;
pub mod gantt_view;
pub mod tracks_view;

pub use axis::{Axis, TimeFormat};
pub use clips::{Clip, Edge, Track};
pub use gantt::{ScheduleError, Scheduled, Task, schedule};
pub use gantt_view::{GanttEvent, GanttView};
pub use tracks_view::{TracksEvent, TracksView};
