//! guiraffe-timeline: Gantt, DAW tracks, dope sheet, curve editor on one shared time axis.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};
use guiraffe_timeline::*;

use guiraffe_components::{SegmentedControl, SegmentedEvent, Text};
use guiraffe_core::Size;

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

const EDGES: [ResizeEdges; 4] = [
    ResizeEdges::Both,
    ResizeEdges::Start,
    ResizeEdges::End,
    ResizeEdges::None,
];

/// Live editing options shared by the Gantt and track demos: which edges resize, and a
/// global lock. Changing an option touches only the timeline view, not the shell.
struct EditOptions<V: 'static> {
    view: Entity<V>,
    label: &'static str,
    height: f32,
    edges: Entity<SegmentedControl>,
    lock: Entity<SegmentedControl>,
    note: &'static str,
    _subs: [gpui_kit::Subscription; 2],
}

trait Editable: Render {
    fn set_edges(&mut self, edges: ResizeEdges, cx: &mut Context<Self>);
    fn set_all_editable(&mut self, editable: bool, cx: &mut Context<Self>);
}

impl Editable for GanttView {
    fn set_edges(&mut self, e: ResizeEdges, cx: &mut Context<Self>) {
        self.set_resize_edges(e, cx)
    }
    fn set_all_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_editable(on, cx)
    }
}

impl Editable for TracksView {
    fn set_edges(&mut self, e: ResizeEdges, cx: &mut Context<Self>) {
        self.set_resize_edges(e, cx)
    }
    fn set_all_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_editable(on, cx)
    }
}

fn with_options<V: Editable>(
    view: Entity<V>,
    label: &'static str,
    height: f32,
    note: &'static str,
    cx: &mut App,
) -> AnyView {
    let edges = SegmentedControl::new(["Resize: both", "Start", "End", "None"], cx);
    let lock = SegmentedControl::new(["Editable", "Locked"], cx);
    cx.new(|cx| {
        let v1 = view.clone();
        let v2 = view.clone();
        let s1 = cx.subscribe(&edges, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            let edges = EDGES.get(*i).copied().unwrap_or_default();
            v1.update(cx, |v, cx| v.set_edges(edges, cx));
        });
        let s2 = cx.subscribe(&lock, move |_, _, e: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(i) = e;
            v2.update(cx, |v, cx| v.set_all_editable(*i == 0, cx));
        });
        EditOptions {
            view,
            label,
            height,
            edges,
            lock,
            note,
            _subs: [s1, s2],
        }
    })
    .into()
}

impl<V: Editable> Render for EditOptions<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Options",
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(16.))
                    .child(div().w(px(340.)).child(self.edges.clone()))
                    .child(div().w(px(200.)).child(self.lock.clone()))
                    .child(Text::new(self.note).size(Size::Xs).dimmed(true)),
            ),
            example(
                self.label,
                div()
                    .h(px(self.height))
                    .overflow_hidden()
                    .child(self.view.clone()),
            ),
        ])
    }
}

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
    // Launch date is fixed: lock it to show per-item locks (dashed, faded).
    v.update(cx, |v, cx| v.set_locked(6, true, cx));
    with_options(
        v,
        "Project plan (critical path in red; Launch is locked)",
        260.,
        "Drag bar ends to resize; the cursor shows what a drag will do.",
        cx,
    )
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
    // Lock the vocal take.
    v.update(cx, |v, cx| {
        if let Some(id) = v
            .tracks()
            .get(3)
            .and_then(|(_, t)| t.clips().first())
            .map(|c| c.id)
        {
            v.set_locked(3, id, true, cx);
        }
    });
    with_options(
        v,
        "Four tracks, 0.25s snap (vocals locked)",
        220.,
        "Trim either edge; clips can't overlap or extend past their media.",
        cx,
    )
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
