//! DAW / animation track timeline: named tracks of clips, a playhead, drag to move,
//! drag clip edges to trim, wheel pan / ctrl+wheel zoom.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    App, Bounds, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Render, Role, ScrollEvent,
    SharedString, Styled, TestSupportExt, Window, canvas, div, prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, StateKind, Variant, focus_ring, ring_visible};

use crate::{
    axis::{Axis, TimeFormat, label},
    clips::{Edge, Track},
    edit::{
        EndEarlier, EndLater, KEY_CONTEXT, MoveEarlier, MoveLater, Nudge, SelectNextItem,
        SelectPrevItem, StartEarlier, StartLater, key_step,
    },
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
    surface: crate::PlotSurface,
    lock_style: crate::LockStyle,
    global_lock_style: crate::LockStyle,
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
    /// Selected clip `(track, clip id)`; keyboard edits act on it.
    selected: Option<(usize, u32)>,
    focus: FocusHandle,
    /// `None`: a tab stop only inside a focus scope.
    focusable: Option<bool>,
}

impl EventEmitter<TracksEvent> for TracksView {}

impl Focusable for TracksView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Element id of the clip `clip` on track `track` (for tests and accessibility).
pub fn clip_element_id(track: usize, clip: u32) -> ElementId {
    ElementId::NamedInteger(SharedString::from(format!("clip-{track}")), u64::from(clip))
}

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
        cx.new(|cx| Self {
            tracks,
            surface: crate::PlotSurface::timeline(axis, format),
            lock_style: crate::LockStyle::Badge,
            global_lock_style: crate::LockStyle::None,
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
            selected: None,
            focus: cx.focus_handle(),
            focusable: None,
        })
    }

    /// The selected clip `(track, clip id)`.
    pub fn selected(&self) -> Option<(usize, u32)> {
        self.selected
    }

    pub fn select(&mut self, sel: Option<(usize, u32)>, cx: &mut Context<Self>) {
        let sel = sel.filter(|(t, c)| {
            self.tracks
                .get(*t)
                .is_some_and(|(_, tr)| tr.clips().iter().any(|x| x.id == *c))
        });
        if self.selected != sel {
            self.selected = sel;
            cx.notify();
        }
    }

    /// Opt into (or out of) Tab focus regardless of the enclosing focus scope.
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = v;
        cx.notify();
    }

    /// Keyboard edit of the selected clip by `dir` steps (snap grid, else a quarter tick).
    pub fn nudge(&mut self, what: Nudge, dir: f64, cx: &mut Context<Self>) {
        let Some((ti, id)) = self.selected else {
            return;
        };
        if !self.editable || self.locked.contains(&(ti, id)) {
            return;
        }
        let d = dir * key_step(self.snap, self.axis.tick_step(80.0, self.format));
        let snap = self.snap;
        let snapped = |t: f64| match snap {
            Some(g) => (t / g).round() * g,
            None => t,
        };
        let Some((_, tr)) = self.tracks.get_mut(ti) else {
            return;
        };
        let Some(c) = tr.clips().iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let changed = match what {
            Nudge::Move => tr.move_clip(id, snapped(c.start + d)),
            Nudge::End => tr.trim(id, Edge::End, snapped(c.end() + d)),
            Nudge::Start => tr.trim(id, Edge::Start, snapped(c.start + d)),
        };
        if changed {
            cx.emit(TracksEvent::ClipChanged {
                track: ti,
                clip: id,
            });
            cx.notify();
        }
    }

    fn all_clips(&self) -> Vec<(usize, u32)> {
        self.tracks
            .iter()
            .enumerate()
            .flat_map(|(i, (_, tr))| tr.clips().iter().map(move |c| (i, c.id)))
            .collect()
    }

    fn select_step(&mut self, dir: i32, cx: &mut Context<Self>) {
        let all = self.all_clips();
        let cur = self.selected.and_then(|s| all.iter().position(|x| *x == s));
        let next = crate::edit::cycle(cur, all.len(), dir);
        self.select(next.and_then(|i| all.get(i).copied()), cx);
    }

    pub fn tracks(&self) -> &[(String, Track)] {
        &self.tracks
    }

    pub fn playhead(&self) -> f64 {
        self.playhead
    }

    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn set_lock_style(
        &mut self,
        item: crate::LockStyle,
        global: crate::LockStyle,
        cx: &mut Context<Self>,
    ) {
        self.lock_style = item;
        self.global_lock_style = global;
        cx.notify();
    }
    pub fn axis(&self) -> &Axis {
        &self.surface.axis
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
        // Any press on a clip selects it (locked clips too); only editable clips drag.
        if let Some((_, tr)) = self.tracks.get(track)
            && let Some((clip, _)) = hit(tr, &self.axis, x, ResizeEdges::None)
        {
            self.selected = Some((track, clip));
        }
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
        hit(tr, &self.surface.axis, x, self.resize)
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
        let t = self.surface.axis.t(x);
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
        let changed = self.surface.navigate(
            x,
            f64::from(f32::from(d.x)),
            f64::from(f32::from(d.y)),
            e.modifiers.control || e.modifiers.platform,
        );
        if changed {
            cx.notify();
        }
    }
}

impl Render for TracksView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let clip_v = t.variant(None, Variant::Light);
        let off = t.state(StateKind::Disabled, Variant::Light, None);
        let ring = t.state(StateKind::FocusRing, Variant::Light, None).border;
        let playhead_color = t.variant(Some(ColorName::Red), Variant::Filled).background;
        let ring_on = ring_visible(&self.focus, window);
        let tab = guiraffe_core::tab_stop(self.focusable);
        let selected = self.selected;
        if let Some(w) = crate::measure::width(&self.body) {
            self.surface.axis.set_width(w);
            if let Some((a, b)) = self.pending_fit.take() {
                self.surface.fit(a, b, 0.0);
            }
        }
        let axis = self.surface.axis;
        let format=self.format;
        let editable = self.editable;
        let item_style = self.lock_style;
        let global_style = self.global_lock_style;
        let locked = &self.locked;

        let line = s.border.to_hsla();
        let body = self.body.clone();
        let owner=cx.entity_id();
        let root = div()
            .id("tracks")
            .test_support()
            .role(Role::ListBox)
            .aria_label("Tracks")
            .track_focus(&self.focus.clone().tab_stop(tab))
            .key_context(KEY_CONTEXT)
            .on_action(
                cx.listener(|this, _: &MoveEarlier, _, cx| this.nudge(Nudge::Move, -1.0, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveLater, _, cx| this.nudge(Nudge::Move, 1.0, cx)))
            .on_action(cx.listener(|this, _: &EndEarlier, _, cx| this.nudge(Nudge::End, -1.0, cx)))
            .on_action(cx.listener(|this, _: &EndLater, _, cx| this.nudge(Nudge::End, 1.0, cx)))
            .on_action(
                cx.listener(|this, _: &StartEarlier, _, cx| this.nudge(Nudge::Start, -1.0, cx)),
            )
            .on_action(cx.listener(|this, _: &StartLater, _, cx| this.nudge(Nudge::Start, 1.0, cx)))
            .on_action(cx.listener(|this, _: &SelectPrevItem, _, cx| this.select_step(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNextItem, _, cx| this.select_step(1, cx)))
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
                                    let t = this.surface.axis.t(x);
                                    this.seek(t, cx);
                                }),
                            )
                            .child(
                                self.surface
                                    .element(t.clone(), true, f64::from(TRACK_H), 0.),
                            ),
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
                            .children(self.tracks.iter().enumerate().map(|(i, (name, track))| {
                                let clip = track.clips().first().map(|c| c.id);
                                let locked = clip.is_some_and(|id| self.is_locked(i, id));
                                div()
                                    .h(px(TRACK_H))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(line)
                                    .child(crate::locks::probe(
                                        "track-lock",
                                        i,
                                        gpui_kit::component::button::Button::new(("track-lock", i))
                                            .icon(gpui_kit::component::Icon::new(if locked {
                                                gpui_kit::assets::IconName::Lock
                                            } else {
                                                gpui_kit::assets::IconName::LockOpen
                                            }))
                                            .tab_stop(false)
                                            .tooltip("Toggle track clip locks")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let ids: Vec<_> = this
                                                    .tracks
                                                    .get(i)
                                                    .map(|(_, t)| {
                                                        t.clips().iter().map(|c| c.id).collect()
                                                    })
                                                    .unwrap_or_default();
                                                for id in ids {
                                                    this.set_locked(i, id, !locked, cx);
                                                }
                                            })),
                                    ))
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
                                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                                    // Pointer edits never take keyboard focus (§12.3).
                                    window.prevent_default();
                                    cx.notify();
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
                                    move |b, _, cx| crate::measure::record(&body, b, cx, owner),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(self.surface.element(t.clone(), false, 32., 0.))
                            .children(self.tracks.iter().enumerate().flat_map(|(i, (name, tr))| {
                                crate::visible(tr.clips(), &axis, |c| c.start, |c| c.end())
                                    .iter()
                                    .map(|c| {
                                        let (x0, x1) =
                                            (axis.x(c.start) as f32, axis.x(c.end()) as f32);
                                        let on = selected == Some((i, c.id));
                                        let is_locked = !editable || locked.contains(&(i, c.id));
                                        div()
                                            .id(clip_element_id(i, c.id))
                                            .test_support()
                                            .role(Role::ListBoxOption)
                                            .aria_label(format!(
                                                "{name} clip {}: {} – {}",
                                                c.id,
                                                label(c.start, format),
                                                label(c.end(), format)
                                            ))
                                            .aria_selected(on)
                                            .absolute()
                                            .top(px(i as f32 * TRACK_H + 4.0))
                                            .left(px(x0))
                                            .w(px((x1 - x0).max(2.0)))
                                            .h(px(TRACK_H - 8.0))
                                            .rounded(px(4.))
                                            .border_1()
                                            .bg(clip_v.background.to_hsla()).border_color(clip_v.foreground.to_hsla())
                                            .when(is_locked,|el|if locked.contains(&(i,c.id)){item_style.decorate(el,t)}else{global_style.decorate(el,t)})
                                        // Selection: 2px overlay outline, no layout change.
                                            .when(on, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .inset_0()
                                                        .rounded(px(4.))
                                                        .border_2()
                                                        .border_color(s.text.to_hsla()),
                                                )
                                            })
                                    })
                                    .collect::<Vec<_>>()
                            }))
                            .child({
                                let mut layer = crate::PlotSurface::new(axis);
                                layer.add_layer(crate::Playhead {
                                    time: self.playhead,
                                    format: self.format,
                                });
                                layer.element(t.clone(), false, f64::from(TRACK_H), 0.)
                            }),
                    ),
            );
        focus_ring(root, ring_on, px(0.), ring)
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
