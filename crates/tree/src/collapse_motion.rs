//! Opt-in collapse/expand animation for TreeList and PropertyGrid (spec/29).
//!
//! Kept apart from the view on purpose: the view only reports a toggle (`toggled`) and asks
//! for the current shift (`sample`). Rows from `anchor` on are painted shifted by the value,
//! clipped below the toggled group, so expanding slides the new rows out from under the
//! header and collapsing slides the rows below up to close the gap.

use guiraffe_motion::{Animated, MotionPrefs};
use std::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct CollapseMotion {
    /// Per-view override of `MotionPrefs::layout.enabled` (None follows the global setting).
    pub enabled: Option<bool>,
    shift: Option<(usize, Animated<f32>)>,
}

/// The shift to paint this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RowShift {
    /// First display rank that is shifted.
    pub anchor: usize,
    /// Vertical paint offset in pixels (negative: rows are drawn higher than laid out).
    pub offset: f32,
    pub moving: bool,
}

impl CollapseMotion {
    /// A group toggled: rows from display rank `anchor` on moved by `delta` px in layout
    /// (positive on expand). They start where they were visually and ease into place.
    pub fn toggled(&mut self, anchor: usize, delta: f64, now: Instant, prefs: &MotionPrefs) {
        let delta = delta as f32;
        if !prefs.layout_enabled(self.enabled) || !delta.is_finite() || delta.abs() < 0.5 {
            self.shift = None;
            return;
        }
        let current = self
            .shift
            .as_ref()
            .map_or(0.0, |(_, a)| a.value_at(now, prefs).0);
        let mut anim = Animated::new(current - delta, prefs.layout.timing);
        anim.set_at(0.0, now, prefs);
        self.shift = Some((anchor, anim));
    }

    /// Stop animating (filtering, source replaced).
    pub fn reset(&mut self) {
        self.shift = None;
    }

    /// The shift at `now`; forgets it once settled.
    pub fn sample(&mut self, now: Instant, prefs: &MotionPrefs) -> Option<RowShift> {
        let (anchor, anim) = self.shift.as_ref()?;
        let (offset, moving) = anim.value_at(now, prefs);
        if !moving {
            self.shift = None;
            return None;
        }
        Some(RowShift {
            anchor: *anchor,
            offset,
            moving,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_motion::{Easing, LayoutMotion, Timing};
    use std::time::Duration;

    fn prefs(enabled: bool) -> MotionPrefs {
        MotionPrefs {
            layout: LayoutMotion {
                enabled,
                timing: Timing::tween(Duration::from_millis(100), Easing::Linear),
            },
            ..Default::default()
        }
    }

    #[test]
    fn expand_slides_from_under_the_header_and_settles() {
        let p = prefs(true);
        let t0 = Instant::now();
        let mut m = CollapseMotion::default();
        m.toggled(3, 120.0, t0, &p);
        let s = m.sample(t0, &p);
        assert_eq!(s.map(|s| (s.anchor, s.offset)), Some((3, -120.0)));
        let s = m.sample(t0 + Duration::from_millis(50), &p);
        assert_eq!(s.map(|s| s.offset), Some(-60.0));
        assert_eq!(m.sample(t0 + Duration::from_millis(100), &p), None);
        assert_eq!(m.sample(t0 + Duration::from_millis(200), &p), None);
    }

    #[test]
    fn reversing_mid_flight_continues_from_the_visual_position() {
        let p = prefs(true);
        let t0 = Instant::now();
        let mut m = CollapseMotion::default();
        m.toggled(3, 100.0, t0, &p);
        let mid = t0 + Duration::from_millis(40); // visually at -60
        m.toggled(3, -100.0, mid, &p);
        assert_eq!(m.sample(mid, &p).map(|s| s.offset), Some(40.0), "no jump");
    }

    #[test]
    fn off_by_default_override_and_reduced_motion() {
        let t0 = Instant::now();
        let mut m = CollapseMotion::default();
        m.toggled(1, 50.0, t0, &prefs(false));
        assert_eq!(
            m.sample(t0, &prefs(false)),
            None,
            "follows the global default (off)"
        );
        m.enabled = Some(true);
        m.toggled(1, 50.0, t0, &prefs(false));
        assert!(
            m.sample(t0, &prefs(false)).is_some(),
            "per-view override wins"
        );
        let reduced = MotionPrefs {
            reduced: true,
            ..prefs(true)
        };
        m.toggled(1, 50.0, t0, &reduced);
        assert_eq!(m.sample(t0, &reduced), None, "reduced motion is instant");
    }

    mod gpui {
        use super::prefs;
        use crate::{Children, NodeId, TreeData, TreeList, TreeSource};
        use gpui_kit::test::TestQueryExt;
        use gpui_kit::{AppContext, Role, TestAppContext, VisualTestContext};
        use std::{rc::Rc, time::Duration};

        struct Data;
        static KIDS: [NodeId; 5] = [1, 2, 3, 4, 5];
        impl TreeSource for Data {
            fn roots(&self) -> &[NodeId] {
                &[0]
            }
            fn children(&self, id: NodeId) -> Children<'_> {
                Children::Ready(if id == 0 { &KIDS } else { &[] })
            }
            fn label(&self, id: NodeId) -> &str {
                if id == 0 { "Group" } else { "Item" }
            }
        }
        impl TreeData for Data {}

        /// Lowest painted top of the child rows.
        fn last_child_top(window: &mut VisualTestContext) -> Option<f32> {
            window.update(|window, _| {
                window
                    .find_all_by_role(Role::TreeItem)
                    .iter()
                    .filter(|row| row.label() == Some("Item"))
                    .map(|row| f32::from(row.bounds().origin.y))
                    .reduce(f32::max)
            })
        }

        fn frame(window: &mut VisualTestContext, ms: u64) -> usize {
            window.executor().advance_clock(Duration::from_millis(ms));
            let n = window.update(|window, cx| window.simulate_next_frame(cx));
            window.run_until_parked();
            n
        }

        fn run(cx: &mut TestAppContext, global: bool) -> (Option<f32>, Option<f32>, usize) {
            cx.update(|cx| {
                gpui_kit::base::init(cx);
                gpui_kit::component::init(cx);
                guiraffe_core::apply(Default::default(), cx);
                guiraffe_motion::MotionPrefs::set(prefs(global), cx);
            });
            let view = cx.new(|cx| TreeList::new(Rc::new(Data), cx));
            let v2 = view.clone();
            let (_, window) =
                cx.add_window_view(|window, cx| gpui_kit::base::Root::new(v2, window, cx));
            window.run_until_parked();
            view.update(window, |view, cx| view.set_expanded(0, true, cx));
            window.run_until_parked();
            let start = last_child_top(window);
            let mut frames = 0;
            for _ in 0..20 {
                frames += frame(window, 20);
            }
            (start, last_child_top(window), frames)
        }

        #[gpui_kit::test]
        fn expand_slides_rows_from_under_the_group_and_settles(cx: &mut TestAppContext) {
            let (start, end, frames) = run(cx, true);
            let (start, end) = (start.unwrap_or(-1.), end.unwrap_or(-1.));
            assert!(end > 100., "rows laid out below the group: {end}");
            assert!(
                start < end - 100.,
                "starts tucked under the header: {start} vs {end}"
            );
            assert!(frames > 2, "animated over frames: {frames}");
            assert!(frames < 20, "and stopped requesting frames once settled");
        }

        #[gpui_kit::test]
        fn expand_is_instant_when_off(cx: &mut TestAppContext) {
            let (start, end, frames) = run(cx, false);
            assert_eq!(start, end);
            assert_eq!(frames, 0, "no frames requested");
        }
    }
}
