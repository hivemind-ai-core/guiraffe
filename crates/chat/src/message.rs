use crate::{Message, Role, ToolCall, ToolStatus};
use gpui_kit::base::{Avatar, AvatarFallback, TestSupportExt};
use gpui_kit::{
    component::text::{TextView, TextViewState},
    prelude::*,
    *,
};
use guiraffe_components::Text;
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StyleContext, Variant};
use guiraffe_motion::{
    MotionPrefs, Presence, PresenceFrame, Transition, TransitionKind, set_presence,
};
#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::rc::Rc;

/// A message's lifetime is independent of its virtualized row. Markdown parsing and
/// selection state survive scrolling out of view; siblings never read this entity.
pub struct MessageView {
    pub(crate) data: Message,
    markdown: Entity<TextViewState>,
    tools: Vec<Entity<ToolCard>>,
    reasoning: Option<Entity<Reasoning>>,
    reasoning_subscription: Option<Subscription>,
    pub(crate) editable: bool,
    owner: WeakEntity<crate::ChatState>,
    presence: Option<Presence>,
    #[cfg(test)]
    pub(crate) renders: Rc<Cell<usize>>,
    _layout_subscriptions: Vec<Subscription>,
    pub(crate) layout: RowLayout,
}
#[derive(Clone)]
pub(crate) struct RowLayout {
    pub list: ListState,
    pub index: usize,
}
impl RowLayout {
    pub fn changed(&self, _cx: &mut App) {
        self.list.remeasure_items(self.index..self.index + 1);
    }
    fn observe<T: 'static>(&self, entity: &Entity<T>, cx: &mut App) -> Subscription {
        let layout = self.clone();
        cx.observe(entity, move |_, cx| layout.changed(cx))
    }
}
impl MessageView {
    pub(crate) fn new(
        data: Message,
        owner: WeakEntity<crate::ChatState>,
        animate: bool,
        layout: RowLayout,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown = cx.new(|cx| TextViewState::markdown(&data.text, cx).scrollable(false));
        let tools: Vec<_> = data
            .tools
            .iter()
            .map(|tool| cx.new(|_| ToolCard::new(tool.clone())))
            .collect();
        let reasoning = (!data.reasoning.is_empty())
            .then(|| cx.new(|cx| Reasoning::new(&data.reasoning, &layout, cx)));
        let transition =
            Transition::themed(TransitionKind::FadeUp, &MotionPrefs::global(cx).tokens);
        let mut presence = Presence::new(!animate, transition.enter, transition.exit);
        if animate {
            set_presence(&mut presence, true, cx);
        }
        let mut subscriptions = vec![layout.observe(&markdown, cx)];
        subscriptions.extend(tools.iter().map(|tool| layout.observe(tool, cx)));
        let reasoning_subscription = reasoning
            .as_ref()
            .map(|reasoning| layout.observe(reasoning, cx));
        Self {
            data,
            markdown,
            tools,
            reasoning,
            reasoning_subscription,
            editable: false,
            owner,
            presence: animate.then_some(presence),
            #[cfg(test)]
            renders: Rc::default(),
            _layout_subscriptions: subscriptions,
            layout,
        }
    }
    pub(crate) fn append(&mut self, delta: &str, cx: &mut Context<Self>) {
        if delta.is_empty() {
            return;
        }
        self.data.text.push_str(delta);
        self.markdown
            .update(cx, |text, cx| text.push_str(delta, cx));
        cx.notify();
    }
    pub(crate) fn replace_user(
        &mut self,
        text: String,
        attachments: Vec<crate::Attachment>,
        cx: &mut Context<Self>,
    ) {
        self.markdown
            .update(cx, |markdown, cx| markdown.set_text(&text, cx));
        self.data.text = text;
        self.data.attachments = attachments;
        cx.notify();
    }
    pub(crate) fn set_reasoning(&mut self, text: String, cx: &mut Context<Self>) {
        self.data.reasoning = text;
        if self.data.reasoning.is_empty() {
            self.reasoning = None;
            self.reasoning_subscription = None;
            cx.notify();
            return;
        }
        if let Some(reasoning) = &self.reasoning {
            reasoning.update(cx, |reasoning, cx| {
                reasoning.markdown.update(cx, |markdown, cx| {
                    markdown.set_text(&self.data.reasoning, cx)
                })
            });
        } else {
            let reasoning = cx.new(|cx| Reasoning::new(&self.data.reasoning, &self.layout, cx));
            self.reasoning_subscription = Some(self.layout.observe(&reasoning, cx));
            self.reasoning = Some(reasoning);
            cx.notify();
        }
        self.layout.changed(cx);
    }
    pub(crate) fn set_tool(&mut self, tool: ToolCall, cx: &mut Context<Self>) {
        if let Some(index) = self.data.tools.iter().position(|t| t.id == tool.id) {
            if self.data.tools[index] == tool {
                return;
            }
            self.data.tools[index] = tool.clone();
            self.tools[index].update(cx, |card, cx| {
                card.details = tool_details(&tool);
                card.tool = tool;
                cx.notify();
            });
        } else {
            self.data.tools.push(tool.clone());
            let card = cx.new(|_| ToolCard::new(tool));
            self._layout_subscriptions
                .push(self.layout.observe(&card, cx));
            self.tools.push(card);
            cx.notify();
        }
    }
}
fn markdown(state: &Entity<TextViewState>) -> TextView {
    TextView::new(state)
        .scrollable(false)
        .selectable(true)
        .stream_fade(false)
        .code_block_actions(|code, _, cx| {
            let content = code.code();
            crate::style::button("copy-code", Variant::Subtle, None, cx)
                .label("Copy code")
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(content.to_string()))
                })
        })
}
impl Render for MessageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        let alpha = if let Some(presence) = self.presence.as_mut() {
            match presence.frame(cx.background_executor().now(), &MotionPrefs::global(cx)) {
                PresenceFrame::Mounted {
                    visibility,
                    animating,
                } => {
                    if animating {
                        window.request_animation_frame();
                    } else {
                        self.presence = None;
                    }
                    visibility
                }
                PresenceFrame::Unmounted => 0.0,
            }
        } else {
            1.0
        };
        let theme = GuiraffeTheme::global(cx);
        let s = theme.surfaces();
        let sc = StyleContext::current();
        let gap = px(sc.density.space(Size::Xs));
        let user = self.data.role == Role::User;
        let color = if user {
            ColorName::Blue
        } else {
            ColorName::Violet
        };
        let v = theme.variant(Some(color), Variant::Light);
        let id = self.data.id;
        let owner = self.owner.clone();
        let retry_owner = owner.clone();
        div()
            .id(("message", id.0))
            .test_support()
            .w_full()
            .py(gap)
            .opacity(alpha)
            .flex()
            .when(user, |el| el.justify_end())
            .child(
                div()
                    .w_full()
                    .max_w(relative(if user { 0.86 } else { 1.0 }))
                    .flex()
                    .gap(gap)
                    .child(
                        Avatar::new()
                            .size(px(sc.control_size.control_height()))
                            .flex_none()
                            .rounded(px(Size::Xl.radius()))
                            .overflow_hidden()
                            .fallback(
                                AvatarFallback::new()
                                    .rounded(px(Size::Xl.radius()))
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(v.background.to_hsla())
                                    .text_color(v.foreground.to_hsla())
                                    .child(if user { "Y" } else { "A" }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(gap)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(gap)
                                    .child(
                                        Text::new(if user { "You" } else { "Assistant" })
                                            .size(Size::Sm),
                                    )
                                    .child(
                                        Text::new(self.data.timestamp.clone())
                                            .size(Size::Xs)
                                            .dimmed(true),
                                    ),
                            )
                            .child(
                                div()
                                    .rounded(px(theme.default_radius.radius()))
                                    .p(px(sc.density.space(Size::Sm)))
                                    .bg(if user {
                                        v.background.to_hsla()
                                    } else {
                                        s.surface.to_hsla()
                                    })
                                    .flex()
                                    .flex_col()
                                    .gap(gap)
                                    .children(self.tools.iter().cloned())
                                    .children(self.reasoning.iter().cloned())
                                    .child(markdown(&self.markdown))
                                    .children(self.data.attachments.iter().map(|a| {
                                        Text::new(format!("↗ {}", a.name))
                                            .dimmed(true)
                                            .size(Size::Xs)
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(gap)
                                    .child(
                                        crate::style::button(
                                            "copy-message",
                                            Variant::Subtle,
                                            None,
                                            cx,
                                        )
                                        .compact()
                                        .label("Copy")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    this.data.text.clone(),
                                                ))
                                            }),
                                        ),
                                    )
                                    .when(!user, |el| {
                                        el.child(
                                            crate::style::button(
                                                "retry-message",
                                                Variant::Subtle,
                                                None,
                                                cx,
                                            )
                                            .compact()
                                            .label("Retry")
                                            .on_click(
                                                move |_, _, cx| {
                                                    let _ = retry_owner.update(cx, |chat, cx| {
                                                        chat.request_retry(id, cx)
                                                    });
                                                },
                                            ),
                                        )
                                    })
                                    .when(self.editable, |el| {
                                        el.child(
                                            crate::style::button(
                                                "edit-message",
                                                Variant::Subtle,
                                                None,
                                                cx,
                                            )
                                            .compact()
                                            .label("Edit")
                                            .on_click(
                                                move |_, window, cx| {
                                                    let _ = owner.update(cx, |chat, cx| {
                                                        chat.edit_last_user(window, cx)
                                                    });
                                                },
                                            ),
                                        )
                                    }),
                            ),
                    ),
            )
    }
}
struct ToolCard {
    tool: ToolCall,
    open: bool,
    details: SharedString,
}
fn tool_details(tool: &ToolCall) -> SharedString {
    format!("Arguments\n{}\n\nResult\n{}", tool.arguments, tool.result).into()
}
impl ToolCard {
    fn new(tool: ToolCall) -> Self {
        Self {
            details: tool_details(&tool),
            tool,
            open: false,
        }
    }
}
impl Render for ToolCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let sc = StyleContext::current();
        let (status, color) = match self.tool.status {
            ToolStatus::Running => ("Running", ColorName::Blue),
            ToolStatus::Succeeded => ("Succeeded", ColorName::Teal),
            ToolStatus::Failed => ("Failed", ColorName::Red),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(sc.density.space(Size::Xs)))
            .child(
                crate::style::button("toggle-tool", Variant::Subtle, Some(color), cx)
                    .accessibility_label(format!("{} · {}", self.tool.name, status))
                    .child(
                        div()
                            .id("tool-header-label")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .text_left()
                            .text_ellipsis()
                            .child(format!(
                                "{} {} · {}",
                                if self.open { "▾" } else { "▸" },
                                self.tool.name,
                                status
                            )),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = !this.open;
                        cx.notify();
                    })),
            )
            .when(self.open, |el| {
                el.child(
                    div()
                        .p(px(sc.density.space(Size::Xs)))
                        .rounded(px(Size::Xs.radius()))
                        .bg(s.track.to_hsla())
                        .child(Text::new(self.details.clone()).size(Size::Xs).color(color)),
                )
            })
    }
}
struct Reasoning {
    open: bool,
    markdown: Entity<TextViewState>,
    _layout_subscription: Subscription,
}
impl Reasoning {
    fn new(text: &str, layout: &RowLayout, cx: &mut Context<Self>) -> Self {
        let markdown = cx.new(|cx| TextViewState::markdown(text, cx).scrollable(false));
        Self {
            open: false,
            _layout_subscription: layout.observe(&markdown, cx),
            markdown,
        }
    }
}
impl Render for Reasoning {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(
                crate::style::button("toggle-reasoning", Variant::Subtle, None, cx)
                    .accessibility_label("Reasoning")
                    // Kit centres a button's content; a flex_1 left-aligned child keeps the
                    // header flush left like the tool header.
                    .child(
                        div()
                            .id("reasoning-header-label")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .text_left()
                            .text_ellipsis()
                            .child(if self.open {
                                "▾ Reasoning"
                            } else {
                                "▸ Reasoning"
                            }),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = !this.open;
                        cx.notify();
                    })),
            )
            .when(self.open, |el| el.child(markdown(&self.markdown)))
    }
}
