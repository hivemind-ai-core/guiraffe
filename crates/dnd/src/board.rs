use crate::{
    KeyboardDrag, KeyboardEvent, announce,
    chrome::{DropOutline, Indicator, IndicatorStyle, Preview, radius, set_indicators, shadows},
    edge_scroll,
    sortable::{Selection, SortEvent, Sortable},
};
use gpui_kit::{
    App, AppContext, Bounds, Context, CursorStyle, DispatchPhase, Entity, EntityId, EventEmitter,
    FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
    assets::IconName,
    base::{ElementExt, TestSupportExt, spring},
    canvas,
    component::ActiveTheme,
    div, point,
    prelude::FluentBuilder,
    px, size,
};
use guiraffe_core::{
    ColorName, GuiraffeTheme, RING_OFFSET, RING_WIDTH, Size, StateColors, StateKind, Variant,
    focus_ring, ring_visible,
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

const CARD_HEIGHT: f32 = 40.;
const CARD_GAP: f32 = 8.;
/// Columns stretch to the tallest one and at least this tall, so there is always room
/// to drop below the last card.
const MIN_COLUMN_HEIGHT: f32 = 200.;

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
    external_drag_overlay: Cell<bool>,
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
        let origin = Rc::new(Cell::new(Point::default()));
        let outline = cx.new(|_| DropOutline::new(origin.clone()));
        let drag = cx.new(|_| BoardDragOverlay {
            board,
            gesture: None,
            cancel: None,
            targets: targets.clone(),
            origin,
            outline,
            scroll_pending: false,
        });
        Self {
            headings,
            scroll,
            drag,
            external_drag_overlay: Cell::new(false),
            targets,
            cards_per_row: 1,
            cards: Sortable::new(cards),
            focus,
            selected: HashSet::new(),
            keyboard: KeyboardDrag::new(),
            keyboard_source: None,
            status: IDLE_HINT.into(),
        }
    }

    /// Whether a pointer or keyboard move is in progress.
    pub fn is_dragging(&self, cx: &App) -> bool {
        self.keyboard.is_active() || self.drag.read(cx).is_pointer_dragging()
    }

    /// Feedback for a drop at `gap` of `list`: the column highlighted as the active drop
    /// target, and an insertion line between its cards.
    fn indicators(&self, list: usize, gap: usize) -> Vec<Indicator> {
        let measured = self.targets.borrow();
        let Some(&column) = measured.columns.get(&list) else {
            return Vec::new();
        };
        let len = self.lists().get(list).map_or(0, Vec::len);
        let cards: Vec<_> = (0..len)
            .map_while(|index| measured.cards.get(&(list, index)).copied())
            .collect();
        let viewport = self
            .scroll
            .get(list)
            .map_or(column, |scroll| scroll.bounds());
        let mut indicators = vec![(column, IndicatorStyle::Zone)];
        if len > 0 && cards.len() == len {
            indicators.push((
                insertion_line(&cards, gap, viewport, self.cards_per_row),
                IndicatorStyle::Line,
            ));
        }
        indicators
    }

    /// Show the keyboard move's destination with the same feedback a pointer drag uses.
    fn show_keyboard_target(&self, target: Option<(usize, usize)>, cx: &mut Context<Self>) {
        let indicators = target.map_or_else(Vec::new, |(list, gap)| self.indicators(list, gap));
        let outline = self.drag.read(cx).outline.clone();
        set_indicators(&outline, indicators, None, cx);
    }

    /// Take responsibility for mounting the drag overlay before the first render.
    ///
    /// Mount it once in the board's window, as a sibling of any cached host.
    /// Otherwise the board renders its own overlay.
    pub fn drag_overlay(&self) -> Entity<BoardDragOverlay> {
        self.external_drag_overlay.set(true);
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
                self.show_keyboard_target(None, cx);
            }
            Some(KeyboardEvent::Moved(slot)) => {
                let (list, gap) = gaps[slot];
                self.status = format!("{}, position {}", self.headings[list], gap + 1).into();
                self.show_keyboard_target(Some((list, gap)), cx);
            }
            Some(KeyboardEvent::Dropped(slot)) => {
                if let Some(drag) = self.keyboard_source.take() {
                    let (list, gap) = gaps[slot];
                    self.commit(&drag, list, gap, cx);
                }
                self.show_keyboard_target(None, cx);
            }
            Some(KeyboardEvent::Cancelled) => {
                self.keyboard_source = None;
                self.status = "Move cancelled".into();
                self.show_keyboard_target(None, cx);
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

/// Status line shown before any move: the keyboard affordance (spec/00 §12.4).
const IDLE_HINT: &str = "Drag a card to move it, or focus it and press Space to pick it up.";

/// The insertion gap for a pointer over a column whose cards (in list order) have `cards`
/// bounds: before the first card the pointer is above (left of, in a grid) the middle of,
/// or the end of the list. Space below the last card, or an empty column, means the end.
pub fn gap_at(cards: &[Bounds<Pixels>], pointer: Point<Pixels>, cards_per_row: u16) -> usize {
    cards
        .iter()
        .position(|card| {
            if cards_per_row > 1 {
                pointer.y < card.top() || (pointer.y < card.bottom() && pointer.x < card.center().x)
            } else {
                pointer.y < card.center().y
            }
        })
        .unwrap_or(cards.len())
}

/// A 2px insertion line centred in the gap before card `gap` (after the last card for the
/// end of the list), kept inside the column's `viewport`.
fn insertion_line(
    cards: &[Bounds<Pixels>],
    gap: usize,
    viewport: Bounds<Pixels>,
    cards_per_row: u16,
) -> Bounds<Pixels> {
    let thickness = px(2.);
    let inset = px(CARD_GAP / 2.) + thickness / 2.;
    let clamp_y = |y: Pixels| y.max(viewport.top()).min(viewport.bottom() - thickness);
    let horizontal = |left: Pixels, width: Pixels, y: Pixels| {
        Bounds::new(point(left, clamp_y(y)), size(width, thickness))
    };
    match (cards.get(gap), cards.last()) {
        (Some(card), _) if cards_per_row > 1 => Bounds::new(
            point(card.left() - inset, card.top()),
            size(thickness, card.size.height),
        ),
        (Some(card), _) => horizontal(card.left(), card.size.width, card.top() - inset),
        (None, Some(last)) if cards_per_row > 1 => Bounds::new(
            point(last.right() + inset - thickness, last.top()),
            size(thickness, last.size.height),
        ),
        (None, Some(last)) => horizontal(
            last.left(),
            last.size.width,
            last.bottom() + inset - thickness,
        ),
        (None, None) => horizontal(viewport.left(), viewport.size.width, viewport.top()),
    }
}

/// Bounds measured during the board's last prepaint.
#[derive(Default)]
struct Measured {
    /// Each column's whole body: the drop target for that list.
    columns: HashMap<usize, Bounds<Pixels>>,
    /// Unclipped card bounds by (list, index), so scrolled-away cards still order the gap.
    cards: HashMap<(usize, usize), Bounds<Pixels>>,
}

type Targets = Rc<RefCell<Measured>>;

struct PointerGesture {
    selection: Selection,
    origin: Point<Pixels>,
    preview: Entity<Preview>,
    position: Option<Point<Pixels>>,
    target: Option<(usize, usize)>,
}

/// Window-level pointer tracking and feedback for a sortable board.
pub struct BoardDragOverlay {
    board: WeakEntity<SortableBoard>,
    gesture: Option<PointerGesture>,
    cancel: Option<Subscription>,
    targets: Targets,
    origin: Rc<Cell<Point<Pixels>>>,
    outline: Entity<DropOutline>,
    scroll_pending: bool,
}

impl BoardDragOverlay {
    fn outline(&self, target: Option<(usize, usize)>, cx: &mut App) {
        let indicators = target
            .zip(self.board.upgrade())
            .map_or_else(Vec::new, |((list, gap), board)| {
                board.read(cx).indicators(list, gap)
            });
        set_indicators(&self.outline, indicators, None, cx);
    }

    /// Re-render the board when a pointer drag starts or ends: it drops the cards' hover and
    /// pressed styles for the drag, so pointer moves never re-render it (or its host).
    fn notify_board(&self, cx: &mut App) {
        if let Some(board) = self.board.upgrade() {
            App::notify(cx, board.entity_id());
        }
    }

    /// Whether a pointer drag has passed its threshold.
    pub fn is_pointer_dragging(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|drag| drag.position.is_some())
    }

    /// The column under the pointer and the insertion gap among its cards.
    fn target_at(
        &self,
        selection: &Selection,
        position: Point<Pixels>,
        cx: &App,
    ) -> Option<(usize, usize)> {
        let board = self.board.upgrade()?;
        let scope = board.entity_id();
        let board = board.read(cx);
        let measured = self.targets.borrow();
        let list = measured
            .columns
            .iter()
            .find_map(|(&list, bounds)| bounds.contains(&position).then_some(list))?;
        let len = board.lists().get(list)?.len();
        let cards: Vec<_> = (0..len)
            .map_while(|index| measured.cards.get(&(list, index)).copied())
            .collect();
        let gap = gap_at(&cards, position, board.cards_per_row);
        board
            .cards
            .accepts(scope, selection, list, gap)
            .then_some((list, gap))
    }

    fn moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut drag) = self.gesture.take() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.cancel = None;
            self.outline(None, cx);
            cx.notify();
            if drag.position.is_some() {
                self.notify_board(cx);
            }
            return;
        }
        let delta = event.position - drag.origin;
        if drag.position.is_some() || delta.x.abs().max(delta.y.abs()) >= px(5.) {
            let started = drag.position.replace(event.position).is_none();
            drag.target = self.target_at(&drag.selection, event.position, cx);
            self.outline(drag.target, cx);
            drag.preview.update(cx, |preview, cx| {
                preview.position = event.position;
                cx.notify();
            });
            if started {
                cx.notify();
                self.notify_board(cx);
            }
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
            self.outline(next_target, cx);
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
        self.outline(None, cx);
        cx.notify();
        if drag.position.is_some() {
            self.notify_board(cx);
        }
        if drag.position.is_some()
            && let Some((list, gap)) = self.target_at(&drag.selection, event.position, cx)
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
        let origin = self.origin.clone();
        let grabbing = self.is_pointer_dragging();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                canvas(
                    move |bounds, _, _| origin.set(bounds.origin),
                    move |_, _, window, _| {
                        if grabbing {
                            window.set_window_cursor_style(CursorStyle::ClosedHand);
                        }
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ =
                                    movement.update(cx, |drag, cx| drag.moved(event, window, cx));
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
            )
            .child(self.outline.clone())
            .children(
                self.gesture
                    .as_ref()
                    .filter(|drag| drag.position.is_some())
                    .map(|drag| drag.preview.clone()),
            )
    }
}

impl Render for SortableBoard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let motion = cx.theme().motion_tokens().spring_move.with_epsilon(0.5);
        let scope = cx.entity_id();
        let drag = self.drag.clone();
        let targets = self.targets.clone();
        *targets.borrow_mut() = Measured::default();
        let surfaces = theme.surfaces();
        let corner = radius(&theme);
        let card_colors = theme.press_colors(Variant::Default, Some(ColorName::Gray));
        let selected_colors = theme.state(StateKind::Selected, Variant::Light, None);
        let ring = theme
            .state(StateKind::FocusRing, Variant::Default, None)
            .border;
        let pointer_drag = self.drag.read(cx).is_pointer_dragging();
        let lifted: HashSet<usize> = self
            .keyboard_source
            .as_ref()
            .and_then(|source| {
                let cards = self.lists().get(source.list)?;
                Some(
                    source
                        .indices
                        .iter()
                        .filter_map(|&i| cards.get(i).map(|card| card.id))
                        .collect(),
                )
            })
            .unwrap_or_default();
        let columns = self.lists().iter().enumerate().map(|(list, cards)| {
            let measure_card =
                |row: gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>, index| {
                    let targets = targets.clone();
                    row.on_prepaint(move |bounds, _, _| {
                        targets.borrow_mut().cards.insert((list, index), bounds);
                    })
                };
            let rows = cards.iter().enumerate().filter_map(|(index, card)| {
                let focus = self.focus.get(&card.id)?;
                let selection = self.pick(scope, list, index)?;
                let source = drag.clone();
                let source_focus = focus.clone();
                let id = card.id;
                let label = card.label.clone();
                let preview_label = match selection.indices.len() {
                    0 | 1 => label.clone(),
                    n => SharedString::from(format!("{label} +{}", n - 1)),
                };
                let selected = self.selected.contains(&id);
                let is_lifted = lifted.contains(&id);
                let rest = if selected {
                    selected_colors
                } else {
                    StateColors {
                        background: surfaces.raised,
                        foreground: surfaces.text,
                        border: surfaces.border,
                    }
                };
                let (hover, pressed) =
                    (card_colors.hover.background, card_colors.pressed.background);
                let row = div()
                    .id(("card", id))
                    .test_support()
                    .role(Role::ListItem)
                    .aria_label(label.clone())
                    .aria_selected(selected)
                    .aria_keyshortcuts("Space ArrowUp ArrowDown ArrowLeft ArrowRight Escape")
                    .track_focus(focus)
                    .tab_stop(true)
                    .rounded(corner)
                    .border_1()
                    .border_color(if is_lifted || selected {
                        theme.primary_color().to_hsla()
                    } else {
                        rest.border.to_hsla()
                    })
                    .bg(rest.background.to_hsla())
                    .text_color(rest.foreground.to_hsla())
                    .shadow(shadows(&theme, if is_lifted { Size::Md } else { Size::Xs }))
                    .when(!selected && !pointer_drag, |row| {
                        row.hover(move |style| style.bg(hover.over(surfaces.raised).to_hsla()))
                            .active(move |style| style.bg(pressed.to_hsla()))
                    })
                    .h(px(CARD_HEIGHT))
                    .text_sm()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .when(selected, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_color(theme.primary_color().to_hsla())
                                .child(IconName::Check),
                        )
                    })
                    .child(div().min_w_0().truncate().child(label.clone()))
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
                let row = focus_ring(row, ring_visible(focus, window), corner, ring);
                let row = measure_card(
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
                                            let gesture = overlay.gesture.take();
                                            let cleared = gesture.is_some();
                                            if cleared {
                                                overlay.outline(None, cx);
                                                cx.notify();
                                            }
                                            if gesture.is_some_and(|drag| drag.position.is_some()) {
                                                overlay.notify_board(cx);
                                            }
                                            cleared
                                        })
                                        .unwrap_or(false)
                                {
                                    cx.stop_propagation();
                                }
                            });
                            source.update(cx, |drag, cx| {
                                let preview = cx.new(|_| Preview {
                                    label: preview_label.clone(),
                                    position: event.position,
                                    origin: drag.origin.clone(),
                                });
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
            // The focus ring sits outside a card's box; the inset keeps it unclipped.
            let ring_room = px(RING_OFFSET + RING_WIDTH);
            let viewport = div()
                .id(("card-viewport", list))
                .max_h(px(300.))
                .overflow_y_scroll()
                .track_scroll(&self.scroll[list])
                .when(!cards.is_empty(), |viewport| {
                    viewport.child(
                        div()
                            .grid()
                            .grid_cols(self.cards_per_row)
                            .gap(px(CARD_GAP))
                            .p(ring_room)
                            .children(rows),
                    )
                });
            // An empty column says so; the whole column is the drop target either way.
            let empty = cards.is_empty().then(|| {
                div()
                    .id(("empty", list))
                    .test_support()
                    .flex_1()
                    .min_h(px(CARD_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(surfaces.dimmed.to_hsla())
                    .child("No cards")
            });
            let column_targets = targets.clone();
            div()
                .id(("column", list))
                .test_support()
                .role(Role::List)
                .aria_label(self.headings[list].clone())
                .relative()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded(corner)
                .bg(surfaces.surface.to_hsla())
                .border_1()
                .border_color(surfaces.border.to_hsla())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(ring_room)
                        .pb_1()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.headings[list].clone()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(surfaces.dimmed.to_hsla())
                                .child(cards.len().to_string()),
                        ),
                )
                .child(viewport)
                .children(empty)
                .on_prepaint(move |bounds, window, _| {
                    column_targets
                        .borrow_mut()
                        .columns
                        .insert(list, bounds.intersect(&window.content_mask().bounds));
                })
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .gap_4()
                    .min_h(px(MIN_COLUMN_HEIGHT))
                    .children(columns),
            )
            .child(announce(
                div()
                    .id("sort-status")
                    .role(Role::Status)
                    .text_sm()
                    .text_color(surfaces.dimmed.to_hsla())
                    .child(self.status.clone()),
                self.status.clone(),
            ))
            .children((!self.external_drag_overlay.get()).then(|| self.drag.clone()))
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
            window.drag_to(("card", 1_usize), ("column", 1_usize), cx);
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
                    .child(
                        self.1
                            .clone()
                            .cached(StyleRefinement::default().absolute().size_full()),
                    )
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
            window.update(|window, _| window.find(("column", 1_usize)).bounds().center());
        let before = renders.get();
        window.simulate_mouse_move(destination, Some(MouseButton::Left), Default::default());
        window.run_until_parked();
        window.update(|_, cx| {
            let root = root.read(cx);
            assert_eq!(
                root.1
                    .read(cx)
                    .gesture
                    .as_ref()
                    .and_then(|drag| drag.target),
                Some((1, 0)),
            );
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

    fn bounds(x: f32, y: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(100.), px(CARD_HEIGHT)))
    }

    #[test]
    fn gap_follows_the_pointer_between_cards() {
        let cards = [bounds(0., 0.), bounds(0., 48.), bounds(0., 96.)];
        let at = |y: f32| gap_at(&cards, point(px(50.), px(y)), 1);
        assert_eq!(at(-10.), 0);
        assert_eq!(at(10.), 0);
        assert_eq!(at(30.), 1, "lower half of the first card inserts after it");
        assert_eq!(at(60.), 1);
        assert_eq!(at(130.), 3);
        assert_eq!(at(400.), 3, "space below the last card is the end");
        assert_eq!(gap_at(&[], point(px(0.), px(0.)), 1), 0, "an empty column");
        // Grid: left half before the card, right half after it, row by row.
        let grid = [bounds(0., 0.), bounds(110., 0.), bounds(0., 48.)];
        let at = |x: f32, y: f32| gap_at(&grid, point(px(x), px(y)), 2);
        assert_eq!(at(20., 20.), 0);
        assert_eq!(at(80., 20.), 1);
        assert_eq!(at(190., 20.), 2);
        assert_eq!(at(20., 60.), 2);
        assert_eq!(at(80., 60.), 3);
    }

    #[test]
    fn insertion_line_sits_in_the_gap_and_inside_the_viewport() {
        let cards = [bounds(0., 10.), bounds(0., 58.)];
        let viewport = Bounds::new(point(px(0.), px(10.)), size(px(100.), px(88.)));
        let before_second = insertion_line(&cards, 1, viewport, 1);
        assert_eq!(before_second.size.height, px(2.));
        assert!(before_second.top() > cards[0].bottom() && before_second.bottom() < cards[1].top());
        let first = insertion_line(&cards, 0, viewport, 1);
        assert_eq!(first.top(), viewport.top(), "clamped into the viewport");
        let end = insertion_line(&cards, 2, viewport, 1);
        assert!(end.bottom() <= viewport.bottom());
        let grid = insertion_line(&cards, 1, viewport, 2);
        assert_eq!(grid.size.width, px(2.));
    }

    fn board_with(
        columns: Vec<(&'static str, Vec<usize>)>,
        cx: &mut Context<SortableBoard>,
    ) -> SortableBoard {
        SortableBoard::new(
            columns
                .into_iter()
                .map(|(heading, ids)| {
                    (
                        heading.into(),
                        ids.into_iter()
                            .map(|id| Card {
                                id,
                                label: format!("Card {id}").into(),
                            })
                            .collect(),
                    )
                })
                .collect(),
            cx,
        )
    }

    fn pointer_drag(
        window: &mut gpui_kit::VisualTestContext,
        from: Point<Pixels>,
        to: Point<Pixels>,
        release: bool,
    ) {
        window.simulate_mouse_move(from, None, Default::default());
        window.simulate_mouse_down(from, MouseButton::Left, Default::default());
        window.simulate_mouse_move(
            from + point(px(12.), px(0.)),
            Some(MouseButton::Left),
            Default::default(),
        );
        window.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
        window.run_until_parked();
        if release {
            window.simulate_mouse_up(to, MouseButton::Left, Default::default());
            window.run_until_parked();
        }
    }

    fn ids(board: &SortableBoard) -> Vec<Vec<usize>> {
        board
            .lists()
            .iter()
            .map(|cards| cards.iter().map(|card| card.id).collect())
            .collect()
    }

    /// Let the cards' position springs finish so hit tests see final positions.
    fn settle(window: &mut gpui_kit::VisualTestContext) {
        for _ in 0..60 {
            window
                .executor()
                .advance_clock(std::time::Duration::from_millis(32));
            window.update(|window, cx| window.render_frame(cx));
            window.run_until_parked();
        }
    }

    fn element_bounds(
        window: &mut gpui_kit::VisualTestContext,
        id: (&'static str, usize),
    ) -> Bounds<Pixels> {
        settle(window);
        window.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds()
        })
    }

    #[gpui_kit::test]
    fn columns_are_the_drop_targets(cx: &mut TestAppContext) {
        let board = cx.update(|cx| {
            guiraffe_core::init(cx);
            cx.new(|cx| {
                board_with(
                    vec![
                        ("Backlog", vec![1, 2, 3]),
                        ("Doing", vec![4]),
                        ("Done", vec![]),
                    ],
                    cx,
                )
            })
        });
        let view = board.clone();
        let (_, window) = cx.add_window_view(move |_, _| TestHost(view));
        // Empty space below the last card of a column lands at its end.
        let from = element_bounds(window, ("card", 1)).center();
        let doing = element_bounds(window, ("column", 1));
        pointer_drag(
            window,
            from,
            point(doing.center().x, doing.bottom() - px(6.)),
            true,
        );
        board.read_with(window, |board, _| {
            assert_eq!(ids(board), [vec![2, 3], vec![4, 1], vec![]])
        });
        // An empty column takes the card.
        let from = element_bounds(window, ("card", 2)).center();
        let done = element_bounds(window, ("column", 2));
        pointer_drag(window, from, done.center(), true);
        board.read_with(window, |board, _| {
            assert_eq!(ids(board), [vec![3], vec![4, 1], vec![2]])
        });
        // The upper half of a card inserts before it.
        let from = element_bounds(window, ("card", 3)).center();
        let target = element_bounds(window, ("card", 1));
        pointer_drag(
            window,
            from,
            point(target.center().x, target.top() + px(4.)),
            true,
        );
        board.read_with(window, |board, _| {
            assert_eq!(ids(board), [vec![], vec![4, 3, 1], vec![2]])
        });
    }

    #[gpui_kit::test]
    fn drop_feedback_shows_only_while_dragging(cx: &mut TestAppContext) {
        let board = cx.update(|cx| {
            guiraffe_core::init(cx);
            cx.new(|cx| board_with(vec![("Backlog", vec![1, 2]), ("Done", vec![])], cx))
        });
        let view = board.clone();
        let (_, window) = cx.add_window_view(move |_, _| TestHost(view));
        let from = element_bounds(window, ("card", 1)).center();
        let done = element_bounds(window, ("column", 1));
        let second = element_bounds(window, ("card", 2));
        let indicators = |window: &mut gpui_kit::VisualTestContext| {
            window.update(|_, cx| {
                let overlay = board.read(cx).drag.clone();
                let outline = overlay.read(cx).outline.clone();
                outline.read(cx).indicators.clone()
            })
        };
        // At rest: no drop feedback, and only the empty column shows its placeholder.
        assert!(indicators(window).is_empty());
        window.update(|window, _| {
            assert!(window.find_all(("empty", 0_usize)).is_empty());
            assert_eq!(window.find_all(("empty", 1_usize)).len(), 1);
        });
        // Over an empty column: the column is highlighted (no line: nothing to go between).
        pointer_drag(window, from, done.center(), false);
        let shown = indicators(window);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].1, IndicatorStyle::Zone);
        assert!(
            shown[0].0.contains(&done.center()) && shown[0].0.size.width > done.size.width / 2.
        );
        // Over the lower half of the last card: column highlight plus a line after it.
        let below = point(second.center().x, second.bottom() - px(4.));
        window.simulate_mouse_move(below, Some(MouseButton::Left), Default::default());
        window.run_until_parked();
        let shown = indicators(window);
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[0].1, IndicatorStyle::Zone);
        assert!(shown[0].0.contains(&below));
        assert_eq!(shown[1].1, IndicatorStyle::Line);
        assert!(shown[1].0.top() >= second.bottom());
        // Released: the feedback goes away.
        window.simulate_mouse_up(below, MouseButton::Left, Default::default());
        window.run_until_parked();
        assert!(indicators(window).is_empty());
        // A keyboard move shows the same column feedback and clears it on drop.
        window.update(|window, cx| {
            let focus = board.read(cx).focus[&1].clone();
            focus.focus(window, cx);
            window.render_frame(cx);
        });
        window.simulate_keystrokes("space right");
        window.run_until_parked();
        assert_eq!(
            indicators(window).first().map(|indicator| indicator.1),
            Some(IndicatorStyle::Zone)
        );
        window.simulate_keystrokes("space");
        window.run_until_parked();
        assert!(indicators(window).is_empty());
        board.read_with(window, |board, _| {
            assert_eq!(ids(board), [vec![2], vec![1]])
        });
    }

    struct TestHost(gpui_kit::Entity<SortableBoard>);

    impl Render for TestHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(800.)).child(self.0.clone())
        }
    }
}
