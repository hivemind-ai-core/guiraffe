use crate::{
    KeyboardDrag, KeyboardEvent, announce, edge_scroll,
    sortable::{Selection, SortEvent, Sortable},
};
use gpui_kit::{
    App, AppContext, Bounds, Context, DispatchPhase, Entity, EntityId, EventEmitter, FocusHandle,
    Hsla, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, Render, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
    base::{TestSupportExt, spring},
    canvas,
    component::ActiveTheme,
    div, px,
};
use guiraffe_core::{GuiraffeTheme, StateKind, Variant, focus_ring, ring_visible};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

const CARD_HEIGHT: f32 = 40.;
const CARD_GAP: f32 = 8.;

/// A card's ID must be unique within its board and stable across transfers.
pub struct Card {
    pub id: usize,
    pub label: SharedString,
}

/// A sortable board with one column per list. Focus and selection follow card IDs.
pub struct SortableBoard {
    headings: Vec<SharedString>,
    scroll: Vec<ScrollHandle>,
    drag: Entity<BoardDragOverlay>,
    targets: Targets,
    cards_per_row: u16,
    cards: Sortable<Card>,
    focus: HashMap<usize, FocusHandle>,
    selected: HashSet<usize>,
    keyboard: KeyboardDrag,
    keyboard_source: Option<Selection>,
    status: SharedString,
}

impl EventEmitter<SortEvent> for SortableBoard {}

impl SortableBoard {
    pub fn new(columns: Vec<(SharedString, Vec<Card>)>, cx: &mut Context<Self>) -> Self {
        let (headings, cards): (Vec<_>, Vec<_>) = columns.into_iter().unzip();
        let focus = cards
            .iter()
            .flatten()
            .map(|card| (card.id, cx.focus_handle()))
            .collect();
        let scroll = headings.iter().map(|_| ScrollHandle::new()).collect();
        let board = cx.weak_entity();
        let targets = Targets::default();
        let drag = cx.new(|_| BoardDragOverlay {
            board,
            gesture: None,
            cancel: None,
            targets: targets.clone(),
            origin: Rc::default(),
            scroll_pending: false,
        });
        Self {
            headings,
            scroll,
            drag,
            targets,
            cards_per_row: 1,
            cards: Sortable::new(cards),
            focus,
            selected: HashSet::new(),
            keyboard: KeyboardDrag::new(),
            keyboard_source: None,
            status: "Ready".into(),
        }
    }

    /// Mount once in the board's window, as a sibling of any cached host.
    pub fn drag_overlay(&self) -> Entity<BoardDragOverlay> {
        self.drag.clone()
    }

    /// Arrange cards in each list in a grid with the given number of columns.
    pub fn with_grid(mut self, cards_per_row: u16) -> Self {
        self.cards_per_row = cards_per_row.max(1);
        self
    }

    pub fn lists(&self) -> &[Vec<Card>] {
        self.cards.lists()
    }

    fn pick(&self, scope: EntityId, list: usize, index: usize) -> Option<Selection> {
        let cards = self.lists().get(list)?;
        let card = cards.get(index)?;
        let indices = if self.selected.contains(&card.id) {
            cards
                .iter()
                .enumerate()
                .filter_map(|(i, card)| self.selected.contains(&card.id).then_some(i))
                .collect()
        } else {
            vec![index]
        };
        self.cards.selection(scope, list, &indices)
    }

    fn commit(&mut self, drag: &Selection, list: usize, gap: usize, cx: &mut Context<Self>) {
        if let Some(event) = self.cards.drop(cx.entity_id(), drag, list, gap) {
            self.status = format!(
                "Moved {} to {}, position {}",
                event.reorder.original_indices.len(),
                self.headings[list],
                event.reorder.insertion_index + 1
            )
            .into();
            cx.emit(event);
        } else {
            self.status = "Order unchanged".into();
        }
        cx.notify();
    }

    fn key(&mut self, key: &str, list: usize, index: usize, cx: &mut Context<Self>) -> bool {
        let lengths: Vec<_> = self.lists().iter().map(Vec::len).collect();
        let initial = (!self.keyboard.is_active())
            .then(|| self.pick(cx.entity_id(), list, index))
            .flatten();
        let drag = self.keyboard_source.as_ref().or(initial.as_ref());
        let gaps: Vec<_> = lengths
            .iter()
            .copied()
            .enumerate()
            .flat_map(|(list, len)| {
                (0..=len)
                    .filter(move |gap| {
                        drag.is_none_or(|drag| drag.list != list || !drag.indices.contains(gap))
                    })
                    .map(move |gap| (list, gap))
            })
            .collect();
        let original_gap =
            drag.and_then(|drag| drag.indices.first().copied())
                .map_or(index, |first| {
                    (first..=lengths[list])
                        .find(|gap| gaps.contains(&(list, *gap)))
                        .unwrap_or(index)
                });
        let source = gaps
            .iter()
            .position(|&location| location == (list, original_gap))
            .unwrap_or_default();
        let stride = usize::from(self.cards_per_row);
        let event = self.keyboard.key(
            key,
            source,
            |slot, key| {
                let (list, gap) = *gaps.get(slot)?;
                let next = match key {
                    "up" => {
                        return (0..slot)
                            .rev()
                            .take_while(|&i| gaps[i].0 == list)
                            .nth(stride - 1);
                    }
                    "down" => {
                        return (slot + 1..gaps.len())
                            .take_while(|&i| gaps[i].0 == list)
                            .nth(stride - 1);
                    }
                    "left" if stride > 1 && slot > 0 && gaps[slot - 1].0 == list => {
                        return Some(slot - 1);
                    }
                    "right"
                        if stride > 1 && gaps.get(slot + 1).is_some_and(|next| next.0 == list) =>
                    {
                        return Some(slot + 1);
                    }
                    "left" => list.checked_sub(1)?,
                    "right" => list + 1,
                    _ => return None,
                };
                gaps.iter()
                    .enumerate()
                    .filter(|(_, (list, _))| *list == next)
                    .min_by_key(|(_, (_, candidate))| candidate.abs_diff(gap))
                    .map(|(i, _)| i)
            },
            |_| true,
        );
        match event {
            Some(KeyboardEvent::PickedUp(_)) => {
                self.keyboard_source = initial;
                self.status =
                    "Picked up. Use arrows to move, Space to drop, or Escape to cancel.".into();
            }
            Some(KeyboardEvent::Moved(slot)) => {
                let (list, gap) = gaps[slot];
                self.status = format!("{}, position {}", self.headings[list], gap + 1).into();
            }
            Some(KeyboardEvent::Dropped(slot)) => {
                if let Some(drag) = self.keyboard_source.take() {
                    let (list, gap) = gaps[slot];
                    self.commit(&drag, list, gap, cx);
                }
            }
            Some(KeyboardEvent::Cancelled) => {
                self.keyboard_source = None;
                self.status = "Move cancelled".into();
            }
            None => {
                return self.keyboard.is_active()
                    && matches!(key, "up" | "down" | "left" | "right");
            }
        }
        cx.notify();
        true
    }
}

struct Preview(SharedString);

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        div()
            .px_3()
            .py_2()
            .border_1()
            .border_color(theme.surfaces().border.to_hsla())
            .bg(theme.surfaces().surface.to_hsla())
            .child(self.0.clone())
    }
}

type Targets = Rc<RefCell<HashMap<(usize, usize), Bounds<Pixels>>>>;

struct PointerGesture {
    selection: Selection,
    origin: Point<Pixels>,
    preview: Entity<Preview>,
    position: Option<Point<Pixels>>,
    target: Option<(usize, usize, Bounds<Pixels>)>,
}

/// Window-level pointer tracking and feedback for a sortable board.
pub struct BoardDragOverlay {
    board: WeakEntity<SortableBoard>,
    gesture: Option<PointerGesture>,
    cancel: Option<Subscription>,
    targets: Targets,
    origin: Rc<Cell<Point<Pixels>>>,
    scroll_pending: bool,
}

impl BoardDragOverlay {
    fn target_at(
        &self,
        selection: &Selection,
        position: Point<Pixels>,
        cx: &App,
    ) -> Option<(usize, usize, Bounds<Pixels>)> {
        let board = self.board.upgrade()?;
        self.targets
            .borrow()
            .iter()
            .find_map(|(&(list, gap), &bounds)| {
                (bounds.contains(&position)
                    && board
                        .read(cx)
                        .cards
                        .accepts(board.entity_id(), selection, list, gap))
                .then_some((list, gap, bounds))
            })
    }

    fn moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut drag) = self.gesture.take() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.cancel = None;
            cx.notify();
            return;
        }
        let delta = event.position - drag.origin;
        if drag.position.is_some() || delta.x.abs().max(delta.y.abs()) >= px(5.) {
            drag.position = Some(event.position);
            drag.target = self.target_at(&drag.selection, event.position, cx);
            cx.notify();
        }
        let active = drag.position.is_some();
        self.gesture = Some(drag);
        if active && !self.scroll_pending {
            self.scroll(window, cx);
        }
    }

    fn scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_pending = false;
        let Some(drag) = &self.gesture else { return };
        let Some(position) = drag.position else {
            return;
        };
        let Some(board) = self.board.upgrade() else {
            return;
        };
        let next_target = self.target_at(&drag.selection, position, cx);
        if drag.target != next_target
            && let Some(drag) = &mut self.gesture
        {
            drag.target = next_target;
            cx.notify();
        }
        let mut scrolled = false;
        for handle in &board.read(cx).scroll {
            let bounds = handle.bounds();
            if !bounds.contains(&position) {
                continue;
            }
            let mut offset = handle.offset();
            let next = (offset.y
                - edge_scroll(position.y, bounds.top(), bounds.bottom(), px(28.), px(10.)))
            .max(-handle.max_offset().y)
            .min(px(0.));
            if next != offset.y {
                offset.y = next;
                handle.set_offset(offset);
                scrolled = true;
            }
            break;
        }
        if scrolled {
            App::notify(cx, board.entity_id());
            self.scroll_pending = true;
            let overlay = cx.weak_entity();
            window.on_next_frame(move |window, cx| {
                let _ = overlay.update(cx, |overlay, cx| overlay.scroll(window, cx));
            });
        }
    }

    fn released(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(drag) = self.gesture.take() else {
            return;
        };
        self.cancel = None;
        cx.notify();
        if drag.position.is_some()
            && let Some((list, gap, _)) = self.target_at(&drag.selection, event.position, cx)
            && let Some(board) = self.board.upgrade()
        {
            board.update(cx, |board, cx| board.commit(&drag.selection, list, gap, cx));
        }
    }
}

impl Render for BoardDragOverlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let movement = cx.weak_entity();
        let release = movement.clone();
        let color: Hsla = GuiraffeTheme::global(cx).primary_color().to_hsla();
        let origin = self.origin.clone();
        let mut layer = div().absolute().top_0().left_0().size_full().child(
            canvas(
                move |bounds, _, _| origin.set(bounds.origin),
                move |_, _, window, _| {
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = movement.update(cx, |drag, cx| drag.moved(event, window, cx));
                        }
                    });
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = release.update(cx, |drag, cx| drag.released(event, cx));
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        );
        if let Some(drag) = &self.gesture
            && let Some(position) = drag.position
        {
            if let Some((_, _, bounds)) = drag.target {
                layer = layer.child(
                    div()
                        .absolute()
                        .left(bounds.left() - self.origin.get().x)
                        .top(bounds.top() - self.origin.get().y)
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .border_1()
                        .border_color(color),
                );
            }
            layer = layer.child(
                div()
                    .absolute()
                    .left(position.x - self.origin.get().x + px(8.))
                    .top(position.y - self.origin.get().y + px(8.))
                    .child(drag.preview.clone()),
            );
        }
        layer
    }
}

impl Render for SortableBoard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let motion = cx.theme().motion_tokens().spring_move.with_epsilon(0.5);
        let scope = cx.entity_id();
        let drag = self.drag.clone();
        let targets = self.targets.clone();
        targets.borrow_mut().clear();
        let columns = self.lists().iter().enumerate().map(|(list, cards)| {
            let target =
                |row: gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>, gap| {
                    let targets = targets.clone();
                    row.child(
                        canvas(
                            move |bounds, window, _| {
                                targets.borrow_mut().insert(
                                    (list, gap),
                                    bounds.intersect(&window.content_mask().bounds),
                                );
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                };
            let rows = cards.iter().enumerate().filter_map(|(index, card)| {
                let focus = self.focus.get(&card.id)?;
                let selection = self.pick(scope, list, index)?;
                let source = drag.clone();
                let source_focus = focus.clone();
                let id = card.id;
                let label = card.label.clone();
                let selected = self.selected.contains(&id);
                let row = div()
                    .id(("card", id))
                    .test_support()
                    .role(Role::ListItem)
                    .aria_label(label.clone())
                    .aria_selected(selected)
                    .aria_keyshortcuts("Space ArrowUp ArrowDown ArrowLeft ArrowRight Escape")
                    .track_focus(focus)
                    .tab_stop(true)
                    .border_1()
                    .border_color(theme.surfaces().border.to_hsla())
                    .h(px(CARD_HEIGHT))
                    .flex()
                    .items_center()
                    .px_3()
                    .child(if selected { "✓ " } else { "" })
                    .child(label.clone())
                    .on_mouse_down(
                        gpui_kit::MouseButton::Left,
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, _, cx| {
                            if event.modifiers.shift {
                                if !this.selected.remove(&id) {
                                    this.selected.insert(id);
                                }
                                cx.notify();
                            }
                        }),
                    )
                    .on_key_down(cx.listener(
                        move |this, event: &gpui_kit::KeyDownEvent, _, cx| {
                            if this.key(&event.keystroke.key, list, index, cx) {
                                cx.stop_propagation();
                            }
                        },
                    ));
                let row = focus_ring(
                    row,
                    ring_visible(focus, window),
                    px(0.),
                    theme
                        .state(StateKind::FocusRing, Variant::Default, None)
                        .border,
                );
                let row = target(
                    row.cursor(gpui_kit::CursorStyle::OpenHand).on_mouse_down(
                        MouseButton::Left,
                        move |event, window, cx| {
                            if event.modifiers.shift {
                                return;
                            }
                            source_focus.focus(window, cx);
                            let overlay = source.downgrade();
                            let window_id = window.window_handle().window_id();
                            let cancel = cx.intercept_keystrokes(move |event, window, cx| {
                                if window.window_handle().window_id() == window_id
                                    && event.keystroke.key == "escape"
                                    && overlay
                                        .update(cx, |overlay, cx| {
                                            overlay.cancel = None;
                                            let cleared = overlay.gesture.take().is_some();
                                            if cleared {
                                                cx.notify();
                                            }
                                            cleared
                                        })
                                        .unwrap_or(false)
                                {
                                    cx.stop_propagation();
                                }
                            });
                            let preview = cx.new(|_| Preview(label.clone()));
                            source.update(cx, |drag, _| {
                                drag.cancel = Some(cancel);
                                drag.gesture = Some(PointerGesture {
                                    selection: selection.clone(),
                                    origin: event.position,
                                    preview,
                                    position: None,
                                    target: None,
                                });
                            });
                        },
                    ),
                    index,
                );
                let y =
                    px((index / usize::from(self.cards_per_row)) as f32 * (CARD_HEIGHT + CARD_GAP));
                let position = spring((("card", id), "sortable-position"), y, motion, window, cx);
                Some(row.top(position - y))
            });
            let viewport = div()
                .id(("card-viewport", list))
                .max_h(px(300.))
                .overflow_y_scroll()
                .track_scroll(&self.scroll[list])
                .child(
                    div()
                        .grid()
                        .grid_cols(self.cards_per_row)
                        .gap(px(CARD_GAP))
                        .children(rows),
                );
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.headings[list].clone())
                .child(viewport)
                .child(target(
                    div()
                        .id(("end", list))
                        .test_support()
                        .border_1()
                        .border_color(theme.surfaces().border.to_hsla())
                        .px_3()
                        .py_2()
                        .child("Drop at end"),
                    cards.len(),
                ))
        });
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().flex().gap_4().children(columns))
            .child(announce(
                div()
                    .id("sort-status")
                    .role(Role::Status)
                    .child(self.status.clone()),
                self.status.clone(),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, test::TestWindowExt};
    use std::{cell::RefCell, rc::Rc};

    #[gpui_kit::test]
    fn mouse_and_keyboard_transfer_emit_events(cx: &mut TestAppContext) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let log = events.clone();
        let board = cx.update(|cx| {
            guiraffe_core::init(cx);
            let board = cx.new(|cx| {
                SortableBoard::new(
                    vec![
                        (
                            "Backlog".into(),
                            vec![Card {
                                id: 1,
                                label: "Card one".into(),
                            }],
                        ),
                        ("Done".into(), vec![]),
                    ],
                    cx,
                )
            });
            cx.subscribe(&board, move |_, event: &SortEvent, _| {
                log.borrow_mut().push(event.clone())
            })
            .detach();
            board
        });
        let view = board.clone();
        let (_, window) = cx.add_window_view(move |_, _| TestHost(view));
        window.run_until_parked();
        window.update(|window, cx| {
            window.drag_to(("card", 1_usize), ("end", 1_usize), cx);
        });
        window.run_until_parked();
        assert_eq!(events.borrow().len(), 1);
        assert_eq!(events.borrow()[0].destination, 1);

        window.update(|window, cx| {
            let focus = board.read(cx).focus[&1].clone();
            focus.focus(window, cx);
            window.render_frame(cx);
        });
        window.simulate_keystrokes("space left escape space left space");
        window.run_until_parked();
        assert_eq!(events.borrow().len(), 2);
        assert_eq!(events.borrow()[1].destination, 0);
        board.read_with(window, |board, _| assert_eq!(board.lists()[0][0].id, 1));
    }

    #[gpui_kit::test]
    fn pointer_drag_move_preserves_cached_host(cx: &mut TestAppContext) {
        use gpui_kit::{MouseButton, StyleRefinement, point};
        use std::cell::Cell;

        struct CachedHost {
            board: gpui_kit::Entity<SortableBoard>,
            renders: Rc<Cell<usize>>,
        }

        impl Render for CachedHost {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                self.renders.set(self.renders.get() + 1);
                div().size_full().child(self.board.clone())
            }
        }

        struct Root(Entity<CachedHost>, Entity<BoardDragOverlay>);

        impl Render for Root {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .size_full()
                    .child(
                        self.0
                            .clone()
                            .cached(StyleRefinement::default().w(px(800.)).h(px(320.))),
                    )
                    .child(self.1.clone())
            }
        }

        let renders = Rc::new(Cell::new(0));
        let counter = renders.clone();
        let (root, window) = cx.add_window_view(move |_, cx| {
            guiraffe_core::init(cx);
            let board = cx.new(|cx| {
                SortableBoard::new(
                    vec![
                        (
                            "Backlog".into(),
                            vec![Card {
                                id: 1,
                                label: "Card one".into(),
                            }],
                        ),
                        ("Done".into(), vec![]),
                    ],
                    cx,
                )
            });
            let overlay = board.read(cx).drag_overlay();
            Root(
                cx.new(|_| CachedHost {
                    board,
                    renders: counter,
                }),
                overlay,
            )
        });
        let from = window.update(|window, cx| {
            window.render_frame(cx);
            window.find(("card", 1_usize)).bounds().center()
        });
        window.simulate_mouse_move(from, None, Default::default());
        window.simulate_mouse_down(from, MouseButton::Left, Default::default());
        let start = from + point(px(18.), px(0.));
        window.simulate_mouse_move(start, Some(MouseButton::Left), Default::default());
        window.update(|window, cx| {
            assert!(
                root.read(cx)
                    .1
                    .read(cx)
                    .gesture
                    .as_ref()
                    .is_some_and(|drag| drag.position.is_some())
            );
            let _ = window.draw(cx);
        });
        let destination =
            window.update(|window, _| window.find(("end", 1_usize)).bounds().center());
        let before = renders.get();
        window.simulate_mouse_move(destination, Some(MouseButton::Left), Default::default());
        window.update(|window, cx| {
            let root = root.read(cx);
            assert_eq!(
                root.1
                    .read(cx)
                    .gesture
                    .as_ref()
                    .and_then(|drag| drag.target)
                    .map(|(list, gap, _)| (list, gap)),
                Some((1, 0)),
            );
            let _ = window.draw(cx);
        });
        assert_eq!(
            renders.get(),
            before,
            "mouse movement during a drag rendered its host"
        );
        window.simulate_keystrokes("escape");
        window.simulate_mouse_up(destination, MouseButton::Left, Default::default());
        window.update(|_, cx| {
            let root = root.read(cx);
            assert!(root.1.read(cx).gesture.is_none());
            assert_eq!(root.0.read(cx).board.read(cx).lists()[0][0].id, 1);
        });
    }

    struct TestHost(gpui_kit::Entity<SortableBoard>);

    impl Render for TestHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(div().w(px(800.)).child(self.0.clone()))
                .child(self.0.read(cx).drag_overlay())
        }
    }
}
