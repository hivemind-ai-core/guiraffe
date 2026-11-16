//! guiraffe-timeline: Gantt, DAW tracks, dope sheet, curve editor on one shared time axis.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};
use guiraffe_timeline::*;

use super::{column, example};
use crate::catalog::Entry;

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "timeline",
            name: "Gantt chart",
            summary: "Critical-path scheduling; drag a bar to reschedule and dependents follow. Wheel pans, Ctrl+wheel zooms.",
            covers: &["GanttView"],
            demo: gantt,
        },
        Entry {
            krate: "timeline",
            name: "Track timeline",
            summary: "DAW-style clip tracks: drag to move, drag edges to trim, click the ruler to seek; clips never overlap.",
            covers: &["TracksView"],
            demo: tracks,
        },
        Entry {
            krate: "timeline",
            name: "Dope sheet",
            summary: "Keyframes per channel, frame-snapped retiming.",
            covers: &["DopeSheet"],
            demo: dope,
        },
        Entry {
            krate: "timeline",
            name: "Curve editor",
            summary: "Keyframe curves (constant, linear, bezier); drag keys.",
            covers: &["CurveView"],
            demo: curve,
        },
    ]
}

struct Holder<V: 'static>(Entity<V>, &'static str, f32);

impl<V: Render> Render for Holder<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([example(
            self.1,
            div().h(px(self.2)).overflow_hidden().child(self.0.clone()),
        )])
    }
}

fn hold<V: Render>(e: Entity<V>, label: &'static str, h: f32, cx: &mut App) -> AnyView {
    cx.new(|_| Holder(e, label, h)).into()
}

const DAY: f64 = 86_400.0;

fn gantt(_: &mut Window, cx: &mut App) -> AnyView {
    let start = 20_400.0 * DAY;
    let t = |id, name: &str, d: f64, after: &[u32]| Task {
        id,
        name: name.into(),
        not_before: start,
        duration: d * DAY,
        after: after.to_vec(),
    };
    let v = GanttView::new(
        vec![
            t(1, "Research", 3.0, &[]),
            t(2, "Design", 4.0, &[1]),
            t(3, "Build", 8.0, &[2]),
            t(4, "Docs", 3.0, &[2]),
            t(5, "QA", 3.0, &[3]),
            t(6, "Launch", 1.0, &[4, 5]),
        ],
        TimeFormat::Days,
        cx,
    );
    hold(v, "Project plan (critical path in red)", 260., cx)
}

fn tracks(_: &mut Window, cx: &mut App) -> AnyView {
    let mk = |clips: &[(f64, f64)]| {
        let mut t = Track::default();
        for (s, l) in clips {
            t.insert(*s, *l);
        }
        t
    };
    let v = TracksView::new(
        vec![
            ("Drums".into(), mk(&[(0.0, 4.0), (5.0, 3.0)])),
            ("Bass".into(), mk(&[(1.0, 6.0)])),
            ("Keys".into(), mk(&[(2.0, 1.5), (4.0, 1.5), (6.0, 2.0)])),
            ("Vocals".into(), mk(&[(3.0, 5.0)])),
        ],
        TimeFormat::Seconds,
        10.0,
        cx,
    );
    v.update(cx, |v, _| v.set_snap(Some(0.25)));
    hold(v, "Four tracks, 0.25s snap", 220., cx)
}

fn dope(_: &mut Window, cx: &mut App) -> AnyView {
    let v = DopeSheet::new(
        vec![
            Channel::new("position.x", vec![0.0, 0.5, 1.0, 2.0]),
            Channel::new("position.y", vec![0.0, 0.75, 1.5]),
            Channel::new("rotation", vec![0.25, 1.25, 2.25]),
            Channel::new("opacity", vec![0.0, 0.2, 2.8, 3.0]),
        ],
        TimeFormat::Frames(24.0),
        3.0,
        cx,
    );
    hold(v, "24 fps", 140., cx)
}

fn curve(_: &mut Window, cx: &mut App) -> AnyView {
    let mut c = Curve::default();
    let mut k = Key::new(0.0, 0.0, Interp::Bezier);
    k.out_handle = (0.4, 0.0);
    c.set(k);
    let mut k = Key::new(1.0, 1.0, Interp::Linear);
    k.in_handle = (-0.4, 0.0);
    c.set(k);
    c.set(Key::new(2.0, 0.3, Interp::Constant));
    c.set(Key::new(3.0, 0.8, Interp::Linear));
    let v = CurveView::new(c, 0.0, 3.0, -0.2, 1.2, cx);
    hold(v, "Ease-out, linear, step", 260., cx)
}
