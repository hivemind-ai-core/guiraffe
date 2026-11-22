//!
//! A Rust `ui!` example.
//!
//! ```ignore
//! ui! { div { {cx.new(|cx| SortableBoard::new(columns, cx).with_grid(2))} } }
//! ```
mod board;
mod chrome;
pub use board::{BoardDragOverlay, Card, SortableBoard, gap_at};
pub mod dock;
pub mod sortable;

use gpui_kit::{
    AnyView, App, AppContext, Bounds, Context, CursorStyle, Entity, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render, ScrollHandle,
    StatefulInteractiveElement, StyleRefinement, Styled, Subscription, Window, div, px,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Default)]
pub enum Axis {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Default)]
pub struct Constraints {
    pub axis: Axis,
    pub containment: Option<Bounds<Pixels>>,
}

impl Constraints {
    /// Projects the pointer onto the allowed axis and bounds.
    pub fn position(self, origin: Point<Pixels>, mut pointer: Point<Pixels>) -> Point<Pixels> {
        match self.axis {
            Axis::Both => {}
            Axis::Horizontal => pointer.y = origin.y,
            Axis::Vertical => pointer.x = origin.x,
        }
        if let Some(bounds) = self.containment {
            let end = bounds.bottom_right();
            pointer.x = pointer.x.max(bounds.origin.x).min(end.x);
            pointer.y = pointer.y.max(bounds.origin.y).min(end.y);
        }
        pointer
    }
}

pub struct Drag<T> {
    pub payload: Rc<T>,
    pub constraints: Constraints,
    origin: Cell<Point<Pixels>>,
    gesture: RefCell<Weak<()>>,
}

impl<T> Drag<T> {
    pub fn position(&self, pointer: Point<Pixels>) -> Point<Pixels> {
        self.constraints.position(self.origin.get(), pointer)
    }
}

struct Preview {
    view: AnyView,
    constraints: Constraints,
    origin: Point<Pixels>,
    _cancel: Subscription,
    _gesture: Rc<()>,
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let pointer = window.mouse_position();
        let delta = self.constraints.position(self.origin, pointer) - pointer;
        div()
            .relative()
            .left(delta.x)
            .top(delta.y)
            .child(self.view.clone())
    }
}

pub struct DragSource<T> {
    pub payload: T,
    pub constraints: Constraints,
    pub focus: Option<FocusHandle>,
}

impl<T: 'static> DragSource<T> {
    pub fn new(payload: T) -> Self {
        Self {
            payload,
            constraints: Constraints::default(),
            focus: None,
        }
    }

    pub fn attach<E: StatefulInteractiveElement + Styled, P: Render>(
        self,
        element: E,
        preview: impl Fn(&T, &mut Window, &mut App) -> Entity<P> + 'static,
    ) -> E {
        let drag = Drag {
            payload: Rc::new(self.payload),
            constraints: self.constraints,
            origin: Cell::default(),
            gesture: RefCell::default(),
        };
        let element =
            element
                .cursor(CursorStyle::OpenHand)
                .on_drag(drag, move |drag, _, window, cx| {
                    let origin = window.mouse_position();
                    drag.origin.set(origin);
                    let gesture = Rc::new(());
                    *drag.gesture.borrow_mut() = Rc::downgrade(&gesture);
                    let view = preview(&drag.payload, window, cx).into();
                    let window_id = window.window_handle().window_id();
                    let cancel = cx.intercept_keystrokes(move |event, window, cx| {
                        if window.window_handle().window_id() == window_id
                            && event.keystroke.key == "escape"
                            && cx.stop_active_drag(window)
                        {
                            cx.stop_propagation();
                        }
                    });
                    cx.new(|_| Preview {
                        view,
                        origin,
                        constraints: drag.constraints,
                        _cancel: cancel,
                        _gesture: gesture,
                    })
                });
        if let Some(focus) = self.focus {
            element.track_focus(&focus)
        } else {
            element
        }
    }
}

pub struct DropTarget;

impl DropTarget {
    pub fn attach<T: 'static, E: InteractiveElement>(
        element: E,
        accept: impl Fn(&T, Point<Pixels>, &App) -> bool + 'static,
        indicator: impl Fn(StyleRefinement, &T, &App) -> StyleRefinement + 'static,
        drop: impl Fn(&T, Point<Pixels>, &mut Window, &mut App) + 'static,
    ) -> E {
        element
            .can_drop(move |value, window, cx| {
                value.downcast_ref::<Drag<T>>().is_some_and(|value| {
                    accept(&value.payload, value.position(window.mouse_position()), cx)
                })
            })
            .drag_over::<Drag<T>>(move |style, payload, _, cx| {
                indicator(style, &payload.payload, cx)
            })
            .on_drop::<Drag<T>>(move |payload, window, cx| {
                drop(
                    &payload.payload,
                    payload.position(window.mouse_position()),
                    window,
                    cx,
                );
            })
    }
}

/// Scroll displacement per tick. Positive values move toward the trailing edge.
pub fn edge_scroll(
    position: Pixels,
    start: Pixels,
    end: Pixels,
    edge: Pixels,
    speed: Pixels,
) -> Pixels {
    let edge = edge.min((end - start) / 2.0);
    if edge <= px(0.) || position < start || position > end {
        return px(0.);
    }
    let trailing = ((position - (end - edge)) / edge).max(0.);
    let leading = (((start + edge) - position) / edge).max(0.);
    speed * (trailing - leading)
}

/// Handles a keyboard gesture on a focused source. The destination resolver
/// supplies descriptions and acceptance for the application's list or grid.
pub struct KeyboardDrag {
    destination: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardEvent {
    PickedUp(usize),
    Moved(usize),
    Dropped(usize),
    Cancelled,
}

impl KeyboardDrag {
    pub fn new() -> Self {
        Self { destination: None }
    }

    pub fn key(
        &mut self,
        key: &str,
        source: usize,
        step: impl FnOnce(usize, &str) -> Option<usize>,
        accept: impl FnOnce(usize) -> bool,
    ) -> Option<KeyboardEvent> {
        match (key, self.destination) {
            ("space", None) => {
                self.destination = Some(source);
                Some(KeyboardEvent::PickedUp(source))
            }
            ("space", Some(index)) if accept(index) => {
                self.destination = None;
                Some(KeyboardEvent::Dropped(index))
            }
            ("escape", Some(_)) => {
                self.destination = None;
                Some(KeyboardEvent::Cancelled)
            }
            ("up" | "down" | "left" | "right", Some(index)) => {
                let next = step(index, key).filter(|next| *next != index)?;
                self.destination = Some(next);
                Some(KeyboardEvent::Moved(next))
            }
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.destination.is_some()
    }
}

impl Default for KeyboardDrag {
    fn default() -> Self {
        Self::new()
    }
}

/// Exposes a status message as a polite live region. Keep the element ID
/// stable as the text changes and give the element an accessibility role.
pub fn announce<E: StatefulInteractiveElement>(element: E, message: gpui_kit::SharedString) -> E {
    element.a11y_synthetic_children(move |tree| {
        let node = tree.parent_node();
        node.set_live(accesskit::Live::Polite);
        node.set_label(message.to_string());
    })
}

/// Keeps a vertical viewport scrolling while a matching drag stays near an edge.
/// Retain the handle across renders and pass the same scroll handle to GPUI's
/// `track_scroll` on the attached element.
#[derive(Clone)]
pub struct EdgeAutoScroll {
    scroll: ScrollHandle,
    edge: Pixels,
    speed: Pixels,
    scheduled: Rc<Cell<bool>>,
}

impl EdgeAutoScroll {
    pub fn new(scroll: ScrollHandle, edge: Pixels, speed: Pixels) -> Self {
        Self {
            scroll,
            edge,
            speed,
            scheduled: Rc::new(Cell::new(false)),
        }
    }

    pub fn attach<T: 'static, E: InteractiveElement>(&self, element: E) -> E {
        let control = self.clone();
        element.on_drag_move::<Drag<T>>(move |event, window, cx| {
            let drag = event.drag(cx);
            if control.scheduled.get() || !event.bounds.contains(&event.event.position) {
                return;
            }
            let gesture = drag.gesture.borrow().clone();
            control.clone().tick(gesture, window, cx);
        })
    }

    fn tick(self, gesture: Weak<()>, window: &mut Window, cx: &mut App) {
        self.scheduled.set(false);
        let bounds = self.scroll.bounds();
        let pointer = window.mouse_position();
        if !cx.has_active_drag() || gesture.upgrade().is_none() || !bounds.contains(&pointer) {
            return;
        }
        let amount = edge_scroll(
            pointer.y,
            bounds.top(),
            bounds.bottom(),
            self.edge,
            self.speed,
        );
        let mut offset = self.scroll.offset();
        let next = (offset.y - amount)
            .max(-self.scroll.max_offset().y)
            .min(px(0.));
        if next == offset.y {
            return;
        }
        offset.y = next;
        self.scroll.set_offset(offset);
        window.refresh();
        self.scheduled.set(true);
        window.on_next_frame(move |window, cx| self.tick(gesture, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, size};

    #[test]
    fn edge_scroll_ramps_and_stops_outside_the_viewport() {
        let displacement = |position| edge_scroll(px(position), px(10.), px(110.), px(20.), px(8.));
        assert_eq!(displacement(9.), px(0.));
        assert_eq!(displacement(20.), px(-4.));
        assert_eq!(displacement(60.), px(0.));
        assert_eq!(displacement(100.), px(4.));
        assert_eq!(displacement(111.), px(0.));
    }

    #[test]
    fn axis_lock_and_containment_project_drag_coordinates() {
        let limits = Constraints {
            axis: Axis::Vertical,
            containment: Some(Bounds::new(point(px(10.), px(20.)), size(px(60.), px(40.)))),
        };
        assert_eq!(
            limits.position(point(px(30.), px(30.)), point(px(0.), px(100.))),
            point(px(30.), px(60.))
        );
    }
}

use guiraffe_core::widget_spec;
mod widget_specs;

/// Register the sortable board for document authoring.
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::init(cx);
    }
    board::document::register(cx);
    dock::document::register(cx);
}
