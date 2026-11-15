//! Window-local Controls bridge. Register before creating a Root.
use crate::{
    ConfirmResult,
    confirm::{self, ConfirmSender},
    stack::{Dismiss, OverlayId, OverlayKind, OverlayStack, Policy},
    toast::{Toast, ToastId, ToastPosition, ToastQueue},
};
use gpui_kit::StatefulInteractiveElement;
use gpui_kit::base::{Root, RootPlugin, TestSupportExt};
use gpui_kit::component::{
    WindowExt, dialog::Dialog, notification::Notification, progress::Progress,
};
use gpui_kit::{
    Anchor, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Subscription, Task, Window, div, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use std::{
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

struct ToastCard {
    entity: Entity<Notification>,
    _subscription: Subscription,
    message: String,
    position: ToastPosition,
}
pub type ContextBuilder = Rc<dyn Fn(Dialog, &mut Window, &mut App) -> Dialog>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayError {
    MissingRoot,
    NotTopmost,
    DrawerAlreadyOpen,
    UnknownContext,
}
impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OverlayError {}
/// State belongs to the window Root, and is destroyed with it (including pending confirms).
#[derive(Default)]
pub struct OverlayHost {
    stack: OverlayStack,
    confirms: HashMap<OverlayId, ConfirmSender>,
    contexts: HashMap<String, ContextBuilder>,
    toasts: ToastQueue,
    cards: HashMap<ToastId, ToastCard>,
    timer: Option<Task<()>>,
    layer: Option<Entity<ToastLayer>>,
}
impl RootPlugin for OverlayHost {
    fn decorate(
        &self,
        surface: gpui_kit::AnyElement,
        _: &Root,
        _: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let escape = self.stack.top().is_none_or(|e| e.policy.escape);
        let _ = cx;
        div()
            .size_full()
            .capture_action(move |_: &gpui_kit::base::actions::Cancel, _, cx| {
                if escape {
                    cx.propagate();
                } else {
                    cx.stop_propagation();
                }
            })
            .child(surface)
    }
}
impl OverlayHost {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let host = cx.weak_entity();
        let layer = cx.new(|_| ToastLayer { host });
        Self {
            layer: Some(layer),
            ..Self::default()
        }
    }
    fn notify_layer(&self, cx: &mut App) {
        if let Some(layer) = &self.layer {
            layer.update(cx, |_, cx| cx.notify());
        }
    }
    pub fn entity(window: &Window, cx: &App) -> Result<Entity<Self>, OverlayError> {
        window
            .root::<Root>()
            .flatten()
            .and_then(|r| r.read(cx).plugin::<Self>())
            .ok_or(OverlayError::MissingRoot)
    }
    pub fn stack(&self) -> &OverlayStack {
        &self.stack
    }
    pub fn toasts(&self) -> &ToastQueue {
        &self.toasts
    }
    fn settled(window: &mut Window, cx: &mut App, id: OverlayId, result: bool) {
        if let Ok(host) = Self::entity(window, cx) {
            host.update(cx, |host, cx| {
                if host.stack.dismiss(id, Dismiss::Programmatic) {
                    if let Some(sender) = host.confirms.remove(&id) {
                        sender.resolve(result);
                    }
                    cx.notify();
                }
            });
        }
    }
    /// Close only the top managed layer, preserving Controls' focus restoration order.
    pub fn close(window: &mut Window, cx: &mut App, id: OverlayId) -> Result<(), OverlayError> {
        let host = Self::entity(window, cx)?;
        let kind = host
            .read(cx)
            .stack
            .top()
            .filter(|e| e.id == id)
            .map(|e| e.kind)
            .ok_or(OverlayError::NotTopmost)?;
        match kind {
            OverlayKind::Drawer => window.close_sheet(cx),
            _ => window.close_dialog(cx),
        }
        Self::settled(window, cx, id, false);
        Ok(())
    }
    pub fn close_all(window: &mut Window, cx: &mut App) -> Result<(), OverlayError> {
        loop {
            let host = Self::entity(window, cx)?;
            let id = host.read(cx).stack.top().map(|e| e.id);
            let Some(id) = id else { return Ok(()) };
            Self::close(window, cx, id)?;
        }
    }
    pub fn open(
        window: &mut Window,
        cx: &mut App,
        policy: Policy,
        build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    ) -> Result<OverlayId, OverlayError> {
        Self::open_inner(window, cx, policy, OverlayKind::Modal, None, build)
    }
    pub(crate) fn open_inner(
        window: &mut Window,
        cx: &mut App,
        policy: Policy,
        kind: OverlayKind,
        sender: Option<ConfirmSender>,
        build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    ) -> Result<OverlayId, OverlayError> {
        let host = Self::entity(window, cx)?;
        let id = host.update(cx, |host, cx| {
            let id = host.stack.push(kind, policy);
            if let Some(sender) = sender {
                host.confirms.insert(id, sender);
            }
            cx.notify();
            id
        });
        window.open_dialog(cx, move |dialog, window, cx| {
            let theme = GuiraffeTheme::global(cx);
            let surface = theme.surfaces();
            let dialog = dialog
                .rounded(px(theme.default_radius.radius()))
                .bg(surface.surface.to_hsla())
                .text_color(surface.text.to_hsla())
                .p(px(Size::Md.spacing()));
            build(dialog, window, cx)
                .keyboard(policy.escape)
                .overlay_closable(policy.outside)
                .on_close(move |_, window, cx| Self::settled(window, cx, id, false))
        });
        Ok(id)
    }
    pub fn confirm(
        window: &mut Window,
        cx: &mut App,
        title: impl Into<gpui_kit::SharedString>,
        message: impl Into<gpui_kit::SharedString>,
    ) -> Result<(OverlayId, ConfirmResult), OverlayError> {
        let (sender, result) = confirm::channel();
        let title = title.into();
        let message = message.into();
        let id = Self::open_inner(
            window,
            cx,
            Policy {
                outside: false,
                ..Default::default()
            },
            OverlayKind::Modal,
            Some(sender),
            move |dialog, _, _| {
                dialog
                    .title(title.clone())
                    .child(message.clone())
                    .button_props(
                        gpui_kit::component::dialog::DialogButtonProps::default().show_cancel(true),
                    )
                    .on_ok(|_, window, cx| {
                        if let Ok(host) = Self::entity(window, cx) {
                            host.update(cx, |host, _| {
                                if let Some(entry) = host.stack.top() {
                                    if let Some(sender) = host.confirms.get(&entry.id) {
                                        sender.resolve(true);
                                    }
                                }
                            });
                        }
                        true
                    })
            },
        )?;
        Ok((id, result))
    }
    pub fn register_context(
        window: &mut Window,
        cx: &mut App,
        name: impl Into<String>,
        build: ContextBuilder,
    ) -> Result<(), OverlayError> {
        Self::entity(window, cx)?.update(cx, |host, _| {
            host.contexts.insert(name.into(), build);
        });
        Ok(())
    }
    pub fn open_context(
        window: &mut Window,
        cx: &mut App,
        name: &str,
        policy: Policy,
    ) -> Result<OverlayId, OverlayError> {
        let host = Self::entity(window, cx)?;
        let build = host
            .read(cx)
            .contexts
            .get(name)
            .cloned()
            .ok_or(OverlayError::UnknownContext)?;
        Self::open(window, cx, policy, move |dialog, window, cx| {
            build(dialog, window, cx)
        })
    }
    /// Push or replace a toast. A single cancellable per-window timer services all visible cards.
    pub fn notify(window: &mut Window, cx: &mut App, toast: Toast) -> Result<(), OverlayError> {
        let host = Self::entity(window, cx)?;
        host.update(cx, |host, cx| {
            host.toasts.push(toast);
            host.sync_cards(cx);
            host.notify_layer(cx);
        });
        Ok(())
    }
    pub fn dismiss_toast(
        window: &mut Window,
        cx: &mut App,
        id: ToastId,
    ) -> Result<(), OverlayError> {
        Self::entity(window, cx)?.update(cx, |host, cx| {
            host.toasts.remove(id);
            host.sync_cards(cx);
            host.notify_layer(cx);
        });
        Ok(())
    }
    pub fn toast_limit(
        window: &mut Window,
        cx: &mut App,
        limit: usize,
    ) -> Result<(), OverlayError> {
        Self::entity(window, cx)?.update(cx, |host, cx| {
            host.toasts.set_limit(limit);
            host.sync_cards(cx);
            host.notify_layer(cx);
        });
        Ok(())
    }
    pub fn clear_toasts(window: &mut Window, cx: &mut App) -> Result<(), OverlayError> {
        Self::entity(window, cx)?.update(cx, |host, cx| {
            host.toasts.clear();
            host.cards.clear();
            host.timer = None;
            host.notify_layer(cx);
        });
        Ok(())
    }
    fn ensure_timer(&mut self, cx: &mut Context<Self>) {
        if self.timer.is_none() && self.toasts.visible().iter().any(|t| t.auto_close.is_some()) {
            self.timer = Some(cx.spawn(async move |weak, cx| {
                let mut last = Instant::now();
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(33))
                        .await;
                    let now = Instant::now();
                    let delta = now.saturating_duration_since(last);
                    last = now;
                    let Ok(active) = weak.update(cx, |host, cx| {
                        host.toasts.advance(delta);
                        host.sync_cards(cx);
                        host.notify_layer(cx);
                        host.toasts.visible().iter().any(|t| t.auto_close.is_some())
                    }) else {
                        break;
                    };
                    if !active {
                        let _ = weak.update(cx, |host, _| host.timer = None);
                        break;
                    }
                }
            }));
        }
    }
    fn sync_cards(&mut self, cx: &mut Context<Self>) {
        self.cards
            .retain(|id, _| self.toasts.visible().iter().any(|t| t.id == *id));
        for toast in self.toasts.visible() {
            let id = toast.id;
            let host = cx.weak_entity();
            if let Some(card) = self.cards.get_mut(&id) {
                card.entity.update(cx, |notification_view, cx| {
                    if card.message != toast.message || card.position != toast.position {
                        *notification_view = notification(toast, host);
                        card.message = toast.message.clone();
                        card.position = toast.position;
                    }
                    cx.notify();
                });
            } else {
                let entity = cx.new(|_| notification(toast, host));
                let subscription =
                    cx.subscribe(&entity, move |host, _, _: &gpui_kit::DismissEvent, cx| {
                        host.toasts.remove(id);
                        host.sync_cards(cx);
                        host.notify_layer(cx);
                    });
                self.cards.insert(
                    id,
                    ToastCard {
                        entity,
                        _subscription: subscription,
                        message: toast.message.clone(),
                        position: toast.position,
                    },
                );
            }
        }
        self.ensure_timer(cx);
    }
}
fn notification(toast: &Toast, host: gpui_kit::WeakEntity<OverlayHost>) -> Notification {
    let id = toast.id;
    Notification::new()
        .message(toast.message.clone())
        .autohide(false)
        .placement(anchor(toast.position))
        .content(move |_, _, cx| {
            let progress = host.upgrade().and_then(|host| {
                host.read(cx)
                    .toasts
                    .visible()
                    .iter()
                    .find(|t| t.id == id)
                    .and_then(Toast::progress)
            });
            let color = GuiraffeTheme::global(cx).primary_color().to_hsla();
            div()
                .children(progress.map(|value| {
                    Progress::new("toast-progress")
                        .value(value * 100.)
                        .color(color)
                }))
                .into_any_element()
        })
}

fn anchor(position: ToastPosition) -> Anchor {
    match position {
        ToastPosition::TopLeft => Anchor::TopLeft,
        ToastPosition::TopCenter => Anchor::TopCenter,
        ToastPosition::TopRight => Anchor::TopRight,
        ToastPosition::BottomLeft => Anchor::BottomLeft,
        ToastPosition::BottomCenter => Anchor::BottomCenter,
        ToastPosition::BottomRight => Anchor::BottomRight,
    }
}
struct ToastLayer {
    host: gpui_kit::WeakEntity<OverlayHost>,
}
impl Render for OverlayHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().absolute().inset_0().children(self.layer.clone())
    }
}
impl Render for ToastLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(host) = self.host.upgrade() else {
            return div().into_any_element();
        };
        let host = host.read(cx);
        let gap = Size::Sm.spacing();
        let width = px(Size::Xs.container_width() / 1.5);
        let viewport = window.viewport_size();
        div()
            .absolute()
            .inset_0()
            .children(ToastPosition::ALL.into_iter().map(|position| {
                let mut column = div().absolute().flex().flex_col().gap(px(gap)).w(width);
                column = match position {
                    ToastPosition::TopLeft | ToastPosition::TopCenter | ToastPosition::TopRight => {
                        column.top(px(gap))
                    }
                    _ => column.bottom(px(gap)),
                };
                column = match position {
                    ToastPosition::TopLeft | ToastPosition::BottomLeft => column.left(px(gap)),
                    ToastPosition::TopRight | ToastPosition::BottomRight => column.right(px(gap)),
                    _ => column.left((viewport.width - width) / 2.),
                };
                column.children(
                    host.toasts
                        .visible()
                        .iter()
                        .filter(|t| t.position == position)
                        .filter_map(|toast| {
                            let id = toast.id;
                            host.cards.get(&id).map(|card| {
                                div()
                                    .id(("guiraffe-toast", id.0))
                                    .test_support()
                                    .on_hover({
                                        let host = self.host.clone();
                                        move |hovered, _, cx| {
                                            let _ = host.update(cx, |host, _| {
                                                host.toasts.pause(id, *hovered)
                                            });
                                        }
                                    })
                                    .child(card.entity.clone())
                            })
                        }),
                )
            }))
            .into_any_element()
    }
}
/// Initialize Kit, theme and the window-local overlay plugin before opening windows.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    guiraffe_core::init(cx);
    Root::register_plugin::<OverlayHost>(cx, OverlayHost::new);
}
/// Build this Root inside `WindowManager::open` to keep its typed window handle.
pub fn root(view: impl Into<gpui_kit::AnyView>, window: &mut Window, cx: &mut App) -> Entity<Root> {
    match view.into().downcast::<Root>() {
        Ok(root) => root,
        Err(view) => cx.new(|cx| Root::new(view, window, cx)),
    }
}

/// Four-sided Controls sheet with GUIraffe sizing. Controls permits one sheet per window.
pub struct Drawer {
    pub side: crate::position::Side,
    pub size: Size,
    pub policy: Policy,
}
impl Default for Drawer {
    fn default() -> Self {
        Self {
            side: crate::position::Side::Right,
            size: Size::Md,
            policy: Policy::default(),
        }
    }
}
impl Drawer {
    pub fn open(
        self,
        window: &mut Window,
        cx: &mut App,
        build: impl Fn(
            gpui_kit::component::sheet::Sheet,
            &mut Window,
            &mut App,
        ) -> gpui_kit::component::sheet::Sheet
        + 'static,
    ) -> Result<OverlayId, OverlayError> {
        let host = OverlayHost::entity(window, cx)?;
        if !host.read(cx).stack.entries().is_empty() {
            return Err(OverlayError::DrawerAlreadyOpen);
        }
        let id = host.update(cx, |host, cx| {
            let id = host.stack.push(OverlayKind::Drawer, self.policy);
            cx.notify();
            id
        });
        let placement = match self.side {
            crate::position::Side::Top => gpui_kit::component::Placement::Top,
            crate::position::Side::Bottom => gpui_kit::component::Placement::Bottom,
            crate::position::Side::Left => gpui_kit::component::Placement::Left,
            crate::position::Side::Right => gpui_kit::component::Placement::Right,
        };
        window.open_sheet_at(placement, cx, move |sheet, window, cx| {
            let theme = GuiraffeTheme::global(cx);
            let surface = theme.surfaces();
            build(
                sheet
                    .size(px(self.size.container_width() / 2.))
                    .p(px(self.size.spacing()))
                    .bg(surface.surface.to_hsla())
                    .text_color(surface.text.to_hsla()),
                window,
                cx,
            )
            .overlay_closable(self.policy.outside)
            .on_close(move |_, window, cx| OverlayHost::settled(window, cx, id, false))
        });
        Ok(id)
    }
}
