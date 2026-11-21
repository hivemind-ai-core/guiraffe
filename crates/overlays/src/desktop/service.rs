//! The system-notification service: ids, replace-in-place, close, and typed events.
use std::{collections::HashMap, rc::Rc, sync::Arc};

use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, Global, Subscription, Task};

use super::{
    CloseReason, DesktopNotification, NotificationId, PlatformBackend,
    backend::{Capabilities, Delivery, DesktopError, NotificationBackend, Response},
};

/// What happened to a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopEventKind {
    /// The OS accepted it (also after an [`update`](SystemNotifications::update)).
    Shown,
    /// The user clicked the notification body.
    Activated,
    /// The user pressed the action button with this key.
    Action(String),
    /// It closed without an action. Terminal.
    Closed(CloseReason),
    /// Delivery failed. Terminal.
    Failed(DesktopError),
}

impl DesktopEventKind {
    /// Whether the notification is gone after this event.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Shown)
    }
}

/// A typed event for one notification, emitted by [`NotificationCenter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEvent {
    pub id: NotificationId,
    pub kind: DesktopEventKind,
}

type Handler = Rc<dyn Fn(&DesktopEvent, &mut App)>;

struct Live {
    /// Bumped by every update, so a stale delivery task can't report on the new content.
    generation: u64,
    platform_id: Option<u32>,
    handler: Option<Handler>,
    _task: Task<()>,
}

/// The service state. Subscribe to it for [`DesktopEvent`]s (see
/// [`SystemNotifications::subscribe`]).
pub struct NotificationCenter {
    backend: Arc<dyn NotificationBackend>,
    next_id: u64,
    generation: u64,
    live: HashMap<NotificationId, Live>,
}

impl EventEmitter<DesktopEvent> for NotificationCenter {}

impl NotificationCenter {
    /// Ids of notifications that are open (or being delivered).
    pub fn open_ids(&self) -> Vec<NotificationId> {
        let mut ids: Vec<_> = self.live.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
    pub fn is_open(&self, id: NotificationId) -> bool {
        self.live.contains_key(&id)
    }
    pub fn capabilities(&self) -> Capabilities {
        self.backend.capabilities()
    }
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    fn deliver(
        &mut self,
        id: NotificationId,
        note: DesktopNotification,
        handler: Option<Handler>,
        cx: &mut Context<Self>,
    ) {
        self.generation += 1;
        let generation = self.generation;
        let previous = self.live.remove(&id);
        let replaces = previous
            .as_ref()
            .and_then(|p| p.platform_id)
            .filter(|_| self.backend.capabilities().replace);
        // An update keeps the original handler unless a new one is given.
        let handler = handler.or_else(|| previous.and_then(|p| p.handler));
        let backend = self.backend.clone();
        let task = cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let shown = executor
                .spawn(backend.show(Delivery {
                    notification: note,
                    replaces,
                }))
                .await;
            let shown = match shown {
                Ok(shown) => shown,
                Err(error) => {
                    let _ = this.update(cx, |center, cx| {
                        center.finish(id, generation, DesktopEventKind::Failed(error), true, cx)
                    });
                    return;
                }
            };
            let platform_id = shown.platform_id;
            let current = this
                .update(cx, |center, cx| {
                    let Some(live) = center.live.get_mut(&id) else {
                        return false;
                    };
                    if live.generation != generation {
                        return false;
                    }
                    live.platform_id = Some(platform_id);
                    center.report(id, DesktopEventKind::Shown, cx);
                    true
                })
                .unwrap_or(false);
            if !current {
                return;
            }
            let outcome = executor.spawn(shown.outcome).await;
            let kind = match outcome {
                Some(Response::Activated) => DesktopEventKind::Activated,
                Some(Response::Action(key)) => DesktopEventKind::Action(key),
                Some(Response::Closed(reason)) => DesktopEventKind::Closed(reason),
                // The platform reports nothing more: stop tracking it quietly.
                None => {
                    let _ = this.update(cx, |center, _| {
                        if center
                            .live
                            .get(&id)
                            .is_some_and(|l| l.generation == generation)
                            && let Some(live) = center.live.remove(&id)
                        {
                            // We are inside this task: let it finish instead of dropping it.
                            live._task.detach();
                        }
                    });
                    return;
                }
            };
            let _ = this.update(cx, |center, cx| {
                center.finish(id, generation, kind, true, cx)
            });
        });
        self.live.insert(
            id,
            Live {
                generation,
                platform_id: None,
                handler,
                _task: task,
            },
        );
    }

    fn finish(
        &mut self,
        id: NotificationId,
        generation: u64,
        kind: DesktopEventKind,
        from_task: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .live
            .get(&id)
            .is_none_or(|l| l.generation != generation)
        {
            return;
        }
        // Report before forgetting, so the handler still runs.
        self.report(id, kind, cx);
        if let Some(live) = self.live.remove(&id) {
            // Inside the delivery task (its last step) we must not drop ourselves: detach.
            // From `close`, dropping cancels the task and stops listening.
            if from_task {
                live._task.detach();
            }
        }
    }

    fn report(&mut self, id: NotificationId, kind: DesktopEventKind, cx: &mut Context<Self>) {
        let event = DesktopEvent { id, kind };
        if let Some(handler) = self.live.get(&id).and_then(|l| l.handler.clone()) {
            // Handlers may call back into the service: run them after this update.
            let event = event.clone();
            cx.defer(move |cx| handler(&event, cx));
        }
        cx.emit(event);
    }
}

#[derive(Clone)]
struct Installed(Entity<NotificationCenter>);
impl Global for Installed {}

/// Handle to the app's system-notification service. Cheap to clone.
#[derive(Clone)]
pub struct SystemNotifications {
    center: Entity<NotificationCenter>,
}

/// Install the platform backend (idempotent: keeps an installed backend).
pub fn init(cx: &mut App) {
    if !cx.has_global::<Installed>() {
        init_with_backend(Arc::new(PlatformBackend::new()), cx);
    }
}

/// Install `backend`, replacing any previous one. Notifications of the previous backend stop
/// being tracked (no further events).
pub fn init_with_backend(backend: Arc<dyn NotificationBackend>, cx: &mut App) {
    let center = cx.new(|_| NotificationCenter {
        backend,
        next_id: 0,
        generation: 0,
        live: HashMap::new(),
    });
    cx.set_global(Installed(center));
}

impl SystemNotifications {
    /// The installed service, or [`DesktopError::NotInitialized`].
    pub fn global(cx: &App) -> Result<Self, DesktopError> {
        cx.try_global::<Installed>()
            .map(|i| Self {
                center: i.0.clone(),
            })
            .ok_or(DesktopError::NotInitialized)
    }

    /// The state entity; subscribe to it or read open ids.
    pub fn entity(&self) -> &Entity<NotificationCenter> {
        &self.center
    }

    pub fn capabilities(&self, cx: &App) -> Capabilities {
        self.center.read(cx).capabilities()
    }

    /// Show `note`. Results arrive as [`DesktopEvent`]s.
    pub fn show(&self, note: DesktopNotification, cx: &mut App) -> NotificationId {
        self.show_inner(note, None, cx)
    }

    /// Show `note` and call `handler` with each of its events (after subscribers are notified).
    pub fn show_with(
        &self,
        note: DesktopNotification,
        handler: impl Fn(&DesktopEvent, &mut App) + 'static,
        cx: &mut App,
    ) -> NotificationId {
        self.show_inner(note, Some(Rc::new(handler)), cx)
    }

    fn show_inner(
        &self,
        note: DesktopNotification,
        handler: Option<Handler>,
        cx: &mut App,
    ) -> NotificationId {
        self.center.update(cx, |center, cx| {
            center.next_id += 1;
            let id = NotificationId(center.next_id);
            center.deliver(id, note, handler, cx);
            id
        })
    }

    /// Replace the content of `id` in place (where the platform supports it), keeping its id and
    /// handler. Updating a closed notification shows it again under the same id.
    pub fn update(&self, id: NotificationId, note: DesktopNotification, cx: &mut App) {
        self.center.update(cx, |center, cx| {
            center.next_id = center.next_id.max(id.0);
            center.deliver(id, note, None, cx)
        });
    }

    /// Close `id`. Emits [`DesktopEventKind::Closed`] with [`CloseReason::App`]. Returns
    /// [`DesktopError::Unknown`] if it is not open.
    pub fn close(&self, id: NotificationId, cx: &mut App) -> Result<(), DesktopError> {
        self.center.update(cx, |center, cx| {
            let generation = center
                .live
                .get(&id)
                .map(|l| l.generation)
                .ok_or(DesktopError::Unknown(id.0))?;
            let platform_id = center.live.get(&id).and_then(|l| l.platform_id);
            center.finish(
                id,
                generation,
                DesktopEventKind::Closed(CloseReason::App),
                false,
                cx,
            );
            if let Some(platform_id) = platform_id {
                let close = center.backend.close(platform_id);
                // Already reported as closed; a late failure (e.g. the server already expired
                // it) is not an error for the app.
                cx.background_executor()
                    .spawn(async move {
                        let _ = close.await;
                    })
                    .detach();
            }
            Ok(())
        })
    }

    /// Subscribe to every notification's events.
    pub fn subscribe(
        &self,
        cx: &mut App,
        f: impl Fn(&DesktopEvent, &mut App) + 'static,
    ) -> Subscription {
        cx.subscribe(&self.center, move |_, event, cx| f(event, cx))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    use gpui_kit::TestAppContext;

    use super::*;
    use crate::desktop::{Timeout, mock::MockBackend};

    fn setup(
        cx: &mut TestAppContext,
        mock: &MockBackend,
    ) -> (
        SystemNotifications,
        Rc<RefCell<Vec<DesktopEvent>>>,
        Subscription,
    ) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let log = events.clone();
        let (service, sub) = cx.update(|cx| {
            init_with_backend(Arc::new(mock.clone()), cx);
            let service = SystemNotifications::global(cx).expect("installed");
            let sub = service.subscribe(cx, move |e, _| log.borrow_mut().push(e.clone()));
            (service, sub)
        });
        (service, events, sub)
    }

    #[gpui_kit::test]
    fn not_initialized_is_an_error(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert_eq!(
                SystemNotifications::global(cx).err(),
                Some(DesktopError::NotInitialized)
            );
        });
    }

    #[gpui_kit::test]
    fn action_click_is_reported_as_a_typed_event(cx: &mut TestAppContext) {
        let mock = MockBackend::new();
        let (service, events, _sub) = setup(cx, &mock);
        let id = cx.update(|cx| {
            service.show(
                DesktopNotification::new("Build", "done").action("open", "Open"),
                cx,
            )
        });
        cx.run_until_parked();
        assert_eq!(mock.deliveries().len(), 1);
        assert_eq!(mock.deliveries()[0].notification.actions[0].key, "open");
        assert_eq!(
            *events.borrow(),
            [DesktopEvent {
                id,
                kind: DesktopEventKind::Shown
            }]
        );
        assert!(mock.respond(1, Response::Action("open".into())));
        cx.run_until_parked();
        assert_eq!(
            events.borrow().last(),
            Some(&DesktopEvent {
                id,
                kind: DesktopEventKind::Action("open".into())
            })
        );
        cx.update(|cx| assert!(!service.entity().read(cx).is_open(id)));
    }

    #[gpui_kit::test]
    fn body_click_and_close_reasons(cx: &mut TestAppContext) {
        let mock = MockBackend::new();
        let (service, events, _sub) = setup(cx, &mock);
        let (a, b) = cx.update(|cx| {
            (
                service.show(DesktopNotification::new("a", ""), cx),
                service.show(DesktopNotification::new("b", ""), cx),
            )
        });
        cx.run_until_parked();
        mock.respond(1, Response::Activated);
        cx.run_until_parked();
        mock.respond(2, Response::Closed(CloseReason::Expired));
        cx.run_until_parked();
        let terminal: Vec<_> = events
            .borrow()
            .iter()
            .filter(|e| e.kind.is_terminal())
            .cloned()
            .collect();
        assert_eq!(
            terminal,
            [
                DesktopEvent {
                    id: a,
                    kind: DesktopEventKind::Activated
                },
                DesktopEvent {
                    id: b,
                    kind: DesktopEventKind::Closed(CloseReason::Expired)
                }
            ]
        );
    }

    #[gpui_kit::test]
    fn update_replaces_in_place_and_keeps_the_handler(cx: &mut TestAppContext) {
        let mock = MockBackend::new();
        let (service, events, _sub) = setup(cx, &mock);
        let handled = Rc::new(RefCell::new(Vec::new()));
        let log = handled.clone();
        let id = cx.update(|cx| {
            service.show_with(
                DesktopNotification::new("Downloading", "10%"),
                move |e, _| log.borrow_mut().push(e.kind.clone()),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| service.update(id, DesktopNotification::new("Downloading", "90%"), cx));
        cx.run_until_parked();
        let deliveries = mock.deliveries();
        assert_eq!(deliveries.len(), 2);
        assert_eq!(
            deliveries[1].replaces,
            Some(1),
            "update targets the shown platform id"
        );
        assert_eq!(deliveries[1].notification.body, "90%");
        assert_eq!(
            mock.open(),
            [1],
            "replaced in place, not a second notification"
        );
        mock.respond(1, Response::Closed(CloseReason::Dismissed));
        cx.run_until_parked();
        assert_eq!(
            *handled.borrow(),
            [
                DesktopEventKind::Shown,
                DesktopEventKind::Shown,
                DesktopEventKind::Closed(CloseReason::Dismissed)
            ]
        );
        assert_eq!(events.borrow().len(), 3);
    }

    #[gpui_kit::test]
    fn update_without_replace_support_shows_a_new_one(cx: &mut TestAppContext) {
        let mock = MockBackend::with_capabilities(Capabilities {
            replace: false,
            ..Capabilities::ALL
        });
        let (service, _events, _sub) = setup(cx, &mock);
        let id = cx.update(|cx| service.show(DesktopNotification::new("a", ""), cx));
        cx.run_until_parked();
        cx.update(|cx| service.update(id, DesktopNotification::new("b", ""), cx));
        cx.run_until_parked();
        assert_eq!(mock.deliveries()[1].replaces, None);
    }

    #[gpui_kit::test]
    fn stale_outcome_after_update_is_ignored(cx: &mut TestAppContext) {
        // A backend that hands out a new platform id per delivery: the first one's late
        // response must not close the updated notification.
        let mock = MockBackend::with_capabilities(Capabilities {
            replace: false,
            ..Capabilities::ALL
        });
        let (service, events, _sub) = setup(cx, &mock);
        let id = cx.update(|cx| service.show(DesktopNotification::new("a", ""), cx));
        cx.run_until_parked();
        cx.update(|cx| service.update(id, DesktopNotification::new("b", ""), cx));
        cx.run_until_parked();
        assert!(
            !mock.respond(1, Response::Closed(CloseReason::Expired)),
            "listener dropped"
        );
        cx.run_until_parked();
        cx.update(|cx| assert!(service.entity().read(cx).is_open(id)));
        assert!(events.borrow().iter().all(|e| !e.kind.is_terminal()));
    }

    #[gpui_kit::test]
    fn app_close_reports_reason_and_calls_backend(cx: &mut TestAppContext) {
        let mock = MockBackend::new();
        let (service, events, _sub) = setup(cx, &mock);
        let id = cx.update(|cx| {
            service.show(
                DesktopNotification::new("a", "").timeout(Timeout::Never),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| service.close(id, cx)).expect("open");
        cx.run_until_parked();
        assert_eq!(mock.closed(), [1]);
        assert_eq!(
            events.borrow().last().map(|e| e.kind.clone()),
            Some(DesktopEventKind::Closed(CloseReason::App))
        );
        assert_eq!(
            cx.update(|cx| service.close(id, cx)),
            Err(DesktopError::Unknown(id.0))
        );
    }

    #[gpui_kit::test]
    fn delivery_errors_surface_as_events(cx: &mut TestAppContext) {
        let mock = MockBackend::new();
        let (service, events, _sub) = setup(cx, &mock);
        mock.fail_next(DesktopError::Unavailable("no daemon".into()));
        let id = cx.update(|cx| service.show(DesktopNotification::new("a", ""), cx));
        cx.run_until_parked();
        assert_eq!(
            *events.borrow(),
            [DesktopEvent {
                id,
                kind: DesktopEventKind::Failed(DesktopError::Unavailable("no daemon".into()))
            }]
        );
        cx.update(|cx| assert!(service.entity().read(cx).open_ids().is_empty()));
    }

    #[gpui_kit::test]
    fn detached_platforms_stop_tracking_quietly(cx: &mut TestAppContext) {
        let mock = MockBackend::with_capabilities(Capabilities::NONE);
        let (service, events, _sub) = setup(cx, &mock);
        let id = cx.update(|cx| service.show(DesktopNotification::new("a", ""), cx));
        cx.run_until_parked();
        mock.detach(1);
        cx.run_until_parked();
        cx.update(|cx| assert!(!service.entity().read(cx).is_open(id)));
        assert_eq!(events.borrow().len(), 1, "only Shown");
    }

    #[gpui_kit::test]
    fn handlers_may_reenter_the_service(cx: &mut TestAppContext) {
        let mock = MockBackend::new();
        let (service, _events, _sub) = setup(cx, &mock);
        let inner = service.clone();
        cx.update(|cx| {
            service.show_with(
                DesktopNotification::new("Retry?", "").action("retry", "Retry"),
                move |e, cx| {
                    if e.kind == DesktopEventKind::Action("retry".into()) {
                        inner.show(DesktopNotification::new("Retrying", ""), cx);
                    }
                },
                cx,
            )
        });
        cx.run_until_parked();
        mock.respond(1, Response::Action("retry".into()));
        cx.run_until_parked();
        assert_eq!(mock.deliveries().len(), 2);
        assert_eq!(mock.deliveries()[1].notification.title, "Retrying");
    }
}
