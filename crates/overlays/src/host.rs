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
    Render, Styled, Task, Window, div, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use std::{collections::HashMap, rc::Rc, time::Duration};

struct ToastCard {
    entity: Entity<Notification>,
    message: String,
    position: ToastPosition,
    elapsed: Duration,
    auto_close: Option<Duration>,
    generation: u64,
}
gpui_kit::actions!(guiraffe_overlays, [IgnoreEscape]);
pub type ContextBuilder = Rc<dyn Fn(Dialog, &mut Window, &mut App) -> Dialog>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayError {
    MissingRoot,
    NotTopmost,
    DrawerRequiresEmptyStack,
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
    next_generation: u64,
}
impl RootPlugin for OverlayHost {
    fn decorate(
        &self,
        surface: gpui_kit::AnyElement,
        _: &Root,
        _: &mut Window,
        _: &mut App,
    ) -> impl IntoElement {
        let escape = self.stack.top().is_none_or(|e| e.policy.escape);
        div()
            .size_full()
            .key_context(if escape {
                "GrModal"
            } else {
                "GrModalEscapeDisabled"
            })
            .on_action(|_: &IgnoreEscape, _, cx| cx.stop_propagation())
            .child(surface)
    }
}
impl OverlayHost {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
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
        let id =
            Self::open_inner(
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
                        .content({
                            let message = message.clone();
                            move |content, _, _| content.child(message.clone())
                        })
                        .footer(
                            gpui_kit::component::dialog::DialogFooter::new()
                                .child(
                                    gpui_kit::component::dialog::DialogClose::new()
                                        .trigger(|button| button.label("Cancel")),
                                )
                                .child(gpui_kit::component::dialog::DialogAction::new().child(
                                    gpui_kit::component::button::Button::new("ok").label("OK"),
                                )),
                        )
                        .on_ok(|_, window, cx| {
                            if let Ok(host) = Self::entity(window, cx) {
                                host.update(cx, |host, _| {
                                    if let Some(entry) = host.stack.top()
                                        && let Some(sender) = host.confirms.get(&entry.id)
                                    {
                                        sender.resolve(true);
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
            let cap = gpui_kit::component::theme::Theme::global(cx)
                .notification
                .max_items;
            host.toasts.set_limit(host.toasts.limit().min(cap));
            host.toasts.push(toast);
            host.sync_cards(window, cx);
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
            host.sync_cards(window, cx);
        });
        Ok(())
    }
    pub fn toast_limit(
        window: &mut Window,
        cx: &mut App,
        limit: usize,
    ) -> Result<(), OverlayError> {
        Self::entity(window, cx)?.update(cx, |host, cx| {
            let cap = gpui_kit::component::theme::Theme::global(cx)
                .notification
                .max_items;
            host.toasts.set_limit(limit.min(cap));
            host.sync_cards(window, cx);
        });
        Ok(())
    }
    pub fn clear_toasts(window: &mut Window, cx: &mut App) -> Result<(), OverlayError> {
        Self::entity(window, cx)?.update(cx, |host, cx| {
            for id in host.cards.keys() {
                window.remove_notification1::<ManagedToast>(id.0, cx);
            }
            host.toasts.clear();
            host.cards.clear();
            host.timer = None;
        });
        Ok(())
    }
    fn ensure_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.timer.is_some() || !self.toasts.visible().iter().any(|t| t.auto_close.is_some()) {
            return;
        }
        self.timer = Some(cx.spawn_in(window, async move |weak, cx| {
            let mut last = cx.background_executor().now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let now = cx.background_executor().now();
                let delta = now.saturating_duration_since(last);
                last = now;
                let Ok(active) = weak.update_in(cx, |host, window, cx| {
                    host.toasts.advance(delta);
                    host.sync_cards(window, cx);
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
    fn sync_cards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cards.retain(|id, _| {
            if self.toasts.visible().iter().any(|t| t.id == *id) {
                true
            } else {
                window.remove_notification1::<ManagedToast>(id.0, cx);
                false
            }
        });
        for toast in self.toasts.visible() {
            if let Some(card) = self.cards.get_mut(&toast.id)
                && card.message == toast.message
                && card.position == toast.position
            {
                if card.elapsed != toast.elapsed || card.auto_close != toast.auto_close {
                    card.elapsed = toast.elapsed;
                    card.auto_close = toast.auto_close;
                    card.entity.update(cx, |_, cx| cx.notify());
                }
                continue;
            }
            self.next_generation += 1;
            let generation = self.next_generation;
            let before = window.notifications(cx);
            let host = cx.weak_entity();
            window.push_notification(notification(toast, host, generation), cx);
            let after = window.notifications(cx);
            if let Some(entity) = after.iter().find(|entity| {
                !before
                    .iter()
                    .any(|old| old.entity_id() == entity.entity_id())
            }) {
                self.cards.insert(
                    toast.id,
                    ToastCard {
                        entity: entity.clone(),
                        message: toast.message.clone(),
                        position: toast.position,
                        elapsed: toast.elapsed,
                        auto_close: toast.auto_close,
                        generation,
                    },
                );
            }
        }
        self.ensure_timer(window, cx);
    }
}
struct ManagedToast;
fn notification(
    toast: &Toast,
    host: gpui_kit::WeakEntity<OverlayHost>,
    generation: u64,
) -> Notification {
    let id = toast.id;
    let message: gpui_kit::SharedString = toast.message.clone().into();
    let closed = host.clone();
    Notification::new()
        .id1::<ManagedToast>(id.0)
        .autohide(false)
        .placement(anchor(toast.position))
        .on_close(move |window, cx| {
            let closed = closed.clone();
            window.defer(cx, move |window, cx| {
                let _ = closed.update(cx, |host, cx| {
                    if host
                        .cards
                        .get(&id)
                        .is_some_and(|card| card.generation == generation)
                    {
                        host.toasts.remove(id);
                        host.sync_cards(window, cx);
                    }
                });
            });
        })
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
            let hover = host.clone();
            div()
                .id(("guiraffe-toast", id.0))
                .test_support()
                .w_full()
                .on_hover(move |paused, _, cx| {
                    let _ = hover.update(cx, |host, _| host.toasts.pause(id, *paused));
                })
                .child(message.clone())
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
impl Render for OverlayHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
/// Initialize Kit, theme and the window-local overlay plugin before opening windows.
struct Initialized;
impl gpui_kit::Global for Initialized {}
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    let theme = cx.try_global::<GuiraffeTheme>().cloned();
    gpui_kit::init(cx);
    if let Some(theme) = theme {
        guiraffe_core::apply(theme, cx);
    } else {
        guiraffe_core::init(cx);
    }
    Root::register_plugin::<OverlayHost>(cx, OverlayHost::new);
    cx.bind_keys([gpui_kit::KeyBinding::new(
        "escape",
        IgnoreEscape,
        Some("GrModalEscapeDisabled > Sheet"),
    )]);
    crate::spotlight::init(cx);
    cx.set_global(Initialized);
}
/// Build this Root inside `WindowManager::open` to keep its typed window handle.
pub fn root(view: impl Into<gpui_kit::AnyView>, window: &mut Window, cx: &mut App) -> Entity<Root> {
    match view.into().downcast::<Root>() {
        Ok(root) => root,
        Err(view) => {
            let cached = cx.new(|_| CachedContent { view });
            cx.new(|cx| Root::new(cached, window, cx))
        }
    }
}

struct CachedContent {
    view: gpui_kit::AnyView,
}
impl Render for CachedContent {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view
            .clone()
            .cached(gpui_kit::StyleRefinement::default().size_full())
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
    pub fn new() -> Self {
        Self::default()
    }
    pub fn position(mut self, side: crate::position::Side) -> Self {
        self.side = side;
        self
    }
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
    pub fn close_on_escape(mut self, enabled: bool) -> Self {
        self.policy.escape = enabled;
        self
    }
    pub fn close_on_click_outside(mut self, enabled: bool) -> Self {
        self.policy.outside = enabled;
        self
    }

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
            return Err(OverlayError::DrawerRequiresEmptyStack);
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
