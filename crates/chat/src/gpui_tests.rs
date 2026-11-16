use super::*;
use gpui_kit::{
    Bounds, TestAppContext, WindowBounds, WindowHandle, WindowOptions, base::Root, point, px, size,
    test::TestWindowExt,
};
use guiraffe_motion::MotionPrefs;
use std::{cell::RefCell, rc::Rc};

struct Host(Entity<ChatState>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Chat::new(&self.0)
    }
}
fn open(cx: &mut TestAppContext) -> (WindowHandle<Root>, Entity<ChatState>) {
    cx.update(|cx| {
        init(cx);
        MotionPrefs::set(
            MotionPrefs {
                reduced: true,
                ..MotionPrefs::global(cx)
            },
            cx,
        );
    });
    cx.update(|cx| {
        let mut chat = None;
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(900.), px(760.)),
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let state = cx.new(|cx| ChatState::new(window, cx));
                    chat = Some(state.clone());
                    {
                        let host = cx.new(|_| Host(state));
                        guiraffe_overlays::root(host, window, cx)
                    }
                },
            )
            .expect("window");
        (handle, chat.expect("chat"))
    })
}
fn draw<V: Render>(cx: &mut TestAppContext, handle: WindowHandle<V>) {
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("draw");
}
fn tick<V: Render>(cx: &mut TestAppContext, handle: WindowHandle<V>) {
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(20));
    draw(cx, handle);
}
#[gpui_kit::test]
fn enter_sends_shift_enter_newline_and_empty_is_ignored(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    let events = Rc::new(RefCell::new(Vec::new()));
    let out = events.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&chat, move |_, event, cx| {
            let _ = cx;
            out.borrow_mut().push(event.clone());
        })
    });
    cx.update_window(handle.into(), |_, window, cx| {
        chat.update(cx, |chat, cx| chat.set_draft("first", window, cx));
        window.render_frame(cx);
        window.press("shift-enter", cx);
        window.input("second", cx);
        window.press("enter", cx);
    })
    .expect("send");
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(chat.read(cx).message_count(), 1);
        assert_eq!(
            chat.read(cx)
                .message(MessageId(1), cx)
                .expect("message")
                .text,
            "first\nsecond"
        );
    });
    assert!(matches!(&events.borrow()[0],ChatEvent::Submitted {text,..} if text=="first\nsecond"));
    cx.update_window(handle.into(), |_, window, cx| window.press("enter", cx))
        .expect("empty");
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(chat.read(cx).message_count(), 1));
}
#[gpui_kit::test]
fn completion_keyboard_and_attachment_removal(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        chat.update(cx, |chat, cx| {
            chat.add_attachment(
                Attachment {
                    id: "one".into(),
                    name: "notes.md".into(),
                },
                cx,
            );
            chat.set_draft("/rev", window, cx);
        });
        window.render_frame(cx);
        window.press("tab", cx);
        assert_eq!(
            chat.read(cx)
                .composer
                .read(cx)
                .input
                .read(cx)
                .value()
                .as_str(),
            "/review "
        );
        chat.update(cx, |chat, cx| chat.set_draft("Look at @sel", window, cx));
        window.render_frame(cx);
        window.press("enter", cx);
        assert_eq!(
            chat.read(cx)
                .composer
                .read(cx)
                .input
                .read(cx)
                .value()
                .as_str(),
            "Look at @selection "
        );
        window.click("attachment-one", cx);
    })
    .expect("completion");
    cx.run_until_parked();
    cx.update(|cx| assert!(chat.read(cx).composer.read(cx).attachments.is_empty()));
}
#[gpui_kit::test]
fn stream_finishes_stops_and_rejects_stale_tokens(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    let (id, producer) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    producer.push("Hello ".into()).expect("chunk");
    tick(cx, handle);
    cx.update(|cx| {
        assert!(chat.read(cx).is_streaming());
        assert_eq!(
            chat.read(cx).message(id, cx).expect("message").text,
            "Hello "
        );
    });
    producer.push("world".into()).expect("chunk");
    drop(producer);
    tick(cx, handle);
    cx.update(|cx| {
        assert!(!chat.read(cx).is_streaming());
        assert_eq!(
            chat.read(cx).message(id, cx).expect("message").text,
            "Hello world"
        );
    });
    let (id, producer) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    producer.push("never visible".into()).expect("queued");
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("stop-chat", cx);
    })
    .expect("stop");
    tick(cx, handle);
    assert!(producer.is_cancelled());
    assert!(producer.push("late".into()).is_err());
    cx.update(|cx| assert_eq!(chat.read(cx).message(id, cx).expect("message").text, ""));
}
#[gpui_kit::test]
fn conversations_are_isolated_and_ids_are_unique(cx: &mut TestAppContext) {
    let (_, a) = open(cx);
    let (_, b) = open(cx);
    cx.update(|cx| {
        a.update(cx, |chat, cx| {
            assert!(chat.push_message(Message::new(MessageId(1), Role::User, "one"), cx));
            assert!(!chat.push_message(Message::new(MessageId(1), Role::User, "duplicate"), cx));
            chat.submit("two".into(), vec![], cx);
        });
        assert_eq!(a.read(cx).message_count(), 2);
        assert_eq!(b.read(cx).message_count(), 0);
        assert_eq!(
            a.read(cx).message(MessageId(1), cx).expect("one").text,
            "one"
        );
    });
}
#[gpui_kit::test]
fn edit_last_user_replaces_branch_and_copy_retry_emit(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    let events = Rc::new(RefCell::new(Vec::new()));
    let out = events.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&chat, move |_, event, _| {
            out.borrow_mut().push(event.clone())
        })
    });
    cx.update_window(handle.into(), |_, window, cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(Message::new(MessageId(1), Role::User, "original"), cx);
            chat.push_message(Message::new(MessageId(2), Role::Assistant, "answer"), cx);
        });
        window.render_frame(cx);
        window.within(("message", 2u64)).click("copy-message", cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some("answer".into())
        );
        window.within(("message", 2u64)).click("retry-message", cx);
        window.within(("message", 1u64)).click("edit-message", cx);
        chat.update(cx, |chat, cx| chat.set_draft("revised", window, cx));
        window.press("enter", cx);
    })
    .expect("edit");
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(chat.read(cx).message_count(), 1);
        assert_eq!(
            chat.read(cx)
                .message(MessageId(1), cx)
                .expect("revised")
                .text,
            "revised"
        );
        assert!(chat.read(cx).message(MessageId(2), cx).is_none());
    });
    assert!(events.borrow().contains(&ChatEvent::Retry(MessageId(2))));
    assert!(events.borrow().contains(&ChatEvent::Edited(MessageId(1))));
}
#[gpui_kit::test]
fn long_variable_history_virtualizes_and_jump_restores_tail(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            for i in 1..501 {
                chat.push_message(
                    Message::new(
                        MessageId(i),
                        Role::Assistant,
                        format!(
                            "Message {i}{}",
                            "\n\nMore context.".repeat((i % 4) as usize)
                        ),
                    ),
                    cx,
                );
            }
        })
    });
    draw(cx, handle);
    cx.update(|cx| {
        let h = chat.read(cx).history.read(cx);
        let rendered = h
            .rows
            .iter()
            .filter(|r| r.read(cx).renders.get() > 0)
            .count();
        assert!(rendered < 40, "virtualized rows: {rendered}");
        assert!(rendered > 0);
        assert!(h.list.is_following_tail());
    });
    cx.update(|cx| {
        let history = chat.read(cx).history.clone();
        history.update(cx, |history, cx| {
            history.list.pause_following_tail();
            history.list.scroll_to_reveal_item(0);
            history.sticky.user_scroll(false);
            cx.notify();
        });
    });
    draw(cx, handle);
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(Message::new(MessageId(999), Role::Assistant, "new"), cx);
        })
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("jump-latest", cx);
    })
    .expect("jump");
    draw(cx, handle);
    cx.update(|cx| {
        let h = chat.read(cx).history.read(cx);
        assert!(h.sticky.following());
        assert_eq!(h.sticky.unread(), 0);
        assert!(h.list.is_following_tail());
    });
}
// gpui-fast exposes its scroll-layer test switch; upstream has no scroll layers.
// An inherent method wins over this fallback on the fast backend.
#[allow(
    dead_code,
    reason = "fallback is unused when the fast backend provides an inherent method"
)]
trait ScrollLayersFallback {
    fn set_scroll_layers(&mut self, _enabled: bool) {}
}
impl ScrollLayersFallback for Window {}
#[gpui_kit::test]
fn streaming_retains_parent_and_previous_messages_without_composited_layers(
    cx: &mut TestAppContext,
) {
    stream_render_counts(cx, false);
}
#[gpui_kit::test]
fn default_composited_layers_bound_redraws_to_visible_messages(cx: &mut TestAppContext) {
    stream_render_counts(cx, true);
}
fn stream_render_counts(cx: &mut TestAppContext, composited_layers: bool) {
    let (handle, chat) = open(cx);
    cx.update_window(handle.into(), |_, window, _| {
        window.set_scroll_layers(composited_layers)
    })
    .expect("layer policy");
    cx.update_window(handle.into(), |_, window, cx| {
        chat.update(cx, |chat, cx| chat.set_draft("", window, cx))
    })
    .expect("focus composer");
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(
                Message::new(MessageId(1), Role::Assistant, "stable paragraph"),
                cx,
            );
        })
    });
    let (id, producer) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    // Settle native Markdown's background parsing and initial Presence.
    tick(cx, handle);
    draw(cx, handle);
    let (parent, previous, active, composer) = cx.update(|cx| {
        let chat = chat.read(cx);
        (
            chat.renders.clone(),
            chat.messages[&MessageId(1)].read(cx).renders.clone(),
            chat.messages[&id].read(cx).renders.clone(),
            chat.composer.read(cx).renders.clone(),
        )
    });
    let before = (parent.get(), previous.get(), active.get(), composer.get());
    for _ in 0..5 {
        producer.push("more ".into()).expect("chunk");
        cx.background_executor
            .advance_clock(Duration::from_millis(20));
        cx.update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .expect("frame");
        cx.run_until_parked();
    }
    assert!(active.get() > before.2, "active row must render");
    if gpui_kit::GPUI_FAST {
        assert_eq!(parent.get(), before.0, "chat parent retained");
        assert_eq!(composer.get(), before.3, "composer retained");
        if !composited_layers {
            assert_eq!(previous.get(), before.1, "earlier message retained");
        } else {
            assert!(
                previous.get() - before.1 <= 12,
                "bounded visible-row repaint"
            );
        }
    }
    cx.update(|cx| {
        assert_eq!(
            chat.read(cx).message(id, cx).expect("message").text,
            "more ".repeat(5)
        )
    });
}
#[gpui_kit::test]
fn tool_and_reasoning_toggle_with_keyboard_and_status_updates(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            let mut message = Message::new(MessageId(1), Role::Assistant, "Done.");
            message.reasoning = "Consider the invariant first.".into();
            message.tools.push(ToolCall {
                id: "t".into(),
                name: "read_file".into(),
                status: ToolStatus::Running,
                arguments: "src/lib.rs".into(),
                result: String::new(),
            });
            chat.push_message(message, cx);
        })
    });
    draw(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("toggle-tool", cx);
        window.press("space", cx);
        window.click("toggle-reasoning", cx);
        chat.update(cx, |chat, cx| {
            assert!(chat.update_tool(
                MessageId(1),
                ToolCall {
                    id: "t".into(),
                    name: "read_file".into(),
                    status: ToolStatus::Succeeded,
                    arguments: "src/lib.rs".into(),
                    result: "42 lines".into()
                },
                cx
            ));
        });
    })
    .expect("tool");
    draw(cx, handle);
    cx.update(|cx| {
        assert_eq!(
            chat.read(cx)
                .message(MessageId(1), cx)
                .expect("message")
                .tools[0]
                .status,
            ToolStatus::Succeeded
        );
        assert!(MotionPrefs::global(cx).reduced);
    });
}

#[gpui_kit::test]
fn header_history_and_composer_have_disjoint_bounds(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(Message::new(MessageId(1), Role::Assistant, "hello"), cx);
        })
    });
    draw(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        let header = window.find("chat-header").bounds();
        let history = window.find("chat-history").bounds();
        let composer = window.find("chat-composer").bounds();
        assert!(header.bottom() <= history.top());
        assert!(history.bottom() <= composer.top());
        assert!(composer.bottom() <= px(760.));
    })
    .expect("layout");
}

#[gpui_kit::test]
fn wheel_detaches_streaming_tail_and_jump_reattaches(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            for i in 1..31 {
                chat.push_message(
                    Message::new(
                        MessageId(i),
                        Role::Assistant,
                        format!("Historical message {i}"),
                    ),
                    cx,
                );
            }
        })
    });
    let (_, producer) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    tick(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.scroll(
            "chat-history",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(400.))),
            cx,
        );
    })
    .expect("wheel");
    draw(cx, handle);
    let anchor = cx.update(|cx| {
        let history = chat.read(cx).history.read(cx);
        assert!(!history.sticky.following());
        history.list.logical_scroll_top()
    });
    producer
        .push("A new paragraph.\n\n".repeat(20))
        .expect("delta");
    tick(cx, handle);
    draw(cx, handle);
    cx.update(|cx| {
        let history = chat.read(cx).history.read(cx);
        assert!(!history.sticky.following());
        assert_eq!(history.list.logical_scroll_top(), anchor);
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.click("jump-latest", cx)
    })
    .expect("jump");
    draw(cx, handle);
    cx.update(|cx| assert!(chat.read(cx).history.read(cx).list.is_following_tail()));
}
#[gpui_kit::test]
fn streamed_markdown_remeasures_growing_message(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    let (id, producer) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    tick(cx, handle);
    let height = cx.update(|cx| {
        chat.read(cx)
            .history
            .read(cx)
            .list
            .bounds_for_item(0)
            .expect("bounds")
            .size
            .height
    });
    producer
        .push("Paragraph with enough content to take space.\n\n".repeat(12))
        .expect("delta");
    tick(cx, handle);
    draw(cx, handle);
    cx.update(|cx| {
        let history = chat.read(cx).history.read(cx);
        assert!(
            history
                .list
                .bounds_for_item(0)
                .expect("grown bounds")
                .size
                .height
                > height + px(200.)
        );
        assert!(history.list.is_following_tail());
        assert_eq!(
            chat.read(cx)
                .message(id, cx)
                .expect("message")
                .text
                .matches("Paragraph")
                .count(),
            12
        );
    });
}
#[gpui_kit::test]
fn native_code_copy_copies_only_the_fence_contents(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(
                Message::new(
                    MessageId(1),
                    Role::Assistant,
                    "Before.\n\n```rust\nlet value = 42;\n```\n\nAfter.",
                ),
                cx,
            );
        })
    });
    draw(cx, handle);
    draw(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.click("copy-code", cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("let value = 42;")
        );
    })
    .expect("code copy");
}
#[gpui_kit::test]
fn cancelling_edit_preserves_history_and_releases_draft(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(Message::new(MessageId(1), Role::User, "original"), cx);
            chat.edit_last_user(window, cx);
        });
        window.render_frame(cx);
        window.click("cancel-edit", cx);
    })
    .expect("cancel edit");
    cx.update(|cx| {
        let chat = chat.read(cx);
        assert!(chat.editing.is_none());
        assert_eq!(chat.message_count(), 1);
        assert_eq!(
            chat.message(MessageId(1), cx).expect("message").text,
            "original"
        );
        assert!(chat.composer.read(cx).input.read(cx).value().is_empty());
    });
}
#[gpui_kit::test]
fn replacement_stream_cancels_old_producer_without_late_writes(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    let (old_id, old) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    old.push("stale".into()).expect("old queued");
    let (new_id, new) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    assert!(old.is_cancelled());
    new.push("fresh".into()).expect("fresh queued");
    drop(new);
    tick(cx, handle);
    cx.update(|cx| {
        let chat = chat.read(cx);
        assert_eq!(chat.message(old_id, cx).expect("old").text, "");
        assert_eq!(chat.message(new_id, cx).expect("new").text, "fresh");
        assert!(!chat.is_streaming());
    });
}

#[gpui_kit::test]
fn provider_reasoning_updates_and_removal_remeasure_the_row(cx: &mut TestAppContext) {
    let (handle, chat) = open(cx);
    let (id, _producer) = cx.update(|cx| chat.update(cx, |chat, cx| chat.begin_reply(cx)));
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            assert!(chat.set_reasoning(id, "Initial reasoning.", cx));
            assert!(!chat.set_reasoning(MessageId(999), "missing", cx));
        })
    });
    draw(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.click("toggle-reasoning", cx)
    })
    .expect("expand");
    draw(cx, handle);
    let initial = cx.update(|cx| {
        chat.read(cx)
            .history
            .read(cx)
            .list
            .bounds_for_item(0)
            .expect("bounds")
            .size
            .height
    });
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.set_reasoning(id, "More detail.\n\n".repeat(10), cx);
        })
    });
    draw(cx, handle);
    draw(cx, handle);
    cx.update(|cx| {
        assert!(
            chat.read(cx)
                .history
                .read(cx)
                .list
                .bounds_for_item(0)
                .expect("expanded bounds")
                .size
                .height
                > initial
        )
    });
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.set_reasoning(id, "", cx);
        })
    });
    draw(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("toggle-reasoning").is_none());
    })
    .expect("reasoning removed");
}
