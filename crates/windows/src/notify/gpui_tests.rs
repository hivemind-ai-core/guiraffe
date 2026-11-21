//! Interaction tests for the notification windows (TestAppContext; the executor clock drives
//! both the countdowns and the animations).
use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::{Modifiers, TestAppContext, VisualTestContext, point, px};

use super::*;

fn setup(
    cx: &mut TestAppContext,
    config: NotificationWindowsConfig,
) -> (
    NotificationWindows,
    Rc<RefCell<Vec<WindowNotificationEvent>>>,
    gpui_kit::Subscription,
) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    let (service, sub) = cx.update(|cx| {
        gpui_kit::component::init(cx);
        let service = init(config, cx);
        let sub = service.subscribe(cx, move |e, _| sink.borrow_mut().push(e.clone()));
        (service, sub)
    });
    (service, log, sub)
}

fn kinds(
    log: &Rc<RefCell<Vec<WindowNotificationEvent>>>,
) -> Vec<(u64, WindowNotificationEventKind)> {
    log.borrow()
        .iter()
        .map(|e| (e.id.0, e.kind.clone()))
        .collect()
}

fn surface(service: &NotificationWindows, cx: &mut TestAppContext) -> Option<VisualTestContext> {
    let handle = cx.update(|cx| service.entity().read(cx).window())?;
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    // The test platform applies `Window::resize` without the configure a real platform sends.
    let bounds = vcx.update(|window, _| window.bounds());
    vcx.simulate_resize(bounds.size);
    // Start with the pointer outside (the test platform reports it at the origin).
    vcx.simulate_mouse_move(point(px(-50.), px(-50.)), None, Modifiers::none());
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    Some(vcx)
}

fn reduced(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

#[gpui_kit::test]
fn show_opens_a_surface_sized_to_the_stack(cx: &mut TestAppContext) {
    let (service, log, _sub) = setup(cx, Default::default());
    assert!(cx.update(|cx| service.entity().read(cx).window()).is_none());
    let a = cx.update(|cx| service.show(WindowNotification::new("Saved").body("All good"), cx));
    let b = cx.update(|cx| {
        service.show(
            WindowNotification::new("Download")
                .progress(0.4)
                .action("open", "Open"),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        kinds(&log),
        [
            (a.0, WindowNotificationEventKind::Shown),
            (b.0, WindowNotificationEventKind::Shown)
        ]
    );
    let layout = cx.update(|cx| service.entity().read(cx).layout());
    let mut vcx = surface(&service, cx).expect("surface window");
    let size = vcx.update(|window, _| window.viewport_size());
    assert_eq!(size.width, px(layout.width));
    assert_eq!(size.height, px(layout.height));
    // Newest first, in its slot, at its fixed height.
    let newest = vcx.debug_bounds("notification-2").expect("newest card");
    let oldest = vcx.debug_bounds("notification-1").expect("older card");
    assert!(newest.top() < oldest.top(), "top corner: newest on top");
    assert_eq!(newest.size.height, px(layout.slots[0].h));
    assert!(vcx.debug_bounds("notification-2-action-open").is_some());
    assert!(vcx.debug_bounds("notification-1-action-open").is_none());
    assert!(vcx.debug_bounds("notification-1-close").is_some());
}

#[gpui_kit::test]
fn auto_dismiss_after_timeout_then_surface_closes(cx: &mut TestAppContext) {
    let (service, log, _sub) = setup(cx, Default::default());
    let id = cx.update(|cx| {
        service.show(
            WindowNotification::new("Bye").timeout(Some(Duration::from_secs(3))),
            cx,
        )
    });
    cx.run_until_parked();
    let _surface = surface(&service, cx).expect("surface");
    cx.executor().advance_clock(Duration::from_millis(2900));
    cx.run_until_parked();
    assert_eq!(log.borrow().len(), 1, "not yet");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(
        kinds(&log).last(),
        Some(&(
            id.0,
            WindowNotificationEventKind::Dismissed(DismissReason::Timeout)
        ))
    );
    // The exit animation keeps the slot briefly, then the surface goes away.
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update(|cx| service.entity().read(cx).window()).is_none());
    assert!(cx.update(|cx| service.entity().read(cx).model().is_empty()));
    assert!(cx.windows().is_empty(), "no window left behind");
}

#[gpui_kit::test]
fn hover_pauses_the_countdown(cx: &mut TestAppContext) {
    reduced(cx);
    let (service, log, _sub) = setup(cx, Default::default());
    cx.update(|cx| {
        service.show(
            WindowNotification::new("Hover me").timeout(Some(Duration::from_secs(2))),
            cx,
        )
    });
    cx.run_until_parked();
    let mut vcx = surface(&service, cx).expect("surface");
    let card = vcx.debug_bounds("notification-1").expect("card");
    vcx.simulate_mouse_move(card.center(), None, Modifiers::none());
    vcx.run_until_parked();
    assert!(cx.update(|cx| service.entity().read(cx).model().is_paused()));
    cx.executor().advance_clock(Duration::from_secs(30));
    cx.run_until_parked();
    assert_eq!(log.borrow().len(), 1, "paused while hovered");
    vcx.simulate_mouse_move(point(px(-50.), px(-50.)), None, Modifiers::none());
    vcx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        kinds(&log).last().map(|k| k.1.clone()),
        Some(WindowNotificationEventKind::Dismissed(
            DismissReason::Timeout
        ))
    );
}

#[gpui_kit::test]
fn clicks_actions_and_close_emit_typed_events(cx: &mut TestAppContext) {
    reduced(cx);
    let (service, log, _sub) = setup(cx, Default::default());
    let handled = Rc::new(RefCell::new(Vec::new()));
    let h = handled.clone();
    let a = cx.update(|cx| {
        service.show_with(
            WindowNotification::new("Update ready").primary_action("restart", "Restart"),
            move |e, _| h.borrow_mut().push(e.kind.clone()),
            cx,
        )
    });
    let b = cx.update(|cx| service.show(WindowNotification::new("Click me").sticky(), cx));
    let c = cx.update(|cx| service.show(WindowNotification::new("Close me").sticky(), cx));
    cx.run_until_parked();
    let mut vcx = surface(&service, cx).expect("surface");
    let action = vcx
        .debug_bounds("notification-1-action-restart")
        .expect("action button");
    vcx.simulate_click(action.center(), Modifiers::none());
    vcx.run_until_parked();

    let mut vcx = surface(&service, cx).expect("surface");
    let body = vcx.debug_bounds("notification-2").expect("card");
    // Click the body away from the close button.
    vcx.simulate_click(
        point(body.left() + px(40.), body.center().y),
        Modifiers::none(),
    );
    vcx.run_until_parked();
    let mut vcx = surface(&service, cx).expect("surface");
    let close = vcx.debug_bounds("notification-3-close").expect("close");
    vcx.simulate_click(close.center(), Modifiers::none());
    vcx.run_until_parked();
    let terminal: Vec<_> = kinds(&log)
        .into_iter()
        .filter(|(_, k)| *k != WindowNotificationEventKind::Shown)
        .collect();
    assert_eq!(
        terminal,
        [
            (a.0, WindowNotificationEventKind::Action("restart".into())),
            (
                a.0,
                WindowNotificationEventKind::Dismissed(DismissReason::Action)
            ),
            (b.0, WindowNotificationEventKind::Clicked),
            (
                b.0,
                WindowNotificationEventKind::Dismissed(DismissReason::Clicked)
            ),
            (
                c.0,
                WindowNotificationEventKind::Dismissed(DismissReason::Closed)
            ),
        ],
        "a click on a button does not also click the card"
    );
    assert_eq!(
        *handled.borrow(),
        [
            WindowNotificationEventKind::Shown,
            WindowNotificationEventKind::Action("restart".into()),
            WindowNotificationEventKind::Dismissed(DismissReason::Action)
        ]
    );
    assert!(cx.update(|cx| service.entity().read(cx).window()).is_none());
}

#[gpui_kit::test]
fn update_in_place_and_queue_limit(cx: &mut TestAppContext) {
    reduced(cx);
    let config = NotificationWindowsConfig {
        stack: StackConfig {
            max_visible: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let (service, log, _sub) = setup(cx, config);
    let ids: Vec<_> = (0..3)
        .map(|i| {
            cx.update(|cx| service.show(WindowNotification::new(format!("n{i}")).sticky(), cx))
        })
        .collect();
    cx.run_until_parked();
    assert_eq!(log.borrow().len(), 2, "third is queued");
    cx.update(|cx| {
        service.update(
            ids[0],
            WindowNotification::new("n0 updated").sticky().progress(1.0),
            cx,
        )
    });
    let card = cx.update(|cx| service.entity().read(cx).model().get(ids[0]).cloned());
    assert_eq!(card.map(|c| c.content.title), Some("n0 updated".into()));
    assert!(cx.update(|cx| service.dismiss(ids[1], cx)));
    cx.run_until_parked();
    assert_eq!(
        kinds(&log)[2..],
        [
            (
                ids[1].0,
                WindowNotificationEventKind::Dismissed(DismissReason::App)
            ),
            (ids[2].0, WindowNotificationEventKind::Shown)
        ]
    );
    cx.update(|cx| service.clear(cx));
    cx.run_until_parked();
    assert!(cx.update(|cx| service.entity().read(cx).model().is_empty()));
    assert!(!cx.update(|cx| service.dismiss(ids[0], cx)));
}

#[gpui_kit::test]
fn bottom_corner_stacks_newest_at_the_bottom(cx: &mut TestAppContext) {
    reduced(cx);
    let config = NotificationWindowsConfig {
        stack: StackConfig {
            corner: Corner::BottomLeft,
            ..Default::default()
        },
        placement: Placement::Positioned,
        ..Default::default()
    };
    let (service, _log, _sub) = setup(cx, config);
    cx.update(|cx| service.show(WindowNotification::new("first").sticky(), cx));
    cx.run_until_parked();
    cx.update(|cx| service.show(WindowNotification::new("second").sticky(), cx));
    cx.run_until_parked();
    let mut vcx = surface(&service, cx).expect("surface");
    let newest = vcx.debug_bounds("notification-2").expect("newest");
    let oldest = vcx.debug_bounds("notification-1").expect("oldest");
    assert!(newest.top() > oldest.top());
    assert_eq!(
        cx.update(|cx| service.entity().read(cx).placement_used().cloned()),
        Some(PlacementUsed::Positioned)
    );
}

#[gpui_kit::test]
fn surface_does_not_keep_the_app_alive(cx: &mut TestAppContext) {
    reduced(cx);
    let (service, _log, _sub) = setup(cx, Default::default());
    struct Main;
    impl gpui_kit::Render for Main {
        fn render(
            &mut self,
            _: &mut gpui_kit::Window,
            _: &mut gpui_kit::Context<Self>,
        ) -> impl gpui_kit::IntoElement {
            gpui_kit::div()
        }
    }
    let main = cx.update(|cx| {
        crate::WindowManager::init(Default::default(), cx);
        crate::WindowManager::open(
            crate::WindowKey::kind("main"),
            crate::OpenMode::Reuse,
            Default::default(),
            |_, cx| gpui_kit::AppContext::new(cx, |_| Main),
            cx,
        )
        .expect("open")
    });
    cx.update(|cx| service.show(WindowNotification::new("still here").sticky(), cx));
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 2);
    let surface: gpui_kit::AnyWindowHandle = cx
        .update(|cx| service.entity().read(cx).window())
        .expect("surface")
        .into();
    let main_id = gpui_kit::AnyWindowHandle::from(main).window_id();
    let open = [main_id, surface.window_id()];
    let auxiliary = std::collections::HashSet::from([surface.window_id()]);
    assert!(
        crate::manager::should_quit(
            crate::QuitPolicy::WhenLastWindowCloses,
            false,
            &open,
            main_id,
            &auxiliary
        ),
        "closing the last app window quits despite the notification"
    );
    assert!(!crate::manager::should_quit(
        crate::QuitPolicy::WhenLastWindowCloses,
        false,
        &open,
        main_id,
        &Default::default()
    ));
    assert!(cx.update(|cx| crate::WindowManager::request_close(main.into(), cx)));
}
