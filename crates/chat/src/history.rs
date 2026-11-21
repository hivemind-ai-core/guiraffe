use crate::{message::MessageView, scroll::Stickiness};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{prelude::*, *};
use guiraffe_core::{Size, StyleContext, Variant};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

const RETAINED_TEXT_ROWS: usize = 64;

#[derive(Default)]
struct TextRows {
    recent: BTreeMap<usize, WeakEntity<MessageView>>,
    selected: BTreeMap<usize, WeakEntity<MessageView>>,
}

impl TextRows {
    fn use_row(cache: &Rc<RefCell<Self>>, index: usize, row: &Entity<MessageView>, cx: &mut App) {
        let mut this = cache.borrow_mut();
        if this.recent.get(&index).is_some_and(|entry| entry == row) {
            return;
        }
        this.recent.insert(index, row.downgrade());
        this.selected.remove(&index);

        // Reconsider pins when entering a new row. Selection endpoints are
        // owned by the window and must keep their original entity IDs.
        this.selected.retain(|&key, candidate| {
            let Some(candidate) = candidate.upgrade() else {
                return false;
            };
            if candidate.read(cx).has_text_selection(cx) {
                return true;
            }
            defer_evict(cache, key, candidate, cx);
            false
        });
        while this.recent.len() > RETAINED_TEXT_ROWS {
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
                if row.read(cx).has_text_selection(cx) {
                    this.selected.insert(victim, candidate);
                } else {
                    defer_evict(cache, victim, row, cx);
                }
            }
        }
    }
}

fn defer_evict(
    cache: &Rc<RefCell<TextRows>>,
    index: usize,
    row: Entity<MessageView>,
    cx: &mut App,
) {
    let cache = cache.clone();
    cx.defer(move |cx| {
        let mut cache = cache.borrow_mut();
        if cache.recent.contains_key(&index) {
            return;
        }
        if row.read(cx).has_text_selection(cx) {
            cache.selected.insert(index, row.downgrade());
        } else {
            row.update(cx, MessageView::evict_text);
        }
    });
}

pub(crate) struct History {
    pub(crate) rows: Vec<Entity<MessageView>>,
    pub(crate) list: ListState,
    pub(crate) sticky: Stickiness,
    text_rows: Rc<RefCell<TextRows>>,
    #[cfg(test)]
    pub(crate) renders: std::rc::Rc<std::cell::Cell<usize>>,
}
impl History {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
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
            list,
            sticky: Stickiness::default(),
            text_rows: Default::default(),
            #[cfg(test)]
            renders: Default::default(),
        }
    }
    pub(crate) fn push(&mut self, row: Entity<MessageView>, cx: &mut Context<Self>) {
        let first_pending_insert = self.list.item_count() == self.rows.len();
        self.rows.push(row);
        self.sticky.appended_message();
        if first_pending_insert {
            cx.notify();
        }
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
        let text_rows = self.text_rows.clone();
        let sc = StyleContext::current();
        div()
            .id("chat-history")
            .test_support()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                list(self.list.clone(), move |index, _, cx| {
                    let Some(row) = owner.read(cx).rows.get(index).cloned() else {
                        return div().into_any_element();
                    };
                    TextRows::use_row(&text_rows, index, &row, cx);
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
