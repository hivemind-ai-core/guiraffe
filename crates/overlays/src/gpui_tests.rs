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
            .on_scroll_wheel(move |_, _, _| scrolls.set(scrolls.get() + 1))
            .child("Background")
            .child(
                div()
                    .id("background-corner")
                    .test_support()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(8.)),
            )
    }
}
fn open(cx: &mut TestAppContext) -> WindowHandle<Root> {
    cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Plain {
                    focus,
                    scrolls: Rc::default(),
                    renders: Rc::default(),
                }
            })
        })
        .expect("window")
        .0
        .downcast::<Root>()
        .expect("root")
    })
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
                    ..Default::default()
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
    let handle = open(cx);
    cx.update_window(handle.into(), |root, window, cx| {
        let content = root
            .downcast::<Root>()
            .expect("root")
            .read(cx)
            .view()
            .clone()
            .downcast::<Plain>()
            .expect("plain");
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
            .child(LoadingOverlay::new(self.loading))
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
