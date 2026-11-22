//! Manual test bed for GUIraffe desktop integration:
//! system notifications (`guiraffe_overlays::desktop`), notification windows
//! (`guiraffe_windows::notify`) and the system tray (`guiraffe_tray`).
//!
//! ```text
//! cargo run -p desktop            # interactive window; every event is printed
//! cargo run -p desktop -- --auto  # scripted run: exercises all three, then quits
//! ```
//!
//! Closing the window hides it to the tray when a tray host is available (click the tray icon
//! to bring it back; use the tray menu's Quit to exit).
use std::time::Duration;

use gpui_kit::{
    App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, WindowOptions,
    component::button::Button, div, px,
};
use guiraffe_commands::{Command, CommandId, Commands, Payload};
use guiraffe_core::{GuiraffeTheme, Size};
use guiraffe_overlays::desktop::{self, DesktopNotification, SystemNotifications, Urgency};
use guiraffe_tray::{
    Badge, HideMode, MenuEntry, RgbaIcon, Tray, TrayEvent, TrayIcon, TrayOptions, minimize_to_tray,
};
use guiraffe_windows::{
    OpenMode, QuitPolicy, WindowConfig, WindowKey, WindowManager,
    notify::{NotificationKind, NotificationWindows, WindowNotification},
};

struct Main {
    commands: Commands,
}

fn log(line: impl AsRef<str>) {
    println!("[desktop] {}", line.as_ref());
}

fn system_notification(cx: &mut App) {
    let Ok(service) = SystemNotifications::global(cx) else {
        return;
    };
    service.show(
        DesktopNotification::new("Meeting in 5 minutes", "Design review — room 2")
            .app_name("GUIraffe desktop")
            .icon_name("appointment-soon")
            .urgency(Urgency::Normal)
            .action("join", "Join")
            .action("snooze", "Snooze"),
        cx,
    );
}

fn window_notifications(cx: &mut App) {
    let service = NotificationWindows::global(cx);
    service.show(
        WindowNotification::new("New message").body("Ada: the build is green, shipping 0.2 now."),
        cx,
    );
    service.show(
        WindowNotification::new("Update ready")
            .body("Restart to finish installing.")
            .kind(NotificationKind::Success)
            .action("later", "Later")
            .primary_action("restart", "Restart")
            .timeout(Some(Duration::from_secs(12))),
        cx,
    );
    let id = service.show(
        WindowNotification::new("Exporting report")
            .body("report-2026.pdf")
            .progress(0.0)
            .sticky(),
        cx,
    );
    cx.spawn(async move |cx| {
        for step in 1..=10 {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let done = step == 10;
            let card = WindowNotification::new(if done {
                "Export finished"
            } else {
                "Exporting report"
            })
            .body("report-2026.pdf")
            .progress(step as f32 / 10.0);
            let card = if done {
                card.kind(NotificationKind::Success)
                    .primary_action("open", "Open")
            } else {
                card.sticky()
            };
            cx.update(|cx| service.update(id, card, cx));
        }
    })
    .detach();
}

impl Render for Main {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        let _ = &self.commands;
        div()
            .size_full()
            .p(px(Size::Lg.spacing()))
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .flex()
            .flex_col()
            .gap(px(Size::Md.spacing()))
            .child(
                div()
                    .text_size(px(Size::Lg.font_size()))
                    .child("Desktop integration"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("system")
                            .label("System notification")
                            .on_click(|_, _, cx| system_notification(cx)),
                    )
                    .child(
                        Button::new("windows")
                            .label("Notification windows")
                            .on_click(|_, _, cx| window_notifications(cx)),
                    )
                    .child(
                        Button::new("clear")
                            .label("Clear notification windows")
                            .on_click(|_, _, cx| NotificationWindows::global(cx).clear(cx)),
                    ),
            )
            .child(
                div().text_color(surfaces.dimmed.to_hsla()).child(
                    "Close this window to hide it to the tray (when a tray host is running).",
                ),
            )
    }
}

fn open_main(commands: Commands, cx: &mut App) {
    let opened = WindowManager::open(
        WindowKey::kind("main"),
        OpenMode::Reuse,
        WindowOptions {
            titlebar: Some(gpui_kit::TitlebarOptions {
                title: Some("GUIraffe desktop integration".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|_| Main { commands }),
        cx,
    );
    if let Err(e) = opened {
        log(format!("failed to open the main window: {e:#}"));
    }
}

fn register(commands: &Commands, cx: &mut App) {
    let cmd = |id: &str| CommandId::parse(id).ok();
    type Entry = (&'static str, &'static str, fn(&mut App));
    let entries: [Entry; 3] = [
        (
            "app.notify-system",
            "Send system notification",
            system_notification,
        ),
        (
            "app.notify-window",
            "Show notification windows",
            window_notifications,
        ),
        ("app.quit", "Quit", |cx| cx.quit()),
    ];
    for (id, title, run) in entries {
        if let Some(id) = cmd(id) {
            let _ = commands.register(
                Command::new(
                    id,
                    title,
                    move |_: &Payload, _: &mut Window, cx: &mut App| run(cx),
                ),
                cx,
            );
        }
    }
}

fn main() {
    let auto = std::env::args().any(|a| a == "--auto");
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::component::init(cx);
            guiraffe_core::init(cx);
            guiraffe_commands::init(cx);
            desktop::init(cx);
            WindowManager::init(
                WindowConfig {
                    quit_policy: QuitPolicy::WhenLastWindowCloses,
                    ..Default::default()
                },
                cx,
            );

            // Every event, printed for manual testing.
            if let Ok(service) = SystemNotifications::global(cx) {
                service
                    .subscribe(cx, |e, _| log(format!("system notification {:?}: {:?}", e.id, e.kind)))
                    .detach();
            }
            NotificationWindows::global(cx)
                .subscribe(cx, |e, _| log(format!("notification window {:?}: {:?}", e.id, e.kind)))
                .detach();

            let commands = Commands::new(cx);
            register(&commands, cx);
            open_main(commands.clone(), cx);

            // The tray: menu from the command registry plus a custom "Show window" item.
            let tray = Tray::new(
                TrayOptions {
                    id: "guiraffe-desktop".into(),
                    title: "GUIraffe".into(),
                    tooltip_title: "GUIraffe desktop integration".into(),
                    tooltip_body: "Click: show window · right-click: menu".into(),
                    icon: Some(TrayIcon::Rgba(RgbaIcon::disc(64, [250, 176, 5, 255]))),
                    ..Default::default()
                },
                cx,
            );
            log(format!("tray backend: {}", tray.entity().read(cx).backend_name()));
            let mut entries = vec![
                MenuEntry::Custom {
                    id: "show".into(),
                    label: "Show window".into(),
                    enabled: true,
                },
                MenuEntry::Separator,
            ];
            entries.extend(
                ["app.notify-system", "app.notify-window"]
                    .into_iter()
                    .filter_map(MenuEntry::command),
            );
            entries.push(MenuEntry::Separator);
            entries.extend(MenuEntry::command("app.quit"));
            tray.set_menu_from_commands(&entries, &commands, cx);
            if let Some((_, handle)) = WindowManager::windows(cx).into_iter().next() {
                tray.route_commands(&commands, handle, cx);
                let reopen_commands = commands.clone();
                let tray_for_window = tray.clone();
                let _ = handle.update(cx, move |_, window, cx| {
                    minimize_to_tray(
                        window,
                        &tray_for_window,
                        HideMode::Close,
                        move |cx| open_main(reopen_commands.clone(), cx),
                        cx,
                    )
                });
            }
            {
                let tray_handle = tray.clone();
                let menu_commands = commands.clone();
                tray.subscribe(cx, move |e, cx| {
                    log(format!("tray {e:?}"));
                    match e {
                        TrayEvent::MenuItem(guiraffe_tray::TrayItemId::Custom(id)) if id == "show" => {
                            if !guiraffe_tray::restore(&tray_handle, cx) {
                                open_main(menu_commands.clone(), cx);
                            }
                        }
                        TrayEvent::MenuItem(guiraffe_tray::TrayItemId::Command(id))
                            if id.as_str() == "app.quit" =>
                        {
                            // Routed commands need a window; quit works even when hidden.
                            cx.quit();
                        }
                        TrayEvent::DoubleClick => {
                            tray_handle.set_badge(Badge::None, cx);
                        }
                        _ => {}
                    }
                })
                .detach();
            }

            if auto {
                let tray = tray.clone();
                cx.spawn(async move |cx| {
                    let wait = |ms| cx.background_executor().timer(Duration::from_millis(ms));
                    wait(800).await;
                    cx.update(|cx| {
                        log(format!("tray status: {:?}", tray.status(cx)));
                        system_notification(cx);
                        window_notifications(cx);
                    });
                    wait(1500).await;
                    cx.update(|cx| {
                        let stack = NotificationWindows::global(cx);
                        let s = stack.entity().read(cx);
                        log(format!(
                            "notification surface: window={:?} placement={:?} error={:?} layout={:?}x{:?}",
                            s.window().map(|w| w.window_id()),
                            s.placement_used(),
                            s.last_error(),
                            s.layout().width,
                            s.layout().height,
                        ));
                        tray.set_badge(Badge::Count(3), cx);
                        tray.set_tooltip("GUIraffe desktop integration", "3 unread", cx);
                        log(format!("tray status: {:?}", tray.status(cx)));
                    });
                    wait(5000).await;
                    cx.update(|cx| {
                        if let Ok(service) = SystemNotifications::global(cx) {
                            for id in service.entity().read(cx).open_ids() {
                                let _ = service.close(id, cx);
                            }
                        }
                    });
                    wait(500).await;
                    cx.update(|cx| {
                        tray.remove(cx);
                        log("auto run finished");
                        cx.quit();
                    });
                })
                .detach();
            }
        });
}
