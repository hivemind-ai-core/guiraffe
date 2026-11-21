//! The shared selection context menu on chat message text.
use super::*;
use gpui_kit::{
    AnyWindowHandle, Bounds, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    PlatformInput, Point, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use guiraffe_motion::MotionPrefs;
use std::time::Duration;

struct Host(Entity<ChatState>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Chat::new(&self.0)
    }
}

fn settle(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.draw(cx).clear(cx))
        .expect("draw");
}

fn right_click(cx: &mut TestAppContext, handle: AnyWindowHandle, p: Point<Pixels>) {
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: p,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Right,
                position: p,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
            cx,
        );
    })
    .expect("right click");
    settle(cx, handle);
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()))
}

#[gpui_kit::test]
fn message_text_context_menu_copies_selection_and_message(cx: &mut TestAppContext) {
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
    let mut chat = None;
    let handle = cx
        .update(|cx| {
            cx.open_window(
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
                    let host = cx.new(|_| Host(state));
                    guiraffe_overlays::root(host, window, cx)
                },
            )
        })
        .expect("window")
        .into();
    let chat = chat.expect("chat");
    cx.update(|cx| {
        chat.update(cx, |chat, cx| {
            chat.push_message(
                Message::new(MessageId(1), Role::Assistant, "Hello **menu** world"),
                cx,
            );
        })
    });
    settle(cx, handle);
    settle(cx, handle);
    let view = cx.update(|cx| {
        chat.read(cx)
            .mounted_message(MessageId(1), cx)
            .expect("mounted message")
            .clone()
    });
    let origin = cx.update(|cx| {
        view.read(cx)
            .markdown
            .as_ref()
            .expect("rendered message")
            .read(cx)
            .bounds()
            .origin
    });
    let p = origin + point(px(6.), px(6.));
    right_click(cx, handle, p);
    let menu = cx.update(|cx| view.read(cx).menu.clone().expect("rendered message menu"));
    assert!(menu.is_open(), "right-click on message text opens the menu");
    assert_eq!(menu.position(), Some(p));
    let copy = |m: &guiraffe_core::TextMenu| {
        m.entries()
            .into_iter()
            .find(|(l, _)| l.as_ref() == "Copy")
            .map(|(_, e)| e)
    };
    assert_eq!(copy(&menu), Some(false));
    cx.update_window(handle, |_, window, cx| {
        assert!(menu.activate("Select all", window, cx));
    })
    .expect("select all");
    right_click(cx, handle, p);
    assert_eq!(copy(&menu), Some(true));
    cx.update_window(handle, |_, window, cx| {
        assert!(menu.activate("Copy", window, cx));
    })
    .expect("copy");
    let selected = cx.update(|cx| {
        view.read(cx)
            .markdown
            .as_ref()
            .expect("rendered message")
            .read(cx)
            .selected_text()
    });
    assert!(selected.contains("menu"));
    assert_eq!(clipboard(cx), Some(selected));
    right_click(cx, handle, p);
    cx.update_window(handle, |_, window, cx| {
        assert!(menu.activate("Copy message", window, cx));
    })
    .expect("copy message");
    assert_eq!(clipboard(cx), Some("Hello **menu** world".into()));
    right_click(cx, handle, p);
    assert!(menu.is_open());
    cx.update_window(handle, |_, window, cx| {
        use gpui_kit::test::TestWindowExt;
        window.press("escape", cx);
    })
    .expect("escape");
    cx.run_until_parked();
    assert!(!menu.is_open());
}
