use crate::{
    DragSource, DropTarget, KeyboardDrag, KeyboardEvent, announce,
    sortable::{Selection, SortEvent, Sortable},
};
use gpui_kit::{
    AppContext, Context, EntityId, EventEmitter, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, Window,
    base::{TestSupportExt, spring},
    component::ActiveTheme,
    div, px,
};
use guiraffe_core::{GuiraffeTheme, StateKind, Variant, focus_ring, ring_visible};
use std::collections::{HashMap, HashSet};

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
        Self {
            headings,
            cards_per_row: 1,
            cards: Sortable::new(cards),
            focus,
            selected: HashSet::new(),
            keyboard: KeyboardDrag::new(),
            keyboard_source: None,
            status: "Ready".into(),
        }
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

impl Render for SortableBoard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let indicator = theme.primary_color().to_hsla();
        let motion = cx.theme().motion_tokens().spring_move.with_epsilon(0.5);
        let scope = cx.entity_id();
        let board = cx.entity();
        let columns = self.lists().iter().enumerate().map(|(list, cards)| {
            let target = |row, gap| {
                let check = board.clone();
                let commit = board.clone();
                DropTarget::attach(
                    row,
                    move |drag: &Selection, _, cx| {
                        check.read(cx).cards.accepts(scope, drag, list, gap)
                    },
                    move |mut style, _, _| {
                        style.border_color = Some(indicator);
                        style
                    },
                    move |drag, _, _, cx| {
                        commit.update(cx, |board, cx| board.commit(drag, list, gap, cx))
                    },
                )
            };
            let rows = cards.iter().enumerate().filter_map(|(index, card)| {
                let focus = self.focus.get(&card.id)?;
                let mut source = DragSource::new(self.pick(scope, list, index)?);
                source.focus = Some(focus.clone());
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
                    source.attach(row, move |_, _, cx| cx.new(|_| Preview(label.clone()))),
                    index,
                );
                let y =
                    px((index / usize::from(self.cards_per_row)) as f32 * (CARD_HEIGHT + CARD_GAP));
                let position = spring((("card", id), "sortable-position"), y, motion, window, cx);
                Some(row.top(position - y))
            });
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.headings[list].clone())
                .child(
                    div()
                        .grid()
                        .grid_cols(self.cards_per_row)
                        .gap(px(CARD_GAP))
                        .children(rows),
                )
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

    struct TestHost(gpui_kit::Entity<SortableBoard>);

    impl Render for TestHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(800.)).child(self.0.clone())
        }
    }
}
