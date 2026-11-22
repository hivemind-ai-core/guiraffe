//! Window-local Controls bridge. Register before creating a Root.
use crate::modal::{ManagedModal, Modal as Dialog};
use crate::{
    ConfirmResult,
    confirm::{self, ConfirmSender},
    stack::{Dismiss, OverlayId, OverlayKind, OverlayStack, Policy},
    toast::{Toast, ToastId, ToastPosition, ToastQueue},
};
use gpui_kit::StatefulInteractiveElement;
use gpui_kit::base::{Root, RootPlugin, TestSupportExt};
use gpui_kit::component::{WindowExt, notification::Notification, progress::Progress};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Task, Window, div, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

struct ToastCard {
    entity: Entity<Notification>,
    message: String,
    position: ToastPosition,
    elapsed: Duration,
    auto_close: Option<Duration>,
    generation: u64,
}
gpui_kit::actions!(guiraffe_overlays, [IgnoreEscape]);
/// Prefer WeakEntity captures. Window-close cleanup releases even accidentally strong Root captures.
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
    dialogs: HashMap<OverlayId, Entity<ManagedModal>>,
    dialog_order: Vec<OverlayId>,
    native_restore_pending: bool,
    native_restore_previous: Option<gpui_kit::WeakFocusHandle>,
    root_scope: Option<gpui_kit::FocusHandle>,
    root_focus: Option<gpui_kit::FocusHandle>,
    focus_chain: HashMap<OverlayId, (gpui_kit::FocusHandle, Option<gpui_kit::WeakFocusHandle>)>,
    contexts: HashMap<String, ContextBuilder>,
    toasts: ToastQueue,
    cards: HashMap<ToastId, ToastCard>,
    timer: Option<Task<()>>,
    next_generation: u64,
    next_document_toast: u64,
    requested_toast_limit: Option<usize>,
    owned_notes: HashSet<gpui_kit::EntityId>,
    native_snapshot: Vec<gpui_kit::EntityId>,
    native_cap: usize,
    reconcile_pending: bool,
    _window_closed: Option<gpui_kit::Subscription>,
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
            .when_some(self.root_scope.as_ref(), |root, focus| {
                let focus = focus.clone();
                // Register the scope for containment checks without taking
                // pointer focus from the application content.
                root.on_children_prepainted(move |_, window, cx| {
                    window.set_focus_handle(&focus, cx);
                })
            })
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
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let id = window.window_handle().window_id();
        let host = cx.weak_entity();
        let subscription = cx.on_window_closed(move |cx, closed| {
            if id == closed {
                let _ = host.update(cx, |host, _| {
                    host.contexts.clear();
                    host.dialogs.clear();
                    host.dialog_order.clear();
                    host.focus_chain.clear();
                    host.confirms.clear();
                    host.cards.clear();
                    host.toasts.clear();
                    host.timer = None;
                });
            }
        });
        Self {
            root_scope: Some(cx.focus_handle()),
            root_focus: Some(cx.focus_handle()),
            _window_closed: Some(subscription),
            ..Self::default()
        }
    }
    pub fn entity(window: &Window, cx: &App) -> Result<Entity<Self>, OverlayError> {
        window
            .root::<Root>()
            .flatten()
            .and_then(|r| r.read(cx).plugin::<Self>())
            .ok_or(OverlayError::MissingRoot)
    }
    pub(crate) fn refresh_document_modal(window: &Window, cx: &mut App, id: OverlayId) {
        if let Ok(host) = Self::entity(window, cx) {
            let dialog = host.read(cx).dialogs.get(&id).cloned();
            if let Some(dialog) = dialog {
                dialog.update(cx, |_, cx| cx.notify());
            }
        }
    }

    pub(crate) fn allocate_document_toast(
        window: &Window,
        cx: &mut App,
    ) -> Result<ToastId, OverlayError> {
        let host = Self::entity(window, cx)?;
        Ok(host.update(cx, |host, _| {
            loop {
                let id = ToastId(u64::MAX - host.next_document_toast);
                host.next_document_toast = host.next_document_toast.wrapping_add(1);
                if !host
                    .toasts
                    .visible()
                    .iter()
                    .chain(host.toasts.queued())
                    .any(|toast| toast.id == id)
                {
                    break id;
                }
            }
        }))
    }

    pub fn stack(&self) -> &OverlayStack {
        &self.stack
    }
    pub fn toasts(&self) -> &ToastQueue {
        &self.toasts
    }
    fn settled(window: &mut Window, cx: &mut App, id: OverlayId, result: bool) {
        if let Ok(host) = Self::entity(window, cx) {
            Self::settle_entity(&host, window, cx, id, result);
        }
    }
    fn native_blocks_managed(&self, window: &mut Window, cx: &mut App) -> bool {
        window.has_active_dialog(cx)
            && !self
                .stack
                .entries()
                .iter()
                .filter(|entry| entry.kind != OverlayKind::Drawer)
                .any(|entry| {
                    self.focus_chain.get(&entry.id).is_some_and(|(focus, _)| {
                        window.focused(cx).as_ref() == Some(focus)
                            || focus.contains_focused(window, cx)
                    })
                })
    }
    pub(crate) fn settle_entity(
        entity: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
        id: OverlayId,
        result: bool,
    ) {
        entity.update(cx, |host, cx| {
            let native_hidden = host.native_blocks_managed(window, cx);
            if !host.stack.dismiss(id, Dismiss::Programmatic) {
                return;
            }
            if let Some(sender) = host.confirms.remove(&id) {
                sender.resolve(result);
            }
            if let Some((focus, previous)) = host.focus_chain.remove(&id) {
                let owns_focus = focus.contains_focused(window, cx)
                    || window.focused(cx).as_ref() == Some(&focus);
                for (_, prior) in host.focus_chain.values_mut() {
                    if prior
                        .as_ref()
                        .and_then(|p| p.upgrade())
                        .is_some_and(|prior| prior == focus || focus.contains(&prior, window))
                    {
                        *prior = previous.clone();
                    }
                }
                if host
                    .native_restore_previous
                    .as_ref()
                    .and_then(|previous| previous.upgrade())
                    .is_some_and(|prior| prior == focus || focus.contains(&prior, window))
                {
                    host.native_restore_previous = previous.clone();
                }
                if native_hidden {
                    host.native_restore_pending = true;
                    if host.native_restore_previous.is_none() {
                        host.native_restore_previous = previous.clone();
                    }
                }
                if owns_focus {
                    let restore = host.restore_target(previous, window);
                    if let Some(restore) = restore {
                        window.focus(&restore, cx);
                    }
                }
            }
            if native_hidden {
                host.release_modal(id, cx);
            } else if let Some(dialog) = host.dialogs.get(&id) {
                dialog.update(cx, |dialog, cx| dialog.close(cx));
            }
            cx.notify();
        });
    }
    fn restore_target(
        &self,
        previous: Option<gpui_kit::WeakFocusHandle>,
        window: &Window,
    ) -> Option<gpui_kit::FocusHandle> {
        let outer = self
            .stack
            .top()
            .and_then(|entry| self.focus_chain.get(&entry.id))
            .map(|(focus, _)| focus);
        let scope = outer.or(self.root_scope.as_ref())?;
        previous
            .and_then(|focus| focus.upgrade())
            .filter(|focus| focus == scope || scope.contains(focus, window))
            .or_else(|| outer.or(self.root_focus.as_ref()).cloned())
    }

    pub(crate) fn release_modal(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        if self.dialogs.remove(&id).is_some() {
            self.dialog_order.retain(|entry| *entry != id);
            cx.notify();
        }
    }
    /// Remove a managed layer by ID, without touching unrelated native dialogs.
    pub fn close(window: &mut Window, cx: &mut App, id: OverlayId) -> Result<(), OverlayError> {
        let host = Self::entity(window, cx)?;
        let kind = host
            .read(cx)
            .stack
            .entries()
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.kind)
            .ok_or(OverlayError::NotTopmost)?;
        let focus = window.focused(cx);
        if kind == OverlayKind::Drawer {
            window.close_sheet(cx);
        }
        Self::settle_entity(&host, window, cx, id, false);
        if kind == OverlayKind::Drawer
            && !host.read(cx).stack.entries().is_empty()
            && let Some(focus) = focus
        {
            window.focus(&focus, cx);
        }
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
        Self::open_layer(
            window,
            cx,
            policy,
            kind,
            sender,
            guiraffe_motion::TransitionKind::Fade,
            build,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_layer(
        window: &mut Window,
        cx: &mut App,
        policy: Policy,
        kind: OverlayKind,
        sender: Option<ConfirmSender>,
        transition: guiraffe_motion::TransitionKind,
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
        let previous = window.focused(cx).map(|focus| focus.downgrade());
        let builder = Rc::new(move |dialog: Dialog, window: &mut Window, cx: &mut App| {
            build(dialog, window, cx)
                .keyboard(policy.escape)
                .overlay_closable(policy.outside)
        });
        let weak = host.downgrade();
        let dialog = cx.new(|cx| ManagedModal::new(id, weak, builder, transition, window, cx));
        let focus = dialog.read(cx).focus.clone();
        host.update(cx, |host, cx| {
            host.focus_chain.insert(id, (focus, previous));
            host.dialogs.insert(id, dialog);
            host.dialog_order.push(id);
            cx.notify();
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
        let identity = Rc::new(std::cell::Cell::new(None));
        let callback_id = identity.clone();
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
                        .on_ok({
                            let callback_id = callback_id.clone();
                            move |_, window, cx| {
                                if let Ok(host) = Self::entity(window, cx) {
                                    host.update(cx, |host, _| {
                                        if let Some(id) = callback_id.get()
                                            && let Some(sender) = host.confirms.get(&id)
                                        {
                                            sender.resolve(true);
                                        }
                                    });
                                }
                                true
                            }
                        })
                },
            )?;
        identity.set(Some(id));
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
    /// Remove a named builder. Already open overlays keep their own lifetime.
    pub fn remove_context(
        window: &mut Window,
        cx: &mut App,
        name: &str,
    ) -> Result<bool, OverlayError> {
        Ok(Self::entity(window, cx)?.update(cx, |host, _| host.contexts.remove(name).is_some()))
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
            host.requested_toast_limit = Some(limit);
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
            cx.notify();
        });
        Ok(())
    }
    fn ensure_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .toasts
            .visible()
            .iter()
            .any(|t| t.auto_close.is_some() && !t.paused)
        {
            self.timer = None;
            return;
        }
        if self.timer.is_some() {
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
                    host.toasts
                        .visible()
                        .iter()
                        .any(|t| t.auto_close.is_some() && !t.paused)
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
    fn finish_admission_failures(
        &mut self,
        failed: Vec<ToastId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if failed.is_empty() {
            return;
        }
        for id in failed {
            self.toasts.remove(id);
        }
        if !self.reconcile_pending {
            self.reconcile_pending = true;
            cx.defer_in(window, |host, window, cx| {
                host.reconcile_pending = false;
                host.sync_cards(window, cx);
            });
        }
    }
    fn sync_cards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let notes = window.notifications(cx);
        self.owned_notes
            .retain(|id| notes.iter().any(|n| n.entity_id() == *id));
        let unmanaged = notes
            .iter()
            .filter(|n| !self.owned_notes.contains(&n.entity_id()))
            .count();
        let cap = gpui_kit::component::theme::Theme::global(cx)
            .notification
            .max_items;
        self.toasts.set_limit(
            self.requested_toast_limit
                .unwrap_or(5)
                .min(cap.saturating_sub(unmanaged)),
        );
        self.cards.retain(|id, _| {
            if self.toasts.visible().iter().any(|t| t.id == *id) {
                true
            } else {
                window.remove_notification1::<ManagedToast>(id.0, cx);
                false
            }
        });
        let mut failed = Vec::new();
        for toast in self.toasts.visible() {
            if let Some(card) = self.cards.get_mut(&toast.id) {
                let content_changed =
                    card.message != toast.message || card.position != toast.position;
                let progress_changed =
                    card.elapsed != toast.elapsed || card.auto_close != toast.auto_close;
                if content_changed || progress_changed {
                    let host = cx.weak_entity();
                    let generation = card.generation;
                    card.entity.update(cx, |note, cx| {
                        if content_changed {
                            *note = notification(toast, host, generation);
                        }
                        cx.notify();
                    });
                    card.message = toast.message.clone();
                    card.position = toast.position;
                    card.elapsed = toast.elapsed;
                    card.auto_close = toast.auto_close;
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
                self.owned_notes.insert(entity.entity_id());
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
            } else {
                eprintln!(
                    "GUIraffe: native toast admission failed for {:?}; dropping it",
                    toast.id
                );
                failed.push(toast.id);
            }
        }
        self.finish_admission_failures(failed, window, cx);
        self.native_snapshot = window
            .notifications(cx)
            .iter()
            .map(|n| n.entity_id())
            .collect();
        self.native_cap = cap;
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
        .delivery(gpui_kit::component::notification::NotificationDelivery::InApp)
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
                .on_hover(move |paused, window, cx| {
                    let _ = hover.update(cx, |host, cx| {
                        host.toasts.pause(id, *paused);
                        host.ensure_timer(window, cx);
                    });
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let notes = window.notifications(cx);
        let changed = notes.len() != self.native_snapshot.len()
            || notes
                .iter()
                .zip(&self.native_snapshot)
                .any(|(n, id)| n.entity_id() != *id)
            || self.native_cap
                != gpui_kit::component::theme::Theme::global(cx)
                    .notification
                    .max_items;
        if changed && !self.reconcile_pending {
            self.reconcile_pending = true;
            cx.defer_in(window, |host, window, cx| {
                host.reconcile_pending = false;
                host.sync_cards(window, cx);
            });
        }
        let native = self.native_blocks_managed(window, cx);
        if self.native_restore_pending
            && (!window.has_active_dialog(cx) || window.focused(cx).is_none())
        {
            self.native_restore_pending = false;
            let previous = self.native_restore_previous.take();
            let restore = self.restore_target(previous, window);
            if let Some(focus) = restore {
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }
        }
        div()
            .absolute()
            .inset_0()
            // The fallback is inside Kit's Root so its Tab actions still apply.
            .children(
                self.root_focus
                    .as_ref()
                    .map(|focus| div().absolute().size_0().track_focus(focus)),
            )
            .children(self.dialog_order.iter().filter_map(|id| {
                if native && self.stack.z_index(*id).is_some() {
                    None
                } else {
                    self.dialogs.get(id).cloned()
                }
            }))
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
    guiraffe_motion::init(cx);
    crate::spotlight::init(cx);
    crate::data_ui::register(cx);
    crate::document_services::register(cx);
    crate::kit_surfaces::register(cx);
    crate::spotlight::document::register(cx);
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
        self.open_with_close(window, cx, build, |_, _| {})
    }

    pub(crate) fn open_with_close(
        self,
        window: &mut Window,
        cx: &mut App,
        build: impl Fn(
            gpui_kit::component::sheet::Sheet,
            &mut Window,
            &mut App,
        ) -> gpui_kit::component::sheet::Sheet
        + 'static,
        close: impl Fn(&mut Window, &mut App) + 'static,
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
        let previous = window.focused(cx).map(|focus| focus.downgrade());
        let close = std::rc::Rc::new(close);
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
            .on_close({
                let close = std::rc::Rc::clone(&close);
                move |_, window, cx| {
                    OverlayHost::settled(window, cx, id, false);
                    close(window, cx);
                }
            })
        });
        if let Some(focus) = window.focused(cx) {
            host.update(cx, |host, _| {
                host.focus_chain.insert(id, (focus, previous));
            });
        }
        Ok(id)
    }
}
#[cfg(test)]
mod review_tests {
    use super::*;
    use gpui_kit::TestAppContext;
    use std::cell::Cell;
    #[gpui_kit::test]
    fn review_6_paused_toasts_stop_timer(cx: &mut TestAppContext) {
        cx.update(init);
        let handle = crate::gpui_tests::open(cx);
        cx.update_window(handle.into(), |_, window, cx| {
            OverlayHost::notify(window, cx, Toast::new(ToastId(1), "timer")).expect("toast");
            OverlayHost::entity(window, cx)
                .expect("host")
                .update(cx, |host, cx| {
                    host.toasts.pause(ToastId(1), true);
                    host.ensure_timer(window, cx);
                    assert!(
                        host.timer.is_none(),
                        "all-paused work must have no wakeup task"
                    );
                });
        })
        .expect("window");
    }
    #[gpui_kit::test]
    fn review_7_clean_notifies_observers(cx: &mut TestAppContext) {
        cx.update(init);
        let handle = crate::gpui_tests::open(cx);
        let changes = Rc::new(Cell::new(0));
        let observed = changes.clone();
        let subscription = cx
            .update_window(handle.into(), |_, window, cx| {
                OverlayHost::notify(window, cx, Toast::new(ToastId(1), "one")).expect("toast");
                let host = OverlayHost::entity(window, cx).expect("host");
                cx.observe(&host, move |_, _| observed.set(observed.get() + 1))
            })
            .expect("observe");
        cx.update_window(handle.into(), |_, window, cx| {
            OverlayHost::clear_toasts(window, cx).expect("clear")
        })
        .expect("window");
        assert!(
            changes.get() > 0,
            "clean must notify interested overlay observers"
        );
        drop(subscription);
    }
    #[gpui_kit::test]
    fn removed_contexts_release_builders_under_churn(cx: &mut TestAppContext) {
        cx.update(init);
        let handle = crate::gpui_tests::open(cx);
        cx.update_window(handle.into(), |_, window, cx| {
            let builder: ContextBuilder = Rc::new(|dialog, _, _| dialog);
            OverlayHost::register_context(window, cx, "keep", builder.clone())
                .expect("register context");
            for id in 0..1024 {
                let name = format!("temporary-{id}");
                OverlayHost::register_context(window, cx, &name, builder.clone())
                    .expect("register context");
                assert!(OverlayHost::remove_context(window, cx, &name).expect("remove context"));
            }
            let host = OverlayHost::entity(window, cx).expect("overlay host");
            assert_eq!(host.read(cx).contexts.len(), 1);
            assert_eq!(Rc::strong_count(&builder), 2);
        })
        .expect("update test window");
    }

    #[gpui_kit::test]
    fn window_close_releases_toasts_even_when_host_handle_survives(cx: &mut TestAppContext) {
        cx.update(init);
        let handle = crate::gpui_tests::open(cx);
        let host = cx
            .update_window(handle.into(), |_, window, cx| {
                let host = OverlayHost::entity(window, cx).expect("host");
                host.update(cx, |host, _| {
                    host.toasts.set_limit(0);
                    for id in 0..1_000 {
                        host.toasts
                            .push(Toast::new(ToastId(id), "queued payload".repeat(128)));
                    }
                    assert_eq!(host.toasts.queued().len(), 1_000);
                });
                window.remove_window();
                host
            })
            .expect("close");
        cx.run_until_parked();
        cx.update(|cx| {
            let host = host.read(cx);
            assert!(host.toasts.visible().is_empty());
            assert_eq!(host.toasts.queued().capacity(), 0);
        });
    }

    #[gpui_kit::test]
    fn review_12_window_close_breaks_context_root_cycle(cx: &mut TestAppContext) {
        cx.update(init);
        let handle = crate::gpui_tests::open(cx);
        let weak = cx
            .update_window(handle.into(), |_, window, cx| {
                let root = window.root::<Root>().flatten().expect("root");
                OverlayHost::register_context(
                    window,
                    cx,
                    "cycle",
                    Rc::new(move |dialog, _, _| {
                        let _ = &root;
                        dialog
                    }),
                )
                .expect("register");
                OverlayHost::entity(window, cx).expect("host").downgrade()
            })
            .expect("window");
        cx.update_window(handle.into(), |_, window, _| window.remove_window())
            .expect("close");
        cx.run_until_parked();
        assert!(
            weak.upgrade().is_none(),
            "window close must release a strongly captured Root"
        );
    }
}

#[cfg(test)]
mod admission_failure_tests {
    use super::*;
    #[gpui_kit::test]
    fn review_1_failed_admission_promotes_persistent_successor(cx: &mut gpui_kit::TestAppContext) {
        cx.update(init);
        let handle = crate::gpui_tests::open(cx);
        cx.update_window(handle.into(), |_, window, cx| {
            OverlayHost::entity(window, cx)
                .expect("host")
                .update(cx, |host, cx| {
                    host.toasts = ToastQueue::new(1);
                    host.requested_toast_limit = Some(1);
                    for id in [1, 2] {
                        let mut note = Toast::new(ToastId(id), "persistent");
                        note.auto_close = None;
                        host.toasts.push(note);
                    }
                    host.finish_admission_failures(vec![ToastId(1)], window, cx);
                });
        })
        .expect("failed admission");
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| {
            let host = OverlayHost::entity(window, cx).expect("host");
            assert!(
                host.read(cx).cards.contains_key(&ToastId(2)),
                "persistent successors require reconciliation without an active timer"
            );
        })
        .expect("successor");
    }
}
