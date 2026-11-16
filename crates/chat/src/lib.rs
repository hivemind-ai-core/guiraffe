//! Agent chat: virtualized native Markdown, isolated message views and bounded streaming.
//! Initialize once before opening windows, then retain a `ChatState` per conversation.
pub mod commands;
pub mod model;
pub mod scroll;
pub mod stream;
pub use model::{Attachment, Message, MessageId, Role, ToolCall, ToolStatus};

mod composer;
mod history;
mod message;
mod style;
pub use composer::Composer;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, Global, IntoElement, ParentElement, Render,
    RenderOnce, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};
use guiraffe_components::Text;
use guiraffe_core::{GuiraffeTheme, Size, StyleContext};
use std::{collections::HashMap, time::Duration};
use stream::{Cancellation, Producer};

/// Small provider-facing events. Transport and file dialogs belong to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    Submitted {
        id: MessageId,
        text: String,
        attachments: Vec<Attachment>,
    },
    Retry(MessageId),
    Edited(MessageId),
    Stopped(MessageId),
    AttachmentRequested,
    Finished(MessageId),
}
struct Initialized;
impl Global for Initialized {}
/// Initialize native Markdown/input, motion, theme and overlay dependencies once.
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    guiraffe_overlays::init(cx);
    composer::init(cx);
    cx.set_global(Initialized);
}

/// Serializable, provider-independent presentation options.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(default)]
pub struct ChatProps {
    pub title: String,
    pub subtitle: String,
}
impl Default for ChatProps {
    fn default() -> Self {
        Self {
            title: "Workspace assistant".into(),
            subtitle: "A little context. A clearer next step.".into(),
        }
    }
}

/// Retain one state per conversation/window. No global conversation state.
pub struct ChatState {
    props: ChatProps,
    history: Entity<history::History>,
    composer: Entity<Composer>,
    messages: HashMap<MessageId, Entity<message::MessageView>>,
    last_user: Option<MessageId>,
    editing: Option<MessageId>,
    next_id: u64,
    active: Option<(MessageId, Cancellation)>,
    stream_task: Option<Task<()>>,
    #[cfg(test)]
    pub(crate) renders: std::rc::Rc<std::cell::Cell<usize>>,
}
impl EventEmitter<ChatEvent> for ChatState {}
impl ChatState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_props(ChatProps::default(), window, cx)
    }
    pub fn with_props(props: ChatProps, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let owner = cx.weak_entity();
        Self {
            props,
            history: cx.new(history::History::new),
            composer: cx.new(|cx| Composer::new(owner, window, cx)),
            messages: HashMap::new(),
            last_user: None,
            editing: None,
            next_id: 1,
            active: None,
            stream_task: None,
            #[cfg(test)]
            renders: Default::default(),
        }
    }
    pub fn props(&self) -> &ChatProps {
        &self.props
    }
    pub fn set_props(&mut self, props: ChatProps, cx: &mut Context<Self>) {
        if self.props != props {
            self.props = props;
            cx.notify();
        }
    }
    pub fn composer(&self) -> &Entity<Composer> {
        &self.composer
    }
    pub fn is_streaming(&self) -> bool {
        self.active.is_some()
    }
    pub fn message<'a>(&self, id: MessageId, cx: &'a App) -> Option<&'a Message> {
        self.messages.get(&id).map(|row| &row.read(cx).data)
    }
    /// Conversation order, borrowed without cloning complete message documents.
    pub fn messages<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a Message> {
        self.history
            .read(cx)
            .rows
            .iter()
            .map(move |row| &row.read(cx).data)
    }
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
    fn allocate_id(&mut self) -> MessageId {
        while self.messages.contains_key(&MessageId(self.next_id)) {
            self.next_id = self.next_id.wrapping_add(1);
        }
        let id = MessageId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
    /// Insert provider-supplied history. Duplicate ids are rejected without altering any state.
    pub fn push_message(&mut self, message: Message, cx: &mut Context<Self>) -> bool {
        self.insert(message, false, cx)
    }
    fn insert(&mut self, data: Message, animate: bool, cx: &mut Context<Self>) -> bool {
        let id = data.id;
        if self.messages.contains_key(&id) {
            return false;
        }
        let user = data.role == Role::User;
        if user {
            if let Some(old) = self.last_user.and_then(|id| self.messages.get(&id)) {
                old.update(cx, |row, cx| {
                    row.editable = false;
                    cx.notify();
                });
            }
            self.last_user = Some(id);
        }
        let owner = cx.weak_entity();
        let h = self.history.read(cx);
        let layout = message::RowLayout {
            list: h.list.clone(),
            index: h.rows.len(),
        };
        let row = cx.new(|cx| message::MessageView::new(data, owner, animate, layout, cx));
        if user {
            row.update(cx, |row, _| row.editable = true);
        }
        self.messages.insert(id, row.clone());
        self.history.update(cx, |history, cx| history.push(row, cx));
        true
    }
    pub fn submit(&mut self, text: String, attachments: Vec<Attachment>, cx: &mut Context<Self>) {
        if self.is_streaming() || (text.trim().is_empty() && attachments.is_empty()) {
            return;
        }
        let id = if let Some(id) = self.editing.take().filter(|id| Some(*id) == self.last_user) {
            let rows = self.history.update(cx, |history, cx| {
                let Some(index) = history
                    .rows
                    .iter()
                    .position(|row| row.read(cx).data.id == id)
                else {
                    return Vec::new();
                };
                let removed = history.rows.split_off(index + 1);
                history.list.splice(index + 1..index + 1 + removed.len(), 0);
                cx.notify();
                removed
            });
            for row in rows {
                self.messages.remove(&row.read(cx).data.id);
            }
            if let Some(row) = self.messages.get(&id) {
                row.update(cx, |row, cx| {
                    row.replace_user(text.clone(), attachments.clone(), cx)
                });
            }
            id
        } else {
            let id = self.allocate_id();
            let mut message = Message::new(id, Role::User, text.clone());
            message.attachments = attachments.clone();
            self.insert(message, true, cx);
            id
        };
        // Submission can originate inside Composer::send; defer its presentation
        // reset until that entity's event callback has released its mutable borrow.
        let composer = self.composer.downgrade();
        cx.defer(move |cx| {
            if let Some(composer) = composer.upgrade()
                && composer.read(cx).editing
            {
                composer.update(cx, |composer, cx| {
                    composer.editing = false;
                    cx.notify();
                });
            }
        });
        cx.emit(ChatEvent::Submitted {
            id,
            text,
            attachments,
        });
    }
    pub fn add_attachment(&mut self, attachment: Attachment, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            if let Some(old) = composer
                .attachments
                .iter_mut()
                .find(|a| a.id == attachment.id)
            {
                if *old == attachment {
                    return;
                }
                *old = attachment;
            } else {
                composer.attachments.push(attachment);
            }
            cx.notify();
        });
    }
    pub fn set_draft(
        &mut self,
        text: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.set_draft(text.into(), window, cx)
        });
    }
    /// Discard an edit draft without modifying conversation history.
    pub fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            self.composer.update(cx, |composer, cx| {
                composer.editing = false;
                composer.attachments.clear();
                composer.set_draft(String::new(), window, cx);
            });
        }
    }
    /// Begin a reply, cancelling the previous producer. Drop the returned producer to finish.
    /// Empty frames perform no entity writes; only the active message receives token batches.
    pub fn begin_reply(&mut self, cx: &mut Context<Self>) -> (MessageId, Producer) {
        self.cancel_active();
        let id = self.allocate_id();
        self.insert(Message::new(id, Role::Assistant, ""), true, cx);
        let (producer, consumer) = stream::channel(64);
        self.active = Some((id, consumer.cancellation()));
        self.composer.update(cx, |composer, cx| {
            if !composer.busy || composer.editing {
                composer.editing = false;
                composer.busy = true;
                cx.notify();
            }
        });
        let row = self.messages[&id].clone();
        let history = self.history.clone();
        let (list, index) = {
            let h = history.read(cx);
            (h.list.clone(), h.rows.len() - 1)
        };
        self.stream_task = Some(cx.spawn(async move |chat, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if consumer.is_cancelled() {
                    break;
                }
                let delta = consumer.drain(8);
                if !delta.is_empty() {
                    cx.update(|cx| {
                        row.update(cx, |row, cx| row.append(&delta, cx));
                        list.remeasure_items(index..index + 1);
                    });
                }
                if consumer.is_finished() {
                    let _ = chat.update(cx, |chat, cx| chat.finish(id, cx));
                    break;
                }
            }
        }));
        (id, producer)
    }
    fn finish(&mut self, id: MessageId, cx: &mut Context<Self>) {
        if self.active.as_ref().map(|(active, _)| *active) != Some(id) {
            return;
        }
        self.active = None;
        self.composer.update(cx, |composer, cx| {
            composer.busy = false;
            cx.notify();
        });
        cx.emit(ChatEvent::Finished(id));
    }
    fn cancel_active(&mut self) {
        if let Some((_, cancellation)) = self.active.take() {
            cancellation.cancel();
        }
        self.stream_task = None;
    }
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let id = self.active.as_ref().map(|(id, _)| *id);
        self.cancel_active();
        if let Some(id) = id {
            self.composer.update(cx, |composer, cx| {
                composer.busy = false;
                cx.notify();
            });
            cx.emit(ChatEvent::Stopped(id));
        }
    }
    /// Replace provider reasoning without opening its disclosure or cloning history.
    pub fn set_reasoning(
        &mut self,
        id: MessageId,
        text: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(row) = self.messages.get(&id) else {
            return false;
        };
        let text = text.into();
        if row.read(cx).data.reasoning != text {
            row.update(cx, |row, cx| row.set_reasoning(text, cx));
        }
        true
    }
    pub fn update_tool(&mut self, id: MessageId, tool: ToolCall, cx: &mut Context<Self>) -> bool {
        let Some(row) = self.messages.get(&id) else {
            return false;
        };
        if row.read(cx).data.tools.contains(&tool) {
            return true;
        }
        row.update(cx, |row, cx| row.set_tool(tool, cx));
        self.history.read(cx).list.remeasure();
        true
    }
    pub(crate) fn request_retry(&mut self, id: MessageId, cx: &mut Context<Self>) {
        if !self.is_streaming() {
            cx.emit(ChatEvent::Retry(id));
        }
    }
    pub fn edit_last_user(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_streaming() {
            return;
        }
        if let Some(id) = self.last_user {
            let data = &self.messages[&id].read(cx).data;
            let text = data.text.clone();
            let attachments = data.attachments.clone();
            self.editing = Some(id);
            self.composer.update(cx, |composer, cx| {
                composer.attachments = attachments;
                composer.editing = true;
                composer.set_draft(text, window, cx);
            });
            cx.emit(ChatEvent::Edited(id));
        }
    }
    pub fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |history, cx| history.jump(cx));
    }
}
impl Drop for ChatState {
    fn drop(&mut self) {
        self.cancel_active();
    }
}
impl Render for ChatState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        let s = GuiraffeTheme::global(cx).surfaces();
        let sc = StyleContext::current();
        div()
            .id("chat")
            .test_support()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .child(
                div()
                    .id("chat-header")
                    .test_support()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(sc.density.space(Size::Sm)))
                    .p(px(sc.density.space(Size::Md)))
                    .border_b_1()
                    .border_color(s.border.to_hsla())
                    .child(
                        div()
                            .flex_none()
                            .size(px(sc.control_size.control_height()))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(GuiraffeTheme::global(cx).default_radius.radius()))
                            .bg(GuiraffeTheme::global(cx)
                                .variant(
                                    Some(guiraffe_core::ColorName::Violet),
                                    guiraffe_core::Variant::Light,
                                )
                                .background
                                .to_hsla())
                            .text_color(
                                GuiraffeTheme::global(cx)
                                    .variant(
                                        Some(guiraffe_core::ColorName::Violet),
                                        guiraffe_core::Variant::Light,
                                    )
                                    .foreground
                                    .to_hsla(),
                            )
                            .child("AI"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(Text::new(self.props.title.clone()))
                            .child(
                                Text::new(self.props.subtitle.clone())
                                    .dimmed(true)
                                    .size(Size::Xs),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.history.clone()),
            )
            .child(self.composer.clone())
    }
}
/// Stateless, styleable Control wrapper. The caller owns outer placement/spacing.
#[derive(IntoElement)]
pub struct Chat {
    state: Entity<ChatState>,
    style: StyleRefinement,
}
impl Chat {
    pub fn new(state: &Entity<ChatState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
        }
    }
}
impl Styled for Chat {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for Chat {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut el = div().size_full();
        el.style().refine(&self.style);
        el.child(self.state)
    }
}

#[cfg(test)]
mod gpui_tests;
