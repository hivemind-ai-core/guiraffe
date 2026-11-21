use crate::{message::MessageView, scroll::Stickiness};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{prelude::*, *};
use guiraffe_core::{Size, StyleContext, Variant};

pub(crate) struct History {
    pub(crate) rows: Vec<Entity<MessageView>>,
    pub(crate) list: ListState,
    pub(crate) sticky: Stickiness,
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
        let sc = StyleContext::current();
        div()
            .id("chat-history")
            .test_support()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                list(self.list.clone(), move |index, _, cx| {
                    owner.read(cx).rows.get(index).map_or_else(
                        || div().into_any_element(),
                        |row| row.clone().into_any_element(),
                    )
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
