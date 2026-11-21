//! DataUi adapter for provider-independent conversation documents.

use std::{collections::HashSet, sync::Arc};

use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};
use guiraffe_overlays::search::Command;

use crate::{
    Attachment, Chat, ChatEvent, ChatProps, ChatState, Message, MessageId, Role, ToolCall,
    ToolStatus,
    history::{History, Row},
    widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| ChatDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

fn string(value: &Value, name: &str) -> String {
    field(value, name).as_str().unwrap_or_default().to_owned()
}

fn attachments(value: &Value) -> Vec<Attachment> {
    items(value)
        .filter_map(|item| {
            Some(Attachment {
                id: field(item, "id").as_str()?.to_owned(),
                name: field(item, "name").as_str()?.to_owned(),
            })
        })
        .collect()
}

fn completions(value: &Value) -> Vec<Command> {
    items(value)
        .filter_map(|item| {
            Some(Command {
                id: field(item, "id").as_str()?.to_owned(),
                label: field(item, "label").as_str()?.to_owned(),
                keywords: string(item, "keywords"),
                disabled: field(item, "disabled").as_bool().unwrap_or_default(),
            })
        })
        .collect()
}

fn messages(value: &Value) -> Vec<Message> {
    let mut ids = HashSet::new();
    items(value)
        .filter_map(|item| {
            let id = field(item, "id")
                .as_f64()
                .filter(|v| *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0)?
                as u64;
            let role = match field(item, "role").as_str()? {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => return None,
            };
            if !ids.insert(id) {
                return None;
            }
            let tools = items(field(item, "tools"))
                .filter_map(|tool| {
                    Some(ToolCall {
                        id: field(tool, "id").as_str()?.to_owned(),
                        name: field(tool, "name").as_str()?.to_owned(),
                        status: match field(tool, "status").as_str()? {
                            "running" => ToolStatus::Running,
                            "succeeded" => ToolStatus::Succeeded,
                            "failed" => ToolStatus::Failed,
                            _ => return None,
                        },
                        arguments: string(tool, "arguments"),
                        result: string(tool, "result"),
                    })
                })
                .collect();
            Some(Message {
                id: MessageId(id),
                role,
                text: string(item, "text"),
                timestamp: string(item, "timestamp"),
                attachments: attachments(field(item, "attachments")),
                tools,
                reasoning: string(item, "reasoning"),
            })
        })
        .collect()
}

fn sync_messages(chat: &mut ChatState, next: Vec<Message>, cx: &mut Context<ChatState>) {
    let count = chat.message_count();
    let retained_prefix = count <= next.len()
        && chat
            .messages(cx)
            .zip(&next)
            .all(|(old, new)| old.id == new.id && old.role == new.role);
    if !retained_prefix {
        if let Some((_, cancellation)) = chat.active.take() {
            cancellation.cancel();
        }
        chat.stream_task = None;
        chat.messages.clear();
        chat.last_user = None;
        chat.editing = None;
        chat.composer.update(cx, |composer, cx| {
            composer.busy = false;
            composer.editing = false;
            cx.notify();
        });
        let owner = cx.weak_entity();
        chat.history = cx.new(|cx| History::new(owner, cx));
        for message in next {
            chat.push_message(message, cx);
        }
        cx.notify();
        return;
    }
    for (index, message) in next.into_iter().enumerate() {
        if index >= count {
            chat.push_message(message, cx);
            continue;
        }
        let Some(old) = chat.message(message.id, cx) else {
            continue;
        };
        if old == &message {
            continue;
        }
        let same_tools = old.tools.len() == message.tools.len()
            && old
                .tools
                .iter()
                .zip(&message.tools)
                .all(|(old, new)| old.id == new.id);
        if let Some(row) = chat.mounted_message(message.id, cx).cloned()
            && same_tools
        {
            row.update(cx, |row, cx| {
                if let Some(delta) = message.text.strip_prefix(&row.data.text)
                    && row.data.attachments == message.attachments
                {
                    row.append(delta, cx);
                } else {
                    row.replace_user(message.text, message.attachments, cx);
                }
                if row.data.reasoning != message.reasoning {
                    row.set_reasoning(message.reasoning, cx);
                }
                for tool in message.tools {
                    row.set_tool(tool, cx);
                }
                if row.data.timestamp != message.timestamp {
                    row.data.timestamp = message.timestamp;
                    cx.notify();
                }
            });
        } else {
            let editable = chat.last_user == Some(message.id);
            chat.history.update(cx, |history, cx| {
                let disclosures = match history.rows.get_mut(index) {
                    Some(Row::Cold { disclosures, .. }) if same_tools => disclosures.take(),
                    _ => None,
                };
                history.rows[index] = Row::Cold {
                    data: message,
                    editable,
                    animate: false,
                    disclosures,
                };
                if index < history.list.item_count() {
                    history.list.remeasure_items(index..index + 1);
                }
                cx.notify();
            });
        }
    }
}

fn sync_streaming(chat: &mut ChatState, value: &Value, cx: &mut Context<ChatState>) {
    let next = value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0)
        .map(|id| MessageId(id as u64))
        .filter(|id| {
            chat.message(*id, cx)
                .is_some_and(|message| message.role == Role::Assistant)
        });
    let previous = chat.active.as_ref().map(|(id, _)| *id);
    if previous == next {
        return;
    }
    if let Some(previous) = previous {
        chat.finish(previous, cx);
        chat.stream_task = None;
    }
    if let Some(id) = next
        && let Some(&index) = chat.messages.get(&id)
        && let Some(row) = chat
            .history
            .update(cx, |history, cx| history.materialize(index, cx))
    {
        let (_, consumer) = crate::stream::channel(1);
        chat.active = Some((id, consumer.cancellation()));
        row.update(cx, |row, cx| {
            row.streaming = true;
            cx.notify();
        });
        chat.composer.update(cx, |composer, cx| {
            composer.editing = false;
            composer.busy = true;
            cx.notify();
        });
    }
}

fn event_payload(event: &ChatEvent) -> (usize, Value) {
    let id = |id: MessageId| Value::Num(id.0 as f64);
    let record = |id, text: &str, attachments: &[Attachment]| {
        let attachments = attachments
            .iter()
            .map(|attachment| {
                Value::Record(Arc::new(
                    [
                        ("id".into(), attachment.id.as_str().into()),
                        ("name".into(), attachment.name.as_str().into()),
                    ]
                    .into(),
                ))
            })
            .collect();
        Value::Record(Arc::new(
            [
                ("id".into(), id),
                ("text".into(), text.into()),
                ("attachments".into(), Value::List(attachments)),
            ]
            .into(),
        ))
    };
    match event {
        ChatEvent::Submitted {
            id: message,
            text,
            attachments,
        } => (0, record(id(*message), text, attachments)),
        ChatEvent::Retry(message) => (1, id(*message)),
        ChatEvent::Edited(message) => (2, id(*message)),
        ChatEvent::Stopped(message) => (3, id(*message)),
        ChatEvent::AttachmentRequested => (4, Value::Null),
        ChatEvent::Finished(message) => (5, id(*message)),
    }
}

#[derive(IntoElement)]
struct ChatDocument {
    key: ElementId,
    props: ChatProps,
    config: [Value; 6],
    handlers: [Option<EventHandler>; 6],
}

impl ChatDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let default = ChatProps::default();
        Self {
            key: node.state_id(),
            props: ChatProps {
                title: node.prop(0).as_str().map_or(default.title, str::to_owned),
                subtitle: node
                    .prop(1)
                    .as_str()
                    .map_or(default.subtitle, str::to_owned),
            },
            config: std::array::from_fn(|i| node.prop(i + 2).clone()),
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct Conversation {
    chat: Entity<ChatState>,
    config: [Value; 6],
    handlers: [Option<EventHandler>; 6],
    _subscription: Subscription,
}

impl RenderOnce for ChatDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let chat = cx.new(|cx| ChatState::with_props(self.props.clone(), window, cx));
            let subscription = cx.subscribe_in(
                &chat,
                window,
                |state: &mut Conversation, _, event, window, cx| {
                    let (index, payload) = event_payload(event);
                    if let Some(handler) = &state.handlers[index] {
                        handler(payload, window, cx);
                    }
                },
            );
            Conversation {
                chat,
                config: std::array::from_fn(|_| Value::Null),
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        let chat = state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            state.chat.update(cx, |chat, cx| {
                chat.set_props(self.props, cx);
                let [history, draft, files, commands, mentions, streaming] = &self.config;
                if state.config[0] != *history {
                    sync_messages(chat, messages(history), cx);
                }
                if state.config[1] != *draft
                    && let Some(text) = draft.as_str()
                {
                    chat.set_draft(text, window, cx);
                }
                if state.config[2] != *files {
                    chat.composer.update(cx, |composer, cx| {
                        composer.attachments = attachments(files);
                        cx.notify();
                    });
                }
                if state.config[3] != *commands || state.config[4] != *mentions {
                    chat.composer.update(cx, |composer, cx| {
                        composer.set_completions(completions(commands), completions(mentions), cx);
                    });
                }
                if state.config[5] != *streaming {
                    sync_streaming(chat, streaming, cx);
                }
            });
            state.config = self.config;
            state.chat.clone()
        });
        Chat::new(&chat)
    }
}
