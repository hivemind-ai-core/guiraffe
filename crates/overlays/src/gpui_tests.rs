use crate::{
    stack::Policy,
    toast::{Toast, ToastId},
    *,
};
use gpui_kit::base::{Root, TestSupportExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollDelta, Styled, TestAppContext, Window, WindowHandle, WindowOptions, div, point, px,
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll, Waker},
};
struct Plain {
    focus: FocusHandle,
    scrolls: Rc<Cell<usize>>,
    renders: Rc<Cell<usize>>,
}
impl Render for Plain {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let scrolls = self.scrolls.clone();
        div()
            .id("background")
            .test_support()
            .size_full()
            .track_focus(&self.focus)
            .on_scroll(move |_, _, _| scrolls.set(scrolls.get() + 1))
            .child("Background")
            .child(
                div()
                    .id("background-corner")
                    .test_support()
                    .absolute()
                    .top(px(40.))
                    .left_0()
                    .size(px(8.)),
            )
    }
}
fn open_with_content(cx: &mut TestAppContext) -> (WindowHandle<Root>, gpui_kit::Entity<Plain>) {
    cx.update(|cx| {
        let mut content = None;
        let handle = cx
            .open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    Plain {
                        focus,
                        scrolls: Rc::default(),
                        renders: Rc::default(),
                    }
                });
                content = Some(view.clone());
                root(view, window, cx)
            })
            .expect("window");
        (handle, content.expect("content"))
    })
}
pub(crate) fn open(cx: &mut TestAppContext) -> WindowHandle<Root> {
    open_with_content(cx).0
}

fn poll(result: &mut ConfirmResult) -> Poll<bool> {
    Pin::new(result).poll(&mut TaskContext::from_waker(Waker::noop()))
}
#[gpui_kit::test]
fn roots_are_idempotent_and_missing_roots_error(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    cx.update_window(handle.into(), |root_view, window, cx| {
        let original = window.root::<Root>().flatten().expect("root");
        assert_eq!(root(original.clone(), window, cx), original);
        assert!(
            root_view
                .downcast::<Root>()
                .expect("root")
                .read(cx)
                .plugin::<OverlayHost>()
                .is_some()
        );
    })
    .expect("window");
    let bare = cx
        .update(|cx| cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Bare)))
        .expect("bare");
    cx.update_window(bare.into(), |_, window, cx| {
        assert!(matches!(
            OverlayHost::entity(window, cx),
            Err(OverlayError::MissingRoot)
        ))
    })
    .expect("bare update");
}
struct Bare;
impl Render for Bare {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
#[gpui_kit::test]
fn stacked_modal_escape_policy_and_focus_restore(cx: &mut TestAppContext) {
    cx.update(init);
    let a = open(cx);
    let b = open(cx);
    let before = cx
        .update_window(a.into(), |_, window, cx| window.focused(cx))
        .expect("focus");
    let ids = cx
        .update_window(a.into(), |_, window, cx| {
            let first =
                OverlayHost::open(window, cx, Policy::default(), |d, _, _| d.title("First"))
                    .expect("first");
            let second = OverlayHost::open(
                window,
                cx,
                Policy {
                    escape: false,
                    outside: false,
                },
                |d, _, _| d.title("Second"),
            )
            .expect("second");
            window.render_frame(cx);
            window.press("tab", cx);
            let trap = gpui_kit::base::active_focus_trap(window, cx).expect("modal trap");
            assert!(trap.contains_focused(window, cx));
            window.press("escape", cx);
            assert_eq!(
                OverlayHost::entity(window, cx)
                    .expect("host")
                    .read(cx)
                    .stack()
                    .entries()
                    .len(),
                2
            );
            assert_eq!(
                OverlayHost::close(window, cx, first),
                Err(OverlayError::NotTopmost)
            );
            (first, second)
        })
        .expect("update");
    cx.update_window(b.into(), |_, window, cx| {
        assert!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .stack()
                .entries()
                .is_empty()
        )
    })
    .expect("b");
    cx.update_window(a.into(), |_, window, cx| {
        OverlayHost::close(window, cx, ids.1).expect("close second");
        OverlayHost::close(window, cx, ids.0).expect("close first");
        assert_eq!(window.focused(cx), before);
        assert!(
            !OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .stack()
                .scroll_locked()
        );
    })
    .expect("close");
}
#[gpui_kit::test]
fn confirmation_click_and_programmatic_cancel(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    let mut result = cx
        .update_window(handle.into(), |_, window, cx| {
            let (_, result) =
                OverlayHost::confirm(window, cx, "Delete?", "Continue?").expect("confirm");
            window.render_frame(cx);
            window.click("ok", cx);
            result
        })
        .expect("window");
    cx.run_until_parked();
    assert_eq!(poll(&mut result), Poll::Ready(true));
    let mut result = cx
        .update_window(handle.into(), |_, window, cx| {
            let (id, result) =
                OverlayHost::confirm(window, cx, "Delete?", "Continue?").expect("confirm");
            OverlayHost::close(window, cx, id).expect("close");
            result
        })
        .expect("window");
    assert_eq!(poll(&mut result), Poll::Ready(false));
}
#[gpui_kit::test]
fn drawer_four_sides_and_escape_policy(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    for side in [
        position::Side::Top,
        position::Side::Bottom,
        position::Side::Left,
        position::Side::Right,
    ] {
        cx.update_window(handle.into(), |_, window, cx| {
            let id = Drawer {
                side,
                policy: Policy {
                    escape: false,
                    ..Default::default()
                },
                ..Default::default()
            }
            .open(window, cx, |s, _, _| s.title("Drawer"))
            .expect("drawer");
            window.render_frame(cx);
            window.press("escape", cx);
            assert_eq!(
                OverlayHost::entity(window, cx)
                    .expect("host")
                    .read(cx)
                    .stack()
                    .top()
                    .map(|e| e.id),
                Some(id)
            );
            OverlayHost::close(window, cx, id).expect("close");
        })
        .expect("window");
    }
}
#[gpui_kit::test]
fn toast_hover_and_update_by_id(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        OverlayHost::toast_limit(window, cx, 1).expect("limit");
        OverlayHost::notify(window, cx, Toast::new(ToastId(1), "Saved")).expect("toast");
        OverlayHost::notify(window, cx, Toast::new(ToastId(2), "Queued")).expect("toast");
        window.render_frame(cx);
    })
    .expect("show");
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.hover(("guiraffe-toast", 1_u64), cx);
        let host = OverlayHost::entity(window, cx).expect("host");
        assert!(host.read(cx).toasts().visible()[0].paused);
        assert_eq!(host.read(cx).toasts().queued().len(), 1);
        OverlayHost::notify(window, cx, Toast::new(ToastId(1), "Updated")).expect("update");
        assert_eq!(host.read(cx).toasts().visible()[0].message, "Updated");
        OverlayHost::dismiss_toast(window, cx, ToastId(1)).expect("dismiss");
        assert_eq!(host.read(cx).toasts().visible()[0].id, ToastId(2));
    })
    .expect("window");
}
#[gpui_kit::test]
fn spotlight_typing_keyboard_and_click(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    let selected = Rc::new(RefCell::new(Vec::new()));
    let output = selected.clone();
    cx.update_window(handle.into(), |_, window, cx| {
        let palette = cx.new(|cx| {
            Spotlight::new(
                vec![
                    search::Command::new("open", "Open"),
                    search::Command::new("close", "Close"),
                ],
                window,
                cx,
                move |id, _, _| output.borrow_mut().push(id.to_owned()),
            )
        });
        Spotlight::open(&palette, window, cx).expect("palette");
        window.render_frame(cx);
        window.input("cl", cx);
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(*selected.borrow(), ["close"]);
}
#[gpui_kit::test]
fn modal_blocks_background_wheel(cx: &mut TestAppContext) {
    cx.update(init);
    let (handle, content) = open_with_content(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let scrolls = content.read(cx).scrolls.clone();
        OverlayHost::open(window, cx, Policy::default(), |d, _, _| d.title("Modal")).expect("open");
        window.render_frame(cx);
        window.scroll(
            "background",
            ScrollDelta::Pixels(point(px(0.), px(-100.))),
            cx,
        );
        assert_eq!(scrolls.get(), 0);
    })
    .expect("window");
}
#[gpui_kit::test]
fn context_modals_and_native_outside_dismissal(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        OverlayHost::register_context(window, cx, "help", Rc::new(|d, _, _| d.title("Help")))
            .expect("context");
        assert_eq!(
            OverlayHost::open_context(window, cx, "unknown", Policy::default()),
            Err(OverlayError::UnknownContext)
        );
        let id = OverlayHost::open_context(
            window,
            cx,
            "help",
            Policy {
                outside: false,
                ..Default::default()
            },
        )
        .expect("open");
        window.render_frame(cx);
        window.click("background-corner", cx);
        assert_eq!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .stack()
                .top()
                .map(|e| e.id),
            Some(id)
        );
        OverlayHost::close(window, cx, id).expect("close");
        OverlayHost::open_context(window, cx, "help", Policy::default()).expect("open");
        window.render_frame(cx);
        window.click("background-corner", cx);
        assert!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .stack()
                .entries()
                .is_empty()
        );
    })
    .expect("window");
}
struct Loading {
    loading: bool,
    clicks: Rc<Cell<usize>>,
}
impl Render for Loading {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("under-loading")
                    .test_support()
                    .size_full()
                    .on_mouse_down(gpui_kit::MouseButton::Left, move |_, _, _| {
                        clicks.set(clicks.get() + 1)
                    }),
            )
            .child(LoadingOverlay::new().visible(self.loading))
    }
}
#[gpui_kit::test]
fn loading_overlay_blocks_pointer_only_when_visible(cx: &mut TestAppContext) {
    cx.update(init);
    let clicks = Rc::new(Cell::new(0));
    let output = clicks.clone();
    let (handle, view) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| Loading {
                    loading: true,
                    clicks: output,
                })
            })
        })
        .expect("open");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("under-loading", cx);
        assert_eq!(clicks.get(), 0);
        view.update(cx, |view, cx| {
            view.loading = false;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("under-loading", cx);
        assert_eq!(clicks.get(), 1);
    })
    .expect("window");
}
#[gpui_kit::test]
fn managed_windows_keep_independent_overlay_entities(cx: &mut TestAppContext) {
    use guiraffe_windows::{OpenMode, QuitPolicy, WindowConfig, WindowKey, WindowManager};
    cx.update(|cx| {
        init(cx);
        WindowManager::init(
            WindowConfig {
                quit_policy: QuitPolicy::Never,
                ..Default::default()
            },
            cx,
        );
    });
    let mut handles = Vec::new();
    for key in ["one", "two"] {
        let handle = cx
            .update(|cx| {
                WindowManager::open(
                    WindowKey::kind(key),
                    OpenMode::Reuse,
                    WindowOptions::default(),
                    |window, cx| {
                        let bare = cx.new(|_| Bare);
                        root(bare, window, cx)
                    },
                    cx,
                )
            })
            .expect("managed window");
        handles.push(handle);
    }
    let first = cx
        .update_window(handles[0].into(), |_, window, cx| {
            OverlayHost::notify(window, cx, Toast::new(ToastId(1), "One")).expect("toast");
            OverlayHost::entity(window, cx).expect("host")
        })
        .expect("first");
    cx.update_window(handles[1].into(), |_, window, cx| {
        let second = OverlayHost::entity(window, cx).expect("host");
        assert_ne!(first, second);
        assert!(second.read(cx).toasts().visible().is_empty());
    })
    .expect("second");
}
#[gpui_kit::test]
fn window_destruction_cancels_pending_confirm(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    let mut result = cx
        .update_window(handle.into(), |_, window, cx| {
            let (_, result) =
                OverlayHost::confirm(window, cx, "Keep?", "Pending").expect("confirm");
            result
        })
        .expect("open");
    assert_eq!(poll(&mut result), Poll::Pending);
    cx.update_window(handle.into(), |_, window, _| window.remove_window())
        .expect("close");
    cx.run_until_parked();
    assert_eq!(poll(&mut result), Poll::Ready(false));
}
#[gpui_kit::test]
fn repeated_init_preserves_theme_and_window_state(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        guiraffe_core::apply(
            guiraffe_core::GuiraffeTheme {
                primary: guiraffe_core::ColorName::Violet,
                ..Default::default()
            },
            cx,
        );
        init(cx);
        assert_eq!(
            guiraffe_core::GuiraffeTheme::global(cx).primary,
            guiraffe_core::ColorName::Violet
        );
    });
    let handle = open(cx);
    cx.update(|cx| {
        guiraffe_core::apply(
            guiraffe_core::GuiraffeTheme {
                primary: guiraffe_core::ColorName::Violet,
                ..Default::default()
            },
            cx,
        );
        init(cx);
        assert_eq!(
            guiraffe_core::GuiraffeTheme::global(cx).primary,
            guiraffe_core::ColorName::Violet
        );
    });
    cx.update_window(handle.into(), |_, window, cx| {
        assert!(OverlayHost::entity(window, cx).is_ok())
    })
    .expect("window");
}
#[gpui_kit::test]
fn toast_progress_does_not_notify_application_content(cx: &mut TestAppContext) {
    use std::time::Duration;
    cx.update(init);
    let (handle, content) = open_with_content(cx);
    let renders = cx
        .update_window(handle.into(), |_, window, cx| {
            OverlayHost::notify(window, cx, Toast::new(ToastId(1), "Progress")).expect("toast");
            window.render_frame(cx);
            content.read(cx).renders.clone()
        })
        .expect("window");
    cx.run_until_parked();
    cx.background_executor.advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("draw");
    let before = renders.get();
    cx.background_executor
        .advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("draw");
    assert_eq!(renders.get(), before, "progress is a leaf update");
}
#[gpui_kit::test]
fn native_toast_close_promotes_queue_and_timer_expires(cx: &mut TestAppContext) {
    use std::time::Duration;
    cx.update(init);
    let handle = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        OverlayHost::toast_limit(window, cx, 1).expect("limit");
        for id in [1, 2] {
            OverlayHost::notify(window, cx, Toast::new(ToastId(id), "Toast")).expect("toast");
        }
        window.render_frame(cx);
    })
    .expect("show");
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.hover(("guiraffe-toast", 1_u64), cx);
        window.click("close", cx);
    })
    .expect("window");
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        let host = OverlayHost::entity(window, cx).expect("host");
        assert_eq!(host.read(cx).toasts().visible()[0].id, ToastId(2));
    })
    .expect("promoted");
    cx.background_executor.advance_clock(Duration::from_secs(6));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        assert!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .toasts()
                .visible()
                .is_empty()
        )
    })
    .expect("expired");
}
#[gpui_kit::test]
fn disabling_escape_keeps_pointer_close_available(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        Drawer::new()
            .close_on_escape(false)
            .open(window, cx, |sheet, _, _| sheet.title("Drawer"))
            .expect("drawer");
        window.render_frame(cx);
        window.press("escape", cx);
        assert_eq!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .stack()
                .entries()
                .len(),
            1
        );
    })
    .expect("open");
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("close", cx);
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        assert!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .stack()
                .entries()
                .is_empty()
        )
    })
    .expect("closed");
}
#[gpui_kit::test]
fn spotlight_arrows_and_pointer_select_commands(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    let output = Rc::new(RefCell::new(Vec::new()));
    for keyboard in [true, false] {
        let output = output.clone();
        cx.update_window(handle.into(), |_, window, cx| {
            let palette = cx.new(|cx| {
                Spotlight::new(
                    vec![
                        search::Command::new("one", "One"),
                        search::Command::new("two", "Two"),
                    ],
                    window,
                    cx,
                    move |id, _, _| output.borrow_mut().push(id.to_owned()),
                )
            });
            let id = Spotlight::open(&palette, window, cx).expect("open");
            assert_eq!(Spotlight::open(&palette, window, cx), Ok(id));
            window.render_frame(cx);
            if keyboard {
                window.press("down", cx);
                window.press("enter", cx);
            } else {
                window.click(("command", 1_usize), cx);
            }
        })
        .expect("window");
        cx.run_until_parked();
    }
    assert_eq!(*output.borrow(), ["two", "two"]);
}

struct AffixDemo {
    position: toast::ToastPosition,
}
impl Render for AffixDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("affix-parent")
            .test_support()
            .relative()
            .size_full()
            .child(
                Affix::new()
                    .position(self.position)
                    .child(div().id("affixed").test_support().size(px(32.))),
            )
    }
}
#[gpui_kit::test]
fn affix_anchors_all_six_positions(cx: &mut TestAppContext) {
    cx.update(init);
    let (handle, view) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| AffixDemo {
                    position: toast::ToastPosition::TopLeft,
                })
            })
        })
        .expect("window");
    for position in toast::ToastPosition::ALL {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.position = position;
                cx.notify();
            });
            window.render_frame(cx);
            let parent = window.find("affix-parent").bounds();
            let marker = window.find("affixed").bounds();
            let offset = px(guiraffe_core::Size::Md.spacing());
            use toast::ToastPosition::*;
            let expected_x = match position {
                TopLeft | BottomLeft => parent.origin.x + offset,
                TopRight | BottomRight => {
                    parent.origin.x + parent.size.width - offset - marker.size.width
                }
                _ => parent.origin.x + (parent.size.width - marker.size.width) / 2.,
            };
            let expected_y = match position {
                TopLeft | TopCenter | TopRight => parent.origin.y + offset,
                _ => parent.origin.y + parent.size.height - offset - marker.size.height,
            };
            assert_eq!(marker.origin, point(expected_x, expected_y));
        })
        .expect("position");
    }
}

#[gpui_kit::test]
fn review_1_system_delivery_still_stores_managed_toast(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, notification::NotificationDelivery, theme::Theme};
    cx.update(init);
    let handle = open(cx);
    cx.update(|cx| Theme::global_mut(cx).notification.delivery = NotificationDelivery::System);
    cx.update_window(handle.into(), |_, window, cx| {
        OverlayHost::notify(window, cx, Toast::new(ToastId(99), "managed")).expect("toast");
        assert_eq!(
            window.notifications(cx).len(),
            1,
            "managed delivery must be InApp"
        );
    })
    .expect("window");
}
#[gpui_kit::test]
fn review_1_unmanaged_notifications_do_not_hide_admitted_toasts(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, notification::Notification, theme::Theme};
    cx.update(init);
    let handle = open(cx);
    cx.update(|cx| Theme::global_mut(cx).notification.max_items = 1);
    cx.update_window(handle.into(), |_, window, cx| {
        window.push_notification(Notification::new().message("unmanaged").autohide(false), cx);
        OverlayHost::notify(window, cx, Toast::new(ToastId(1), "managed")).expect("toast");
        let host = OverlayHost::entity(window, cx).expect("host");
        assert!(
            host.read(cx).toasts().visible().is_empty(),
            "occupied native capacity must queue managed work"
        );
        assert_eq!(host.read(cx).toasts().queued().len(), 1);
    })
    .expect("window");
}
#[gpui_kit::test]
fn review_2_update_retains_native_entity_and_order(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt;
    cx.update(init);
    let handle = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        for id in [1, 2] {
            OverlayHost::notify(window, cx, Toast::new(ToastId(id), "old")).expect("toast");
        }
        let before: Vec<_> = window
            .notifications(cx)
            .iter()
            .map(|n| n.entity_id())
            .collect();
        OverlayHost::notify(window, cx, Toast::new(ToastId(1), "new")).expect("update");
        let after: Vec<_> = window
            .notifications(cx)
            .iter()
            .map(|n| n.entity_id())
            .collect();
        assert_eq!(
            before, after,
            "updating must not reinsert native notifications"
        );
    })
    .expect("window");
}
#[gpui_kit::test]
fn review_1_later_unmanaged_arrival_requeues_and_releases_capacity(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, notification::Notification, theme::Theme};
    struct Other;
    cx.update(init);
    let handle = open(cx);
    cx.update(|cx| Theme::global_mut(cx).notification.max_items = 1);
    cx.update_window(handle.into(), |_, window, cx| {
        OverlayHost::notify(window, cx, Toast::new(ToastId(1), "managed")).expect("toast");
        window.push_notification(
            Notification::new()
                .id::<Other>()
                .message("unmanaged")
                .autohide(false),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("show");
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        let host = OverlayHost::entity(window, cx).expect("host");
        assert!(host.read(cx).toasts().visible().is_empty());
        assert_eq!(host.read(cx).toasts().queued().len(), 1);
        window.remove_notification::<Other>(cx);
    })
    .expect("requeue");
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("frame");
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        assert_eq!(
            OverlayHost::entity(window, cx)
                .expect("host")
                .read(cx)
                .toasts()
                .visible()
                .len(),
            1
        )
    })
    .expect("promoted");
}
#[gpui_kit::test]
fn review_4_stacked_confirm_resolves_its_own_id(cx: &mut TestAppContext) {
    cx.update(init);
    let handle = open(cx);
    let (mut first, mut second) = cx
        .update_window(handle.into(), |_, window, cx| {
            let (_, first) = OverlayHost::confirm(window, cx, "First", "One").expect("first");
            let first_focus = window.focused(cx).expect("first focus");
            let (_, second) = OverlayHost::confirm(window, cx, "Second", "Two").expect("second");
            window.render_frame(cx);
            window.focus(&first_focus, cx);
            window.dispatch_action(
                Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                cx,
            );
            (first, second)
        })
        .expect("window");
    cx.run_until_parked();
    assert_eq!(
        poll(&mut first),
        Poll::Ready(true),
        "the invoked confirm must resolve itself"
    );
    assert_ne!(
        poll(&mut second),
        Poll::Ready(true),
        "a sibling confirm must never be accepted"
    );
}
