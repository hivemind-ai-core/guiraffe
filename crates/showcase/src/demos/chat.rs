//! guiraffe-chat: an agent chat with streaming replies, tool calls and a composer.

use std::time::Duration;

use gpui_kit::component::{Sizable, button::Button};
use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window, div, px,
};
use guiraffe_chat::{
    Attachment, Chat, ChatEvent, ChatProps, ChatState, Message, MessageId, Role, ToolCall,
    ToolStatus, stream::PushError,
};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "chat",
        name: "Agent chat",
        summary: "Virtualized history, streamed Markdown replies, tool calls and a composer with slash commands. Send a message to watch a reply stream in.",
        covers: &["Chat", "ChatState", "Composer", "MessageView"],
        demo: chat,
    }]
}

const REPLY: &str = "Done. I traced the slow frame to the **row measurement** pass:\n\n1. Rows are now measured once and cached by width.\n2. Streaming only re-measures the active message.\n\n```rust\nlist.remeasure_items(index..index + 1);\n```\n\nFrame time dropped from 21 ms to **4 ms**.";

struct ChatDemo {
    chat: Entity<ChatState>,
    log: Entity<EventLog>,
    _events: Subscription,
}

fn describe(e: &ChatEvent) -> SharedString {
    match e {
        ChatEvent::Submitted {
            id,
            text,
            attachments,
        } => format!(
            "Submitted {id:?} “{}” +{} file(s)",
            text.chars().take(32).collect::<String>(),
            attachments.len()
        )
        .into(),
        other => format!("{other:?}").into(),
    }
}

fn tool(status: ToolStatus) -> ToolCall {
    ToolCall {
        id: "profile".into(),
        name: "Profile render loop".into(),
        status,
        arguments: "{ \"view\": \"History\", \"frames\": 120 }".into(),
        result: if status == ToolStatus::Running {
            "Sampling frames…"
        } else {
            "p95 21.4 ms · 83% in measure_rows"
        }
        .into(),
    }
}

fn seed(chat: &mut ChatState, cx: &mut Context<ChatState>) {
    let mut m = Message::new(
        MessageId(1),
        Role::User,
        "Scrolling the history stutters once it has a few hundred messages. Can you find out why?",
    );
    m.timestamp = "10:41".into();
    chat.push_message(m, cx);

    let mut m = Message::new(
        MessageId(2),
        Role::Assistant,
        "I profiled the history view while scrolling. Most of each frame goes to **measuring rows** that have not changed:\n\n| pass | time |\n|---|---|\n| measure_rows | 17.8 ms |\n| paint | 2.9 ms |\n| layout | 0.7 ms |\n\nShall I cache row heights by width?",
    );
    m.timestamp = "10:41".into();
    m.reasoning = "The list re-measures every visible row on each frame; heights only depend on text and width.".into();
    m.tools.push(tool(ToolStatus::Succeeded));
    chat.push_message(m, cx);

    let mut m = Message::new(MessageId(3), Role::User, "Yes, and keep streaming smooth.");
    m.timestamp = "10:42".into();
    m.attachments.push(Attachment {
        id: "trace".into(),
        name: "scroll-trace.json".into(),
    });
    chat.push_message(m, cx);

    let mut m = Message::new(
        MessageId(4),
        Role::Assistant,
        "Caching heights now. I'll re-run the profile once the patch builds…",
    );
    m.timestamp = "10:42".into();
    m.tools.push(ToolCall {
        id: "patch".into(),
        name: "Apply patch".into(),
        status: ToolStatus::Running,
        arguments: "{ \"file\": \"src/history.rs\" }".into(),
        result: "Building…".into(),
    });
    chat.push_message(m, cx);
}

fn stream_reply(chat: &Entity<ChatState>, cx: &mut Context<ChatDemo>) {
    let (id, producer) = chat.update(cx, |chat, cx| chat.begin_reply(cx));
    chat.update(cx, |chat, cx| {
        chat.update_tool(id, tool(ToolStatus::Running), cx)
    });
    let timers = cx.background_executor().clone();
    cx.background_executor()
        .spawn(async move {
            for chunk in REPLY.split_inclusive(' ') {
                let mut pending = chunk.to_owned();
                loop {
                    match producer.push(pending) {
                        Ok(()) => break,
                        Err(PushError::Full(value)) => {
                            pending = value;
                            timers.timer(Duration::from_millis(16)).await;
                        }
                        Err(_) => return,
                    }
                }
                timers.timer(Duration::from_millis(40)).await;
            }
        })
        .detach();
}

fn chat(window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|cx| {
        let chat = cx.new(|cx| {
            let mut chat = ChatState::with_props(
                ChatProps {
                    title: "Perf assistant".into(),
                    subtitle: "Profiles, patches and explains.".into(),
                },
                window,
                cx,
            );
            seed(&mut chat, cx);
            chat
        });
        let events = cx.subscribe(&chat, |_, chat, event, cx| match event {
            ChatEvent::Submitted { .. } | ChatEvent::Retry(_) => stream_reply(&chat, cx),
            ChatEvent::Finished(id) => {
                chat.update(cx, |chat, cx| {
                    chat.update_tool(*id, tool(ToolStatus::Succeeded), cx)
                });
            }
            ChatEvent::AttachmentRequested => chat.update(cx, |chat, cx| {
                chat.add_attachment(
                    Attachment {
                        id: "notes".into(),
                        name: "perf-notes.md".into(),
                    },
                    cx,
                )
            }),
            _ => {}
        });
        let log = EventLog::new(cx);
        EventLog::watch_with(&log, &chat, describe, cx);
        ChatDemo {
            chat,
            log,
            _events: events,
        }
    })
    .into()
}

impl Render for ChatDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        type Action = fn(&mut ChatState, &mut Window, &mut Context<ChatState>);
        let action = |id: &'static str, label: &'static str, f: Action| {
            let chat = self.chat.clone();
            Button::new(id)
                .label(label)
                .small()
                .on_click(move |_, window, cx| chat.update(cx, |c, cx| f(c, window, cx)))
        };
        column([
            example(
                "Conversation with a tool call in flight",
                div()
                    .h(px(380.))
                    .overflow_hidden()
                    .child(Chat::new(&self.chat)),
            ),
            div()
                .flex()
                .gap(px(24.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .content_start()
                        .gap(px(8.))
                        .w(px(360.))
                        .child(action("chat-draft", "Prefill draft", |c, w, cx| {
                            c.set_draft("Why is the first frame slow?", w, cx)
                        }))
                        .child(action("chat-stop", "Stop streaming", |c, _, cx| c.stop(cx)))
                        .child(action("chat-edit", "Edit last message", |c, w, cx| {
                            c.edit_last_user(w, cx)
                        }))
                        .child(action("chat-cancel", "Cancel edit", |c, w, cx| {
                            c.cancel_edit(w, cx)
                        }))
                        .child(action("chat-jump", "Jump to latest", |c, _, cx| {
                            c.jump_to_latest(cx)
                        })),
                )
                .child(div().flex_1().child(self.log.clone()))
                .into_any_element(),
        ])
    }
}
