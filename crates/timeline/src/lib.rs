//! GUIraffe timeline: time-based editors on one shared axis (spec/16 Foundation D).

pub mod axis;
pub mod clips;
pub mod curve_view;
pub mod curves;
pub mod dope_sheet;
pub mod edit;
pub mod gantt;
pub mod gantt_view;
mod measure;

/// The sub-slice of a time-sorted, non-overlapping slice that intersects the axis view.
/// `start`/`end` give each item's extent; cost is O(log n).
pub(crate) fn visible<'a, T>(
    items: &'a [T],
    axis: &Axis,
    start: impl Fn(&T) -> f64,
    end: impl Fn(&T) -> f64,
) -> &'a [T] {
    let lo = items.partition_point(|i| end(i) < axis.start);
    let hi = items.partition_point(|i| start(i) <= axis.end());
    items.get(lo..hi.max(lo)).unwrap_or(&[])
}
pub mod tracks_view;

pub use axis::{Axis, TimeFormat};
pub use clips::{Clip, Edge, Track};
pub use curve_view::{CurveEvent, CurveView, ValueRange};
pub use curves::{Curve, Interp, Key};
pub use dope_sheet::{Channel, DopeEvent, DopeSheet};
pub use gantt::{ScheduleError, Scheduled, Task, schedule};
pub use gantt_view::{GanttEvent, GanttView};
pub use tracks_view::{TracksEvent, TracksView};

pub use edit::ResizeEdges;

/// Register the keyboard bindings of the editable views (see [`edit::init`]).
pub fn init(cx: &mut gpui_kit::App) {
    edit::init(cx);
}

#[cfg(test)]
mod visible_tests {
    use super::*;

    #[test]
    fn visible_is_the_intersecting_window() {
        let a = Axis::new(10.0, 20.0, 100.0);
        let iv = [
            (0.0, 5.0),
            (8.0, 11.0),
            (12.0, 13.0),
            (19.5, 25.0),
            (30.0, 31.0),
        ];
        let v = visible(&iv, &a, |i| i.0, |i| i.1);
        assert_eq!(v, &iv[1..4]);
        assert!(visible(&iv[..1], &a, |i| i.0, |i| i.1).is_empty());
        let none: [(f64, f64); 0] = [];
        assert!(visible(&none, &a, |i| i.0, |i| i.1).is_empty());
    }
}

pub mod plot;
pub use plot::{PlotSurface, Layer, Gridlines, Ruler, GridMarks, Playhead, SelectionBox, Markers, Zebra, BackgroundBands};
mod locks;
pub use locks::LockStyle;

pub mod bars;
pub use bars::BarCurve;
pub use curves::TangentMode;
