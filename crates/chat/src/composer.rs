use crate::{
    Attachment, ChatEvent,
    commands::{self, TriggerKind},
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    component::{
        Disableable,
        input::{InputEvent, Textarea, TextareaState},
    },
    prelude::*,
    *,
};
use guiraffe_components::Text;
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StyleContext, Variant};
use guiraffe_overlays::search::{Command, SearchIndex};

actions!(
    guiraffe_chat,
    [
        AcceptCompletion,
        NextCompletion,
        PreviousCompletion,
        DismissCompletion
    ]
);
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", AcceptCompletion, Some("GrChatCompletion > Input")),
        KeyBinding::new("tab", AcceptCompletion, Some("GrChatCompletion > Input")),
        KeyBinding::new("down", NextCompletion, Some("GrChatCompletion > Input")),
        KeyBinding::new("up", PreviousCompletion, Some("GrChatCompletion > Input")),
        KeyBinding::new(
            "escape",
            DismissCompletion,
            Some("GrChatCompletion > Input"),
        ),
    ]);
}
pub struct Composer {
    pub(crate) input: Entity<TextareaState>,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) busy: bool,
    pub(crate) editing: bool,
    #[cfg(test)]
    pub(crate) renders: std::rc::Rc<std::cell::Cell<usize>>,
    owner: WeakEntity<crate::ChatState>,
    commands: Vec<Command>,
    mentions: Vec<Command>,
    slash_index: SearchIndex,
    mention_index: SearchIndex,
    matches: Vec<(String, String)>,
    selected: usize,
    _subscription: Subscription,
}
impl Composer {
    pub(crate) fn new(
        owner: WeakEntity<crate::ChatState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .submit_on_enter(true)
                .placeholder("Ask anything, / for commands, @ for context…")
        });
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::Change => this.refresh(cx),
                InputEvent::PressEnter { shift: false, .. } => {
                    if this.matches.is_empty() {
                        this.send(window, cx);
                    } else {
                        this.accept(window, cx);
                    }
                }
                _ => {}
            });
        let commands = vec![
            Command::new("help", "Show available commands"),
            Command::new("review", "Review the current changes"),
            Command::new("explain", "Explain selected code"),
            Command::new("test", "Write focused tests"),
        ];
        let mentions = vec![
            Command::new("workspace", "Current workspace"),
            Command::new("selection", "Editor selection"),
        ];
        Self {
            input,
            attachments: vec![],
            busy: false,
            editing: false,
            #[cfg(test)]
            renders: Default::default(),
            owner,
            slash_index: SearchIndex::new(&commands),
            mention_index: SearchIndex::new(&mentions),
            commands,
            mentions,
            matches: vec![],
            selected: 0,
            _subscription: subscription,
        }
    }
    pub fn input(&self) -> &Entity<TextareaState> {
        &self.input
    }
    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    pub fn set_completions(
        &mut self,
        commands: Vec<Command>,
        mentions: Vec<Command>,
        cx: &mut Context<Self>,
    ) {
        self.slash_index = SearchIndex::new(&commands);
        self.mention_index = SearchIndex::new(&mentions);
        self.commands = commands;
        self.mentions = mentions;
        self.refresh(cx);
    }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let input = self.input.read(cx);
        let value = input.value();
        let trigger = commands::trigger(&value, input.cursor());
        self.matches.clear();
        self.selected = 0;
        if let Some(trigger) = trigger {
            let (index, entries) = match trigger.kind {
                TriggerKind::Slash => (&mut self.slash_index, &self.commands),
                TriggerKind::Mention => (&mut self.mention_index, &self.mentions),
            };
            self.matches.extend(
                index
                    .search(trigger.query, 6)
                    .iter()
                    .map(|&i| (entries[i].id.clone(), entries[i].label.clone())),
            );
        }
        cx.notify();
    }
    pub(crate) fn set_draft(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            let line = text.bytes().filter(|b| *b == b'\n').count() as u32;
            let column = text
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .encode_utf16()
                .count() as u32;
            input.set_value(text, window, cx);
            input.set_cursor_position(
                gpui_kit::base::input::Position::new(line, column),
                window,
                cx,
            );
        });
        self.refresh(cx);
    }
    pub(crate) fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((value, _)) = self.matches.get(self.selected) else {
            return;
        };
        let input = self.input.read(cx);
        let text = input.value();
        let replacement = commands::trigger(&text, input.cursor())
            .and_then(|t| commands::complete(&text, &t, value));
        if let Some(text) = replacement {
            self.set_draft(text, window, cx);
        }
    }
    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        let attachments = std::mem::take(&mut self.attachments);
        self.set_draft(String::new(), window, cx);
        let _ = self
            .owner
            .update(cx, |chat, cx| chat.submit(text, attachments, cx));
    }
    pub(crate) fn remove_attachment(&mut self, id: &str, cx: &mut Context<Self>) {
        let old = self.attachments.len();
        self.attachments.retain(|a| a.id != id);
        if old != self.attachments.len() {
            cx.notify();
        }
    }
}
impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        let theme = GuiraffeTheme::global(cx);
        let s = theme.surfaces();
        let sc = StyleContext::current();
        let gap = px(sc.density.space(Size::Xs));
        let owner = self.owner.clone();
        let stop = owner.clone();
        div()
            .id("chat-composer")
            .test_support()
            .flex_none()
            .flex()
            .flex_col()
            .gap(gap)
            .p(px(sc.density.space(Size::Sm)))
            .border_t_1()
            .border_color(s.border.to_hsla())
            .bg(s.body.to_hsla())
            .key_context(if self.matches.is_empty() {
                "GrChatComposer"
            } else {
                "GrChatCompletion"
            })
            .on_action(
                cx.listener(|this, _: &AcceptCompletion, window, cx| this.accept(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NextCompletion, _, cx| {
                if !this.matches.is_empty() {
                    this.selected = (this.selected + 1) % this.matches.len();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousCompletion, _, cx| {
                if !this.matches.is_empty() {
                    this.selected = (this.selected + this.matches.len() - 1) % this.matches.len();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &DismissCompletion, _, cx| {
                this.matches.clear();
                cx.notify();
            }))
            .when(!self.matches.is_empty(), |el| {
                el.child(
                    div()
                        .id("chat-completions")
                        .flex()
                        .flex_col()
                        .gap(gap)
                        .p(gap)
                        .rounded(px(theme.default_radius.radius()))
                        .bg(s.surface.to_hsla())
                        .children(self.matches.iter().enumerate().map(|(i, (id, label))| {
                            crate::style::button(("completion", i), Variant::Light, None, cx)
                                .label(format!(
                                    "{} {id} · {label}",
                                    if i == self.selected { "›" } else { " " }
                                ))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.selected = i;
                                    this.accept(window, cx);
                                }))
                        })),
                )
            })
            .when(!self.attachments.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(gap)
                        .children(self.attachments.iter().map(|a| {
                            let id = a.id.clone();
                            crate::style::button(
                                SharedString::from(format!("attachment-{}", a.id)),
                                Variant::Default,
                                None,
                                cx,
                            )
                            .label(format!("{} ×", a.name))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.remove_attachment(&id, cx)),
                            )
                        })),
                )
            })
            .when(self.editing, |el| {
                el.child(
                    crate::style::button(
                        "cancel-edit",
                        Variant::Light,
                        Some(ColorName::Yellow),
                        cx,
                    )
                    .label("Editing your message · Cancel")
                    .on_click({
                        let owner = self.owner.clone();
                        move |_, window, cx| {
                            let _ = owner.update(cx, |chat, cx| chat.cancel_edit(window, cx));
                        }
                    }),
                )
            })
            .child(Textarea::new(&self.input).aria_label("Message to assistant"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(gap)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(gap)
                            .child(
                                crate::style::button("attach-file", Variant::Default, None, cx)
                                    .label("Attach")
                                    .on_click(move |_, _, cx| {
                                        let _ = owner.update(cx, |_, cx| {
                                            cx.emit(ChatEvent::AttachmentRequested)
                                        });
                                    }),
                            )
                            .child(
                                Text::new("Enter to send · Shift+Enter for a new line")
                                    .dimmed(true)
                                    .size(Size::Xs),
                            ),
                    )
                    .when(self.busy, |el| {
                        el.child(
                            crate::style::button(
                                "stop-chat",
                                Variant::Filled,
                                Some(ColorName::Red),
                                cx,
                            )
                            .label("Stop generating")
                            .on_click(move |_, _, cx| {
                                let _ = stop.update(cx, |chat, cx| chat.stop(cx));
                            }),
                        )
                    })
                    .when(!self.busy, |el| {
                        el.child(
                            crate::style::button("send-chat", Variant::Filled, None, cx)
                                .label("Send")
                                .disabled(
                                    self.input.read(cx).value().trim().is_empty()
                                        && self.attachments.is_empty(),
                                )
                                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                        )
                    }),
            )
    }
}
