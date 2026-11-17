//! DAW / animation track timeline: named tracks of clips, a playhead, drag to move,
//! drag clip edges to trim, wheel pan / ctrl+wheel zoom.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Pixels, Render, ScrollEvent, Styled, Window, canvas, div,
    prelude::*, px,
};
use guiraffe_core::{GuiraffeTheme, Variant};

use crate::{
    axis::{Axis, TimeFormat, label},
    clips::Track,
};

pub const TRACK_H: f32 = 40.0;
pub const HEADER_W: f32 = 140.0;
pub const RULER_H: f32 = 24.0;
pub use crate::edit::EDGE_GRAB;

#[derive(Debug, Clone, PartialEq)]
pub enum TracksEvent {
    ClipChanged { track: usize, clip: u32 },
    Seek(f64),
}

pub use crate::edit::{Grab, ResizeEdges};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    track: usize,
    clip: u32,
    grab: Grab,
}

pub struct TracksView {
    tracks: Vec<(String, Track)>,
    axis: Axis,
    format: TimeFormat,
    playhead: f64,
    snap: Option<f64>,
    drag: Option<Drag>,
    body: Rc<Cell<Bounds<Pixels>>>,
    /// Range to fit once the real width is known.
    pending_fit: Option<(f64, f64)>,
    resize: ResizeEdges,
    /// Clips that cannot be moved or resized, by (track, clip id).
    locked: std::collections::HashSet<(usize, u32)>,
    /// When false, nothing can be edited (seek still works).
    editable: bool,
    /// What the pointer is over, for the cursor.
    hover: Option<Grab>,
}

impl EventEmitter<TracksEvent> for TracksView {}

/// Decide what a press at time `t` (pixel `x`) on `track` grabs. Pure; tested.
pub fn hit(track: &Track, axis: &Axis, x: f64, edges: ResizeEdges) -> Option<(u32, Grab)> {
    let t = axis.t(x);
    // Clips are sorted and disjoint; only those near x can match.
    crate::visible(track.clips(), axis, |c| c.start, |c| c.end())
        .iter()
        .find_map(|c| {
            crate::edit::grab_at(axis.x(c.start), axis.x(c.end()), x, t, c.start, edges)
                .map(|g| (c.id, g))
        })
}

impl TracksView {
    pub fn new(
        tracks: Vec<(String, Track)>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        let axis = Axis::new(0.0, length.max(1.0), 800.0);
        cx.new(|_| Self {
            tracks,
            axis,
            format,
            playhead: 0.0,
            snap: None,
            drag: None,
            body: Rc::default(),
            pending_fit: Some((0.0, length.max(1.0))),
            resize: ResizeEdges::default(),
            locked: Default::default(),
            editable: true,
            hover: None,
        })
    }

    pub fn tracks(&self) -> &[(String, Track)] {
        &self.tracks
    }

    pub fn playhead(&self) -> f64 {
        self.playhead
    }

    pub fn axis(&self) -> &Axis {
        &self.axis
    }

    /// Snap edits to a grid (seconds); `None` disables.
    pub fn set_snap(&mut self, grid: Option<f64>) {
        self.snap = grid.filter(|g| g.is_finite() && *g > 0.0);
    }

    fn snapped(&self, t: f64) -> f64 {
        match self.snap {
            Some(g) => (t / g).round() * g,
            None => t,
        }
    }

    pub fn seek(&mut self, t: f64, cx: &mut Context<Self>) {
        if !t.is_finite() || t.max(0.0) == self.playhead {
            return;
        }
        self.playhead = t.max(0.0);
        cx.emit(TracksEvent::Seek(self.playhead));
        cx.notify();
    }

    /// Begin a drag at body-local pixel `x` on `track`. Returns whether a clip was grabbed.
    pub fn press(&mut self, track: usize, x: f64) -> bool {
        self.drag = self
            .grab(track, x)
            .map(|(clip, grab)| Drag { track, clip, grab });
        self.drag.is_some()
    }

    /// What a press at (`track`, `x`) would grab, honouring locks and the resize policy.
    pub fn grab(&self, track: usize, x: f64) -> Option<(u32, Grab)> {
        if !self.editable {
            return None;
        }
        let (_, tr) = self.tracks.get(track)?;
        hit(tr, &self.axis, x, self.resize)
            .filter(|(clip, _)| !self.locked.contains(&(track, *clip)))
    }

    /// Which clip edges resize (default: both).
    pub fn set_resize_edges(&mut self, edges: ResizeEdges, cx: &mut Context<Self>) {
        self.resize = edges;
        cx.notify();
    }

    /// Lock or unlock one clip's position and size.
    pub fn set_locked(&mut self, track: usize, clip: u32, locked: bool, cx: &mut Context<Self>) {
        let changed = if locked {
            self.locked.insert((track, clip))
        } else {
            self.locked.remove(&(track, clip))
        };
        if changed {
            cx.notify();
        }
    }

    pub fn is_locked(&self, track: usize, clip: u32) -> bool {
        self.locked.contains(&(track, clip))
    }

    /// Allow or forbid all edits (a global lock).
    pub fn set_editable(&mut self, editable: bool, cx: &mut Context<Self>) {
        if self.editable != editable {
            self.editable = editable;
            self.drag = None;
            cx.notify();
        }
    }

    /// Update the hover grab (cursor); re-renders only when it changes.
    fn hover_at(&mut self, track: usize, x: f64, cx: &mut Context<Self>) {
        let g = self.grab(track, x).map(|(_, g)| g);
        let kind = |g: Option<Grab>| g.map(|g| matches!(g, Grab::Move { .. }));
        let edge = |g: Option<Grab>| match g {
            Some(Grab::Trim(e)) => Some(e),
            _ => None,
        };
        if kind(g) != kind(self.hover) || edge(g) != edge(self.hover) {
            self.hover = g;
            cx.notify();
        }
    }

    /// Continue the active drag to body-local pixel `x`.
    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some(d) = self.drag else { return };
        let t = self.axis.t(x);
        let snapped = match d.grab {
            Grab::Move { grab_dt } => self.snapped(t - grab_dt),
            Grab::Trim(_) => self.snapped(t),
        };
        let Some((_, tr)) = self.tracks.get_mut(d.track) else {
            return;
        };
        let changed = match d.grab {
            Grab::Move { .. } => tr.move_clip(d.clip, snapped),
            Grab::Trim(edge) => tr.trim(d.clip, edge, snapped),
        };
        if changed {
            cx.emit(TracksEvent::ClipChanged {
                track: d.track,
                clip: d.clip,
            });
            cx.notify();
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    fn local(&self, p: gpui_kit::Point<Pixels>) -> (f64, f32) {
        let b = self.body.get();
        (
            f64::from(f32::from(p.x - b.origin.x)),
            f32::from(p.y - b.origin.y),
        )
    }

    fn on_wheel(&mut self, e: &ScrollEvent, cx: &mut Context<Self>) {
        let d = e.delta.pixel_delta(px(16.0));
        let (x, _) = self.local(e.position);
        let changed = if e.modifiers.control || e.modifiers.platform {
            self.axis
                .zoom_at(x, (1.0 + f64::from(f32::from(d.y)) * 0.002).max(0.1))
        } else {
            let dx = if f32::from(d.x) != 0.0 { d.x } else { d.y };
            self.axis.pan_px(-f64::from(f32::from(dx)));
            true
        };
        if changed {
            cx.notify();
        }
    }
}

impl Render for TracksView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let clip_v = t.variant(None, Variant::Light);
        if let Some(w) = crate::measure::width(&self.body) {
            self.axis.set_width(w);
            if let Some((a, b)) = self.pending_fit.take() {
                self.axis.fit(a, b, 0.0);
            }
        }
        let axis = self.axis;
        let editable = self.editable;
        let locked = &self.locked;
        let format = self.format;
        let ticks = axis.ticks(axis.tick_step(80.0, format));
        let line = s.border.to_hsla();
        let body = self.body.clone();
        let playhead_x = axis.x(self.playhead) as f32;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(12.))
            .child(
                div()
                    .h(px(RULER_H))
                    .flex()
                    .border_b_1()
                    .border_color(line)
                    .child(
                        div()
                            .w(px(HEADER_W))
                            .flex_none()
                            .border_r_1()
                            .border_color(line),
                    )
                    .child(
                        div()
                            .id("tracks-ruler")
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                                    let (x, _) = this.local(e.position);
                                    let t = this.axis.t(x);
                                    this.seek(t, cx);
                                }),
                            )
                            .children(ticks.into_iter().map(move |tt| {
                                div()
                                    .absolute()
                                    .left(px(axis.x(tt) as f32))
                                    .h_full()
                                    .pl(px(3.))
                                    .border_l_1()
                                    .border_color(line)
                                    .text_color(s.dimmed.to_hsla())
                                    .whitespace_nowrap()
                                    .child(label(tt, format))
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .child(
                        div()
                            .w(px(HEADER_W))
                            .flex_none()
                            .border_r_1()
                            .border_color(line)
                            .children(self.tracks.iter().map(|(name, _)| {
                                div()
                                    .h(px(TRACK_H))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(line)
                                    .child(name.clone())
                            })),
                    )
                    .child(
                        div()
                            .id("tracks-body")
                            .cursor(crate::edit::cursor_for(
                                self.drag.map(|d| d.grab).or(self.hover),
                            ))
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .on_scroll(
                                cx.listener(|this, e: &ScrollEvent, _, cx| this.on_wheel(e, cx)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, _, _| {
                                    let (x, y) = this.local(e.position);
                                    if y >= 0.0 {
                                        this.press((y / TRACK_H) as usize, x);
                                    }
                                }),
                            )
                            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                                let (x, y) = this.local(e.position);
                                if e.pressed_button == Some(MouseButton::Left) {
                                    this.drag_to(x, cx);
                                } else if y >= 0.0 {
                                    this.hover_at((y / TRACK_H) as usize, x, cx);
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, _| this.release()),
                            )
                            .child(
                                canvas(
                                    move |b, window, _| crate::measure::record(&body, b, window),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .children(self.tracks.iter().enumerate().flat_map(|(i, (_, tr))| {
                                crate::visible(tr.clips(), &axis, |c| c.start, |c| c.end())
                                    .iter()
                                    .map(|c| {
                                        let (x0, x1) =
                                            (axis.x(c.start) as f32, axis.x(c.end()) as f32);
                                        div()
                                            .absolute()
                                            .top(px(i as f32 * TRACK_H + 4.0))
                                            .left(px(x0))
                                            .w(px((x1 - x0).max(2.0)))
                                            .h(px(TRACK_H - 8.0))
                                            .rounded(px(4.))
                                            .bg(clip_v.background.to_hsla())
                                            .border_1()
                                            .border_color(clip_v.foreground.to_hsla())
                                            .when(!editable || locked.contains(&(i, c.id)), |el| {
                                                el.opacity(0.55).border_dashed()
                                            })
                                    })
                                    .collect::<Vec<_>>()
                            }))
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(px(playhead_x))
                                    .w(px(1.5))
                                    .bg(guiraffe_core::ColorName::Red.shade(6).to_hsla()),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clips::Edge;

    fn track() -> Track {
        let mut t = Track::default();
        t.insert(1.0, 2.0);
        t.insert(5.0, 1.0);
        t
    }

    fn axis() -> Axis {
        Axis::new(0.0, 10.0, 1000.0) // 100 px per second
    }

    #[test]
    fn hit_distinguishes_edges_body_and_gaps() {
        let (t, a) = (track(), axis());
        let first = t.clips()[0].id;
        assert_eq!(
            hit(&t, &a, 102.0, ResizeEdges::Both),
            Some((first, Grab::Trim(Edge::Start)))
        );
        assert_eq!(
            hit(&t, &a, 297.0, ResizeEdges::Both),
            Some((first, Grab::Trim(Edge::End)))
        );
        assert!(
            matches!(hit(&t, &a, 200.0, ResizeEdges::Both), Some((id, Grab::Move { .. })) if id == first)
        );
        assert_eq!(hit(&t, &a, 400.0, ResizeEdges::Both), None);
    }

    #[test]
    fn drag_moves_with_snap_and_seek_emits() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &TracksEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            v.set_snap(Some(0.5));
            let a = *v.axis();
            let x = |t: f64| a.x(t);
            assert!(v.press(0, x(2.0))); // clip body, grab_dt = 1
            v.drag_to(x(3.21), cx); // 2.21 -> snaps to 2.0
            v.release();
            v.seek(4.0, cx);
            v.seek(4.0, cx); // no-op
        });
        let start = view.read_with(&cx, |v, _| v.tracks()[0].1.clips()[0].start);
        assert_eq!(start, 2.0);
        assert_eq!(events.borrow().len(), 2);
        assert_eq!(events.borrow()[1], TracksEvent::Seek(4.0));
    }

    #[test]
    fn trim_end_and_press_on_missing_track() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            let x = |t: f64| a.x(t);
            assert!(!v.press(7, x(2.0)));
            assert!(v.press(0, x(3.0)));
            v.drag_to(x(4.5), cx); // end -> 4.5 (next clip at 5)
            v.drag_to(x(9.0), cx); // clamped by neighbour
        });
        let c = view.read_with(&cx, |v, _| v.tracks()[0].1.clips()[0].clone());
        assert!(c.end() <= 5.0 + 1e-9 && c.end() >= 4.5 - 1e-9, "{c:?}");
    }

    #[test]
    fn short_clips_stay_movable_and_negative_seek_is_idempotent() {
        let mut t = Track::default();
        let id = t.insert(1.0, 0.05).unwrap_or_default();
        let a = axis(); // 100 px/s: clip is 5 px wide
        assert!(
            matches!(hit(&t, &a, 102.5, ResizeEdges::Both), Some((i, Grab::Move { .. })) if i == id)
        );
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        let n = Rc::new(std::cell::RefCell::new(0));
        let ev = n.clone();
        let _s =
            cx.update(|cx| cx.subscribe(&view, move |_, _: &TracksEvent, _| *ev.borrow_mut() += 1));
        view.update(&mut cx.clone(), |v, cx| {
            v.set_snap(Some(f64::INFINITY));
            v.seek(-1.0, cx);
            v.seek(-2.0, cx);
        });
        assert_eq!(*n.borrow(), 0, "already at 0");
        assert!(
            view.read_with(&cx, |v, _| v.snap.is_none()),
            "inf grid rejected"
        );
    }

    #[test]
    fn locks_and_resize_policy() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            TracksView::new(vec![("A".into(), track())], TimeFormat::Seconds, 10.0, cx)
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            let first = v.tracks()[0].1.clips()[0].id;
            // Start-edge trim: clip [1,3] -> [1.5,3] (source offset advances; extending left
            // past the media start is refused by the model).
            assert!(v.press(0, a.x(1.0)));
            v.drag_to(a.x(1.5), cx);
            v.release();
            let c = v.tracks()[0].1.clips()[0].clone();
            assert_eq!((c.start, c.end(), c.source_offset), (1.5, 3.0, 0.5));
            v.set_locked(0, first, true, cx);
            assert!(v.grab(0, a.x(2.0)).is_none());
            v.set_locked(0, first, false, cx);
            v.set_resize_edges(ResizeEdges::End, cx);
            assert!(
                matches!(v.grab(0, a.x(1.5) + 1.0), Some((_, Grab::Move { .. }))),
                "start edge disabled"
            );
            v.set_editable(false, cx);
            assert!(!v.press(0, a.x(2.0)));
            v.seek(1.0, cx);
            assert_eq!(v.playhead(), 1.0, "seek still works when not editable");
        });
    }
}
