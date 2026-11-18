mod board;
pub use board::{Card, SortableBoard};
pub mod dock;
pub mod sortable;

use gpui_kit::{
    AnyView, App, AppContext, Bounds, Context, CursorStyle, Entity, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render,
    StatefulInteractiveElement, StyleRefinement, Styled, Subscription, Window, div, px,
};
use std::{cell::Cell, rc::Rc};

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
        };
        let element =
            element
                .cursor(CursorStyle::OpenHand)
                .on_drag(drag, move |drag, _, window, cx| {
                    let origin = window.mouse_position();
                    drag.origin.set(origin);
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

/// Scroll a vertical viewport during a matching drag. Put this on the
/// scrollable element itself, with the same handle passed to track_scroll.
pub fn auto_scroll_vertical<T: 'static, E: InteractiveElement>(
    element: E,
    scroll: gpui_kit::ScrollHandle,
    edge: Pixels,
    speed: Pixels,
) -> E {
    element.on_drag_move::<Drag<T>>(move |event, window, cx| {
        let pointer = event.drag(cx).position(event.event.position);
        if !event.bounds.contains(&pointer) {
            return;
        }
        let mut offset = scroll.offset();
        let amount = edge_scroll(
            pointer.y,
            event.bounds.origin.y,
            event.bounds.bottom_right().y,
            edge,
            speed,
        );
        let next = (offset.y - amount).max(-scroll.max_offset().y).min(px(0.));
        if next != offset.y {
            offset.y = next;
            scroll.set_offset(offset);
            window.refresh();
        }
    })
}
