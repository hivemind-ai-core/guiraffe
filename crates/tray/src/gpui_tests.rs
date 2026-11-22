//! Service tests on the mock backend: no D-Bus, no tray host.
use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::{
    AppContext, Context, IntoElement, Render, TestAppContext, Window, WindowOptions, div,
};
use guiraffe_commands::{Command, CommandId, Commands, Payload};
use guiraffe_windows::{OpenMode, QuitPolicy, WindowKey, WindowManager};

use crate::{
    BackendEvent, Badge, HideMode, MenuEntry, MockTray, RgbaIcon, Tray, TrayError, TrayEvent,
    TrayIcon, TrayItemId, TrayMenuItem, TrayOptions, TrayStatus, minimize_to_tray, restore,
};

fn events(
    tray: &Tray,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<TrayEvent>>>, gpui_kit::Subscription) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    let sub = cx.update(|cx| tray.subscribe(cx, move |e, _| sink.borrow_mut().push(e.clone())));
    (log, sub)
}

fn mock_tray(cx: &mut TestAppContext) -> (Tray, MockTray) {
    let mock = MockTray::new();
    let tray = cx.update(|cx| {
        Tray::with_backend(
            TrayOptions {
                id: "test".into(),
                icon: Some(TrayIcon::Rgba(RgbaIcon::disc(16, [0, 0, 255, 255]))),
                ..Default::default()
            },
            Some(Box::new(mock.clone())),
            cx,
        )
    });
    (tray, mock)
}

#[gpui_kit::test]
fn connects_and_pushes_snapshots_only_on_change(cx: &mut TestAppContext) {
    let (tray, mock) = mock_tray(cx);
    let (log, _sub) = events(&tray, cx);
    // Changed while connecting: applied once ready.
    cx.update(|cx| tray.set_tooltip("Syncing", "3 files", cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| tray.status(cx)), TrayStatus::Ready);
    assert_eq!(*log.borrow(), [TrayEvent::Ready]);
    assert_eq!(mock.last().map(|s| s.tooltip_body), Some("3 files".into()));
    let n = mock.states().len();
    cx.update(|cx| tray.set_badge(Badge::Count(2), cx));
    cx.update(|cx| tray.set_badge(Badge::Count(2), cx));
    assert_eq!(mock.states().len(), n + 1, "no-op edits are not pushed");
    assert_eq!(mock.last().map(|s| s.badge), Some(Badge::Count(2)));
}

#[gpui_kit::test]
fn clicks_and_double_clicks(cx: &mut TestAppContext) {
    let (tray, mock) = mock_tray(cx);
    cx.run_until_parked();
    let (log, _sub) = events(&tray, cx);
    mock.send(BackendEvent::Activate);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(150));
    mock.send(BackendEvent::Activate);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    mock.send(BackendEvent::Activate);
    mock.send(BackendEvent::SecondaryActivate);
    mock.send(BackendEvent::Scroll {
        delta: -1,
        horizontal: false,
    });
    cx.run_until_parked();
    assert_eq!(
        *log.borrow(),
        [
            TrayEvent::Click,
            TrayEvent::DoubleClick,
            TrayEvent::Click,
            TrayEvent::MiddleClick,
            TrayEvent::Scroll {
                delta: -1,
                horizontal: false
            }
        ]
    );
}

#[gpui_kit::test]
fn connect_failure_is_reported(cx: &mut TestAppContext) {
    let mock = MockTray::failing(TrayError::Unavailable("no host".into()));
    let tray = cx
        .update(|cx| Tray::with_backend(TrayOptions::default(), Some(Box::new(mock.clone())), cx));
    let (log, _sub) = events(&tray, cx);
    cx.run_until_parked();
    let err = TrayError::Unavailable("no host".into());
    assert_eq!(
        cx.update(|cx| tray.status(cx)),
        TrayStatus::Failed(err.clone())
    );
    assert_eq!(*log.borrow(), [TrayEvent::Error(err)]);
    assert!(mock.states().is_empty());
}

#[gpui_kit::test]
fn no_backend_is_unsupported(cx: &mut TestAppContext) {
    let tray = cx.update(|cx| Tray::with_backend(TrayOptions::default(), None, cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| tray.status(cx)),
        TrayStatus::Failed(TrayError::Unsupported)
    );
}

struct Empty;
impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui_kit::test]
fn menu_from_commands_routes_clicks_to_handlers(cx: &mut TestAppContext) {
    let ran = Rc::new(RefCell::new(Vec::<String>::new()));
    let log = ran.clone();
    let window = cx.add_window(|_, _| Empty);
    let commands = cx.update(|cx| {
        let commands = Commands::new(cx);
        let id = CommandId::parse("sync.now").expect("id");
        commands
            .register(
                Command::new(
                    id,
                    "Sync now",
                    move |p: &Payload, _: &mut Window, _: &mut gpui_kit::App| {
                        log.borrow_mut().push(p.id.to_string())
                    },
                )
                .category("Sync"),
                cx,
            )
            .expect("register");
        commands
            .register(
                Command::new(
                    CommandId::parse("sync.off").expect("id"),
                    "Disabled",
                    |_: &Payload, _: &mut Window, _: &mut gpui_kit::App| {},
                )
                .enabled(false),
                cx,
            )
            .expect("register");
        commands
    });
    let (tray, mock) = mock_tray(cx);
    cx.run_until_parked();
    let (log_events, _sub) = events(&tray, cx);
    cx.update(|cx| {
        tray.set_menu_from_commands(
            &[
                MenuEntry::Category("Sync".into()),
                MenuEntry::Separator,
                MenuEntry::command("sync.off").expect("id"),
                MenuEntry::Custom {
                    id: "quit".into(),
                    label: "Quit".into(),
                    enabled: true,
                },
            ],
            &commands,
            cx,
        );
        tray.route_commands(&commands, window.into(), cx);
    });
    let menu = mock.last().map(|s| s.menu).unwrap_or_default();
    assert_eq!(menu.len(), 4);
    assert!(matches!(
        &menu[2],
        TrayMenuItem::Item { enabled: false, .. }
    ));
    let sync = TrayItemId::Command(CommandId::parse("sync.now").expect("id"));
    mock.send(BackendEvent::MenuItem(sync.clone()));
    mock.send(BackendEvent::MenuItem(TrayItemId::Custom("quit".into())));
    cx.run_until_parked();
    assert_eq!(*ran.borrow(), ["sync.now"]);
    assert_eq!(
        *log_events.borrow(),
        [
            TrayEvent::MenuItem(sync),
            TrayEvent::MenuItem(TrayItemId::Custom("quit".into()))
        ]
    );
}

#[gpui_kit::test]
fn minimize_to_tray_hides_on_close_and_restores_on_click(cx: &mut TestAppContext) {
    let (tray, mock) = mock_tray(cx);
    cx.run_until_parked();
    let reopened = Rc::new(RefCell::new(0));
    let count = reopened.clone();
    let key = WindowKey::kind("main");
    let handle = cx.update(|cx| {
        WindowManager::init(Default::default(), cx);
        WindowManager::open(
            key.clone(),
            OpenMode::Reuse,
            WindowOptions::default(),
            |_, cx| cx.new(|_| Empty),
            cx,
        )
        .expect("open")
    });
    let guard = handle
        .update(cx, |_, window, cx| {
            let reopen_key = key.clone();
            minimize_to_tray(
                window,
                &tray,
                HideMode::Close,
                move |cx| {
                    *count.borrow_mut() += 1;
                    let _ = WindowManager::open(
                        reopen_key.clone(),
                        OpenMode::Reuse,
                        WindowOptions::default(),
                        |_, cx| cx.new(|_| Empty),
                        cx,
                    );
                },
                cx,
            )
        })
        .expect("window");
    assert!(guard.is_some());
    assert_eq!(
        cx.update(|cx| WindowManager::quit_policy(cx)),
        QuitPolicy::Never
    );

    // The user closes the window: vetoed, then removed; the app keeps running.
    let closed = cx.update(|cx| WindowManager::request_close(handle.into(), cx));
    assert!(!closed, "the managed close is vetoed");
    cx.run_until_parked();
    assert!(
        cx.update(|cx| WindowManager::find(&key, cx)).is_none(),
        "window hidden (closed)"
    );
    assert!(cx.update(|cx| tray.entity().read(cx).is_hidden()));

    // A tray click restores it.
    mock.send(BackendEvent::Activate);
    cx.run_until_parked();
    assert_eq!(*reopened.borrow(), 1);
    assert!(cx.update(|cx| WindowManager::find(&key, cx)).is_some());
    assert!(!cx.update(|cx| tray.entity().read(cx).is_hidden()));
    assert!(
        !cx.update(|cx| restore(&tray, cx)),
        "nothing hidden any more"
    );
}

#[gpui_kit::test]
fn without_a_tray_host_closing_works_normally(cx: &mut TestAppContext) {
    let tray = cx.update(|cx| {
        Tray::with_backend(
            TrayOptions::default(),
            Some(Box::new(MockTray::failing(TrayError::Unavailable(
                "x".into(),
            )))),
            cx,
        )
    });
    cx.run_until_parked();
    let key = WindowKey::kind("main");
    let handle = cx.update(|cx| {
        WindowManager::init(
            guiraffe_windows::WindowConfig {
                quit_policy: QuitPolicy::Never,
                ..Default::default()
            },
            cx,
        );
        WindowManager::open(
            key.clone(),
            OpenMode::Reuse,
            WindowOptions::default(),
            |_, cx| cx.new(|_| Empty),
            cx,
        )
        .expect("open")
    });
    handle
        .update(cx, |_, window, cx| {
            minimize_to_tray(window, &tray, HideMode::Close, |_| {}, cx)
        })
        .expect("window");
    assert!(cx.update(|cx| WindowManager::request_close(handle.into(), cx)));
}

#[gpui_kit::test]
fn remove_shuts_the_backend_down(cx: &mut TestAppContext) {
    let (tray, mock) = mock_tray(cx);
    cx.run_until_parked();
    cx.update(|cx| tray.remove(cx));
    assert!(mock.is_shut_down());
    assert_eq!(cx.update(|cx| tray.status(cx)), TrayStatus::Removed);
    assert!(!mock.send(BackendEvent::Activate));
}
