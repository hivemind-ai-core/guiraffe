use crate::{
    ChatState, Message, Role,
    message::{Disclosures, MessageView, RowLayout},
    scroll::Stickiness,
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{prelude::*, *};
use guiraffe_core::{Size, StyleContext, Variant};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

const RETAINED_ROWS: usize = 64;

#[derive(Default)]
struct RowCache {
    recent: BTreeMap<usize, WeakEntity<MessageView>>,
    pinned: BTreeMap<usize, Vec<Subscription>>,
}

impl RowCache {
    fn use_row(
        cache: &Rc<RefCell<Self>>,
        history: &Entity<History>,
        index: usize,
        row: &Entity<MessageView>,
        cx: &mut App,
    ) {
        let mut this = cache.borrow_mut();
        if this.recent.get(&index).is_some_and(|entry| entry == row) {
            return;
        }
        this.recent.insert(index, row.downgrade());
        this.pinned.remove(&index);

        while this.recent.len() > RETAINED_ROWS {
            let extremes = [this.recent.first_key_value(), this.recent.last_key_value()];
            let Some((&victim, _)) = extremes
                .into_iter()
                .flatten()
                .filter(|(key, _)| **key != index)
                .max_by_key(|(key, _)| key.abs_diff(index))
            else {
                break;
            };
            if let Some(candidate) = this.recent.remove(&victim)
                && let Some(row) = candidate.upgrade()
            {
                defer_evict(cache, history, victim, row, cx);
            }
        }
    }
}

fn defer_evict(
    cache: &Rc<RefCell<RowCache>>,
    history: &Entity<History>,
    index: usize,
    row: Entity<MessageView>,
    cx: &mut App,
) {
    let cache = cache.clone();
    let history = history.downgrade();
    cx.defer(move |cx| {
        let mut cache = cache.borrow_mut();
        if cache.recent.contains_key(&index) {
            return;
        }
        let Some(history) = history.upgrade() else {
            return;
        };
        if history.read(cx).rows.get(index).and_then(Row::mounted) != Some(&row) {
            return;
        }
        if row.read(cx).retain_offscreen(cx) {
            cache.pinned.entry(index).or_insert_with(|| {
                let history = history.downgrade();
                let weak_row = row.downgrade();
                let release = move |cx: &mut App| {
                    if let Some(history) = history.upgrade()
                        && let Some(row) = weak_row.upgrade()
                        && !row.read(cx).retain_offscreen(cx)
                    {
                        history.update(cx, |history, cx| {
                            let mut cache = history.row_cache.borrow_mut();
                            if cache.pinned.remove(&index).is_none() {
                                return;
                            }
                            drop(cache);
                            history.evict(index, &row, cx);
                        });
                    }
                };
                let markdown = row.read(cx).selection_views(cx);
                let mut subscriptions = vec![cx.observe(&row, {
                    let release = release.clone();
                    move |_, cx| release(cx)
                })];
                for view in markdown.into_iter().flatten() {
                    let release = release.clone();
                    subscriptions.push(cx.observe(&view, move |_, cx| release(cx)));
                }
                subscriptions
            });
        } else {
            history.update(cx, |history, cx| history.evict(index, &row, cx));
        }
    });
}

pub(crate) enum Row {
    Cold {
        data: Message,
        editable: bool,
        animate: bool,
        disclosures: Option<Box<Disclosures>>,
    },
    Mounted(Entity<MessageView>),
}

impl Row {
    pub(crate) fn data<'a>(&'a self, cx: &'a App) -> &'a Message {
        match self {
            Self::Cold { data, .. } => data,
            Self::Mounted(view) => &view.read(cx).data,
        }
    }

    pub(crate) fn mounted(&self) -> Option<&Entity<MessageView>> {
        match self {
            Self::Cold { .. } => None,
            Self::Mounted(view) => Some(view),
        }
    }
}

pub(crate) struct History {
    pub(crate) rows: Vec<Row>,
    owner: WeakEntity<ChatState>,
    pub(crate) list: ListState,
    pub(crate) sticky: Stickiness,
    row_cache: Rc<RefCell<RowCache>>,
    #[cfg(test)]
    pub(crate) renders: std::rc::Rc<std::cell::Cell<usize>>,
}
impl History {
    pub(crate) fn new(owner: WeakEntity<ChatState>, cx: &mut Context<Self>) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(Size::Xl.control_height() * 2.));
        list.set_follow_mode(FollowMode::Tail);
        let weak = cx.weak_entity();
        list.set_scroll_handler(move |event, _, cx| {
            let Some(history) = weak.upgrade() else {
                return;
            };
            if history.read(cx).sticky.following() == event.is_following_tail {
                return;
            }
            history.update(cx, |history, cx| {
                history.sticky.user_scroll(event.is_following_tail);
                cx.notify();
            });
        });
        Self {
            rows: vec![],
            owner,
            list,
            sticky: Stickiness::default(),
            row_cache: Default::default(),
            #[cfg(test)]
            renders: Default::default(),
        }
    }
    pub(crate) fn push(&mut self, data: Message, animate: bool, cx: &mut Context<Self>) -> usize {
        let index = self.rows.len();
        let first_pending_insert = self.list.item_count() == index;
        self.rows.push(Row::Cold {
            editable: data.role == Role::User,
            animate: animate && self.sticky.following(),
            data,
            disclosures: None,
        });
        self.sticky.appended_message();
        if first_pending_insert {
            cx.notify();
        }
        index
    }

    pub(crate) fn split_off(&mut self, index: usize) -> Vec<Row> {
        let mut cache = self.row_cache.borrow_mut();
        cache.recent.split_off(&index);
        cache.pinned.split_off(&index);
        self.rows.split_off(index)
    }

    pub(crate) fn disable_editing(&mut self, index: usize, cx: &mut Context<Self>) {
        match self.rows.get_mut(index) {
            Some(Row::Cold { editable, .. }) => *editable = false,
            Some(Row::Mounted(view)) => view.update(cx, |view, cx| {
                view.editable = false;
                if view.markdown.is_some() {
                    cx.notify();
                }
            }),
            None => {}
        }
    }

    pub(crate) fn materialize(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<Entity<MessageView>> {
        let row = self.rows.get_mut(index)?;
        let Row::Cold {
            data,
            editable,
            animate,
            disclosures,
        } = row
        else {
            return row.mounted().cloned();
        };
        let layout = RowLayout {
            index,
            list: self.list.clone(),
        };
        let view = cx.new(|cx| {
            let mut view = MessageView::new(
                data.clone(),
                self.owner.clone(),
                *animate,
                layout,
                disclosures.as_deref(),
                cx,
            );
            view.editable = *editable;
            view
        });
        *row = Row::Mounted(view.clone());
        // Replies can materialize without entering the virtual list. Give those
        // rows the same eviction path as rows materialized during rendering.
        RowCache::use_row(&self.row_cache, &cx.entity(), index, &view, cx);
        Some(view)
    }

    fn evict(&mut self, index: usize, candidate: &Entity<MessageView>, cx: &App) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };
        if row.mounted() != Some(candidate) {
            return;
        }
        let view = candidate.read(cx);
        let disclosures = view.disclosures(cx);
        *row = Row::Cold {
            data: view.data.clone(),
            editable: view.editable,
            animate: false,
            disclosures: (!disclosures.is_empty()).then(|| Box::new(disclosures)),
        };
    }

    // Register a burst of history inserts with the list in one splice.
    fn sync_list(&self) {
        let listed = self.list.item_count();
        if listed < self.rows.len() {
            self.list.splice(listed..listed, self.rows.len() - listed);
        }
    }

    pub(crate) fn jump(&mut self, cx: &mut Context<Self>) {
        self.sync_list();
        self.sticky.jump();
        self.list.scroll_to_end();
        cx.notify();
    }
}
impl Render for History {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        self.sync_list();
        let owner = cx.entity();
        let row_cache = self.row_cache.clone();
        let sc = StyleContext::current();
        div()
            .id("chat-history")
            .test_support()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                list(self.list.clone(), move |index, _, cx| {
                    let row = owner
                        .read(cx)
                        .rows
                        .get(index)
                        .and_then(Row::mounted)
                        .cloned()
                        .or_else(|| owner.update(cx, |history, cx| history.materialize(index, cx)));
                    let Some(row) = row else {
                        return div().into_any_element();
                    };
                    RowCache::use_row(&row_cache, &owner, index, &row, cx);
                    row.into_any_element()
                })
                .size_full()
                .px(px(sc.density.space(Size::Md))),
            )
            .when(!self.sticky.following(), |el| {
                el.child(
                    div()
                        .absolute()
                        .bottom(px(sc.density.space(Size::Sm)))
                        .right(px(sc.density.space(Size::Md)))
                        .child(
                            crate::style::button("jump-latest", Variant::Light, None, cx)
                                .label(if self.sticky.unread() > 0 {
                                    format!("↓ {} new · Jump to latest", self.sticky.unread())
                                } else {
                                    "↓ Jump to latest".into()
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.jump(cx))),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{History, RETAINED_ROWS, RowCache};
    use crate::{Message, MessageId, Role};
    use gpui_kit::{
        AppContext, Entity, ListOffset, Modifiers, MouseButton, Pixels, Point, TestAppContext,
        VisualTestContext, base::TextSelection, point, px,
    };

    const HISTORY_SCALE: usize = 100_000;

    #[derive(Clone, Copy, Debug)]
    enum SelectionProbe {
        HeldDrag,
        CompletedDrag,
        SelectAll,
    }

    fn draw_probe_frame(visual: &mut VisualTestContext) {
        visual.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        visual.run_until_parked();
    }

    fn scroll_probe_to(visual: &mut VisualTestContext, history: &Entity<History>, index: usize) {
        visual.update(|_, cx| {
            history.update(cx, |history, cx| {
                history.list.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: px(0.),
                });
                cx.notify();
            });
        });
        draw_probe_frame(visual);
    }

    fn start_probe_selection(
        visual: &mut VisualTestContext,
        history: &Entity<History>,
        probe: SelectionProbe,
    ) -> Point<Pixels> {
        let (start, end) = visual.update(|_, cx| {
            let history = history.read(cx);
            let text_bounds = |index: usize| {
                let row = history.rows[index]
                    .mounted()
                    .expect("visible history row")
                    .read(cx);
                row.markdown
                    .as_ref()
                    .expect("rendered Markdown")
                    .read(cx)
                    .bounds()
            };
            (
                text_bounds(0).origin + point(px(3.), px(5.)),
                text_bounds(2).origin + point(px(35.), px(5.)),
            )
        });
        assert!(start.y < end.y);
        let modifiers = Modifiers::none();
        visual.simulate_mouse_move(start, None, modifiers);
        visual.simulate_mouse_down(start, MouseButton::Left, modifiers);
        if matches!(probe, SelectionProbe::SelectAll) {
            visual.simulate_mouse_up(start, MouseButton::Left, modifiers);
            if cfg!(target_os = "macos") {
                visual.simulate_keystrokes("cmd-a");
            } else {
                visual.simulate_keystrokes("ctrl-a");
            }
        } else {
            visual.simulate_mouse_move(end, MouseButton::Left, modifiers);
            if matches!(probe, SelectionProbe::CompletedDrag) {
                visual.simulate_mouse_up(end, MouseButton::Left, modifiers);
            }
        }
        draw_probe_frame(visual);
        assert!(
            visual.update(|window, cx| TextSelection::has_selection(window, cx)),
            "{probe:?} did not select text"
        );
        end
    }

    fn probe_selection_retention(cx: &mut TestAppContext, probe: SelectionProbe) {
        let (window, chat) = crate::gpui_tests::open(cx);
        let history = cx.update(|cx| chat.read(cx).history.clone());
        cx.update(|cx| {
            history.update(cx, |history, cx| {
                for index in 0..HISTORY_SCALE {
                    history.push(
                        Message::new(
                            MessageId(index as u64),
                            Role::Assistant,
                            "selectable history row",
                        ),
                        false,
                        cx,
                    );
                }
                history.sync_list();
                history.list.pause_following_tail();
                history.sticky.user_scroll(false);
                cx.notify();
            });
        });
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        scroll_probe_to(&mut visual, &history, 0);
        let end = start_probe_selection(&mut visual, &history, probe);
        let mut peak_pins = 0;
        let mut peak_mounted = 0;
        let mut first_selection_loss = None;
        for index in (0..HISTORY_SCALE).step_by(8).chain([HISTORY_SCALE - 1]) {
            if index > 0 {
                scroll_probe_to(&mut visual, &history, index);
                if matches!(probe, SelectionProbe::HeldDrag) {
                    visual.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
                }
            }
            visual.update(|window, cx| {
                let cache = history.read(cx).row_cache.borrow();
                peak_pins = peak_pins.max(cache.pinned.len());
                peak_mounted = peak_mounted.max(cache.pinned.len() + cache.recent.len());
                drop(cache);
                if !TextSelection::has_selection(window, cx) {
                    first_selection_loss.get_or_insert(index);
                }
            });
        }
        if matches!(probe, SelectionProbe::HeldDrag) {
            visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        }
        visual.update(|window, cx| {
            let selected = TextSelection::has_selection(window, cx);
            let history = history.read(cx);
            let mounted = history.rows.iter().filter(|row| row.mounted().is_some()).count();
            println!("{probe:?}: rows={}, mounted={mounted}, peak_mounted={peak_mounted}, peak_pins={peak_pins}, first_selection_loss={first_selection_loss:?}, selection_at_end={selected}", history.rows.len());
            assert_eq!(history.rows.len(), HISTORY_SCALE);
            window.remove_window();
        });
    }

    #[gpui_kit::test]
    #[ignore = "100k-row selection retention probe, run with --ignored --nocapture"]
    fn selection_retention_across_one_hundred_thousand_rows(cx: &mut TestAppContext) {
        for probe in [
            SelectionProbe::HeldDrag,
            SelectionProbe::CompletedDrag,
            SelectionProbe::SelectAll,
        ] {
            probe_selection_retention(cx, probe);
        }
    }

    #[gpui_kit::test]
    fn superseded_offscreen_replies_respect_the_row_cache(cx: &mut TestAppContext) {
        let (_, chat) = crate::gpui_tests::open(cx);
        let history = cx.update(|cx| chat.read(cx).history.clone());
        cx.update(|cx| {
            for _ in 0..1_000 {
                chat.update(cx, |chat, cx| {
                    chat.begin_reply(cx);
                });
            }
        });
        cx.update(|cx| {
            let history = history.read(cx);
            assert_eq!(history.rows.len(), 1_000);
            assert_eq!(
                history
                    .rows
                    .iter()
                    .filter(|row| row.mounted().is_some())
                    .count(),
                RETAINED_ROWS
            );
            assert_eq!(history.row_cache.borrow().pinned.len(), 0);
        });
    }

    #[gpui_kit::test]
    fn ended_pins_evict_without_entering_another_row(cx: &mut TestAppContext) {
        let (_, chat) = crate::gpui_tests::open(cx);
        let history = cx.update(|cx| chat.read(cx).history.clone());
        let rows = cx.update(|cx| {
            let mut rows = Vec::new();
            for index in 0..(RETAINED_ROWS * 8) {
                let row = history.update(cx, |history, cx| {
                    let index = history.push(
                        Message::new(MessageId(index as u64), Role::Assistant, "message"),
                        false,
                        cx,
                    );
                    history
                        .materialize(index, cx)
                        .expect("materialize history row")
                });
                row.update(cx, |row, cx| {
                    if index == 0 {
                        let text = cx.new(|cx| {
                            gpui_kit::component::text::TextViewState::markdown("message", cx)
                        });
                        text.update(cx, |text, cx| text.select_all(cx));
                        row.markdown = Some(text);
                    } else {
                        row.streaming = true;
                    }
                });
                let cache = history.read(cx).row_cache.clone();
                RowCache::use_row(&cache, &history, index, &row, cx);
                rows.push(row);
            }
            rows
        });
        cx.update(|cx| {
            assert_eq!(
                history.read(cx).row_cache.borrow().pinned.len(),
                rows.len() - RETAINED_ROWS
            );
            for row in rows {
                if let Some(text) = row.read(cx).markdown.clone() {
                    text.update(cx, |text, cx| text.clear_selection(cx));
                } else {
                    row.update(cx, |row, cx| {
                        row.streaming = false;
                        cx.notify();
                    });
                }
            }
        });
        cx.update(|cx| {
            let history = history.read(cx);
            assert!(history.row_cache.borrow().pinned.is_empty());
            assert_eq!(
                history
                    .rows
                    .iter()
                    .filter(|row| row.mounted().is_some())
                    .count(),
                RETAINED_ROWS
            );
        });
    }
}
