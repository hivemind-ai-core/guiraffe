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
    clips::{Edge, Track},
};

pub const TRACK_H: f32 = 40.0;
pub const HEADER_W: f32 = 140.0;
pub const RULER_H: f32 = 24.0;
/// Pixels from a clip edge that grab a trim instead of a move.
pub const EDGE_GRAB: f64 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub enum TracksEvent {
    ClipChanged { track: usize, clip: u32 },
    Seek(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Grab {
    Move { grab_dt: f64 },
    Trim(Edge),
}

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
}

impl EventEmitter<TracksEvent> for TracksView {}

/// Decide what a press at time `t` (pixel `x`) on `track` grabs. Pure; tested.
pub fn hit(track: &Track, axis: &Axis, x: f64) -> Option<(u32, Grab)> {
    let t = axis.t(x);
    track.clips().iter().find_map(|c| {
        let (x0, x1) = (axis.x(c.start), axis.x(c.end()));
        if (x - x0).abs() <= EDGE_GRAB {
            Some((c.id, Grab::Trim(Edge::Start)))
        } else if (x - x1).abs() <= EDGE_GRAB {
            Some((c.id, Grab::Trim(Edge::End)))
        } else if x > x0 && x < x1 {
            Some((
                c.id,
                Grab::Move {
                    grab_dt: t - c.start,
                },
            ))
        } else {
            None
        }
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
        self.snap = grid.filter(|g| *g > 0.0);
    }

    fn snapped(&self, t: f64) -> f64 {
        match self.snap {
            Some(g) => (t / g).round() * g,
            None => t,
        }
    }

    pub fn seek(&mut self, t: f64, cx: &mut Context<Self>) {
        if !t.is_finite() || t == self.playhead {
            return;
        }
        self.playhead = t.max(0.0);
        cx.emit(TracksEvent::Seek(self.playhead));
        cx.notify();
    }

    /// Begin a drag at body-local pixel `x` on `track`. Returns whether a clip was grabbed.
    pub fn press(&mut self, track: usize, x: f64) -> bool {
        self.drag = self
            .tracks
            .get(track)
            .and_then(|(_, tr)| hit(tr, &self.axis, x))
            .map(|(clip, grab)| Drag { track, clip, grab });
        self.drag.is_some()
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
        let w = f64::from(f32::from(self.body.get().size.width));
        if w > 0.0 {
            self.axis.set_width(w);
        }
        let axis = self.axis;
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
                                if e.pressed_button == Some(MouseButton::Left) {
                                    let (x, _) = this.local(e.position);
                                    this.drag_to(x, cx);
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, _| this.release()),
                            )
                            .child(
                                canvas(move |b, _, _| body.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                            .children(self.tracks.iter().enumerate().flat_map(|(i, (_, tr))| {
                                tr.clips()
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
        assert_eq!(hit(&t, &a, 102.0), Some((first, Grab::Trim(Edge::Start))));
        assert_eq!(hit(&t, &a, 297.0), Some((first, Grab::Trim(Edge::End))));
        assert!(matches!(hit(&t, &a, 200.0), Some((id, Grab::Move { .. })) if id == first));
        assert_eq!(hit(&t, &a, 400.0), None);
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
}
