//! Desktop integration: notification windows (guiraffe-windows), system notifications
//! (guiraffe-overlays `desktop`) and the system tray (guiraffe-tray).
//!
//! Nothing touches the OS until a button is pressed, so headless renders stay side-effect free.

use std::time::Duration;

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, component::button::Button, div,
};
use guiraffe_components::Text;
use guiraffe_core::Size;
use guiraffe_overlays::desktop::{self, DesktopNotification, SystemNotifications, Urgency};
use guiraffe_tray::{
    Badge, MenuEntry, RgbaIcon, Tray, TrayEvent, TrayIcon, TrayOptions, TrayStatus,
};
use guiraffe_windows::notify::{
    Corner, NotificationCard, NotificationKind, NotificationWindows, NotificationWindowsConfig,
    StackConfig, WindowNotification, WindowNotificationId,
};

use super::{column, example};
use crate::{catalog::Entry, events::EventLog};

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "windows",
            name: "Notification windows",
            summary: "Themed notification cards in a borderless always-on-top surface stacked in a screen corner: actions, progress, auto-dismiss paused on hover.",
            covers: &["NotificationCard"],
            demo: notification_windows,
        },
        Entry {
            krate: "overlays",
            name: "System notifications",
            summary: "OS notifications (freedesktop / macOS / Windows) with action buttons, update-in-place and typed click and close events.",
            covers: &[],
            demo: system_notifications,
        },
        Entry {
            krate: "tray",
            name: "System tray",
            summary: "Tray icon with a menu built from the command registry, tooltip, click/double-click events and a live badge.",
            covers: &[],
            demo: tray,
        },
    ]
}

fn row(children: impl IntoIterator<Item = Button>) -> gpui_kit::Div {
    div().flex().flex_wrap().gap_2().children(children)
}

// ------------------------------------------------------------ notification windows

struct WindowsDemo {
    log: Entity<EventLog>,
    corner: usize,
    progress: Option<WindowNotificationId>,
    _sub: Option<Subscription>,
}

const CORNERS: [(Corner, &str); 4] = [
    (Corner::TopRight, "↗ top right"),
    (Corner::BottomRight, "↘ bottom right"),
    (Corner::BottomLeft, "↙ bottom left"),
    (Corner::TopLeft, "↖ top left"),
];

fn notification_windows(_: &mut Window, cx: &mut App) -> AnyView {
    let log = EventLog::new(cx);
    cx.new(|_| WindowsDemo {
        log,
        corner: 0,
        progress: None,
        _sub: None,
    })
    .into()
}

impl WindowsDemo {
    fn service(&mut self, cx: &mut Context<Self>) -> NotificationWindows {
        let service = NotificationWindows::global(cx);
        if self._sub.is_none() {
            let log = self.log.clone();
            self._sub = Some(service.subscribe(cx, move |e, cx| {
                EventLog::log(&log, format!("{:?} {:?}", e.id, e.kind), cx)
            }));
        }
        service
    }

    fn show(&mut self, n: WindowNotification, cx: &mut Context<Self>) {
        let service = self.service(cx);
        service.show(n, cx);
    }

    fn cycle_corner(&mut self, cx: &mut Context<Self>) {
        self.corner = (self.corner + 1) % CORNERS.len();
        let corner = CORNERS[self.corner].0;
        guiraffe_windows::notify::init(
            NotificationWindowsConfig {
                stack: StackConfig {
                    corner,
                    ..Default::default()
                },
                ..Default::default()
            },
            cx,
        );
        cx.notify();
    }

    fn progress(&mut self, cx: &mut Context<Self>) {
        let service = self.service(cx);
        let id = service.show(
            WindowNotification::new("Exporting report")
                .body("report-2026.pdf")
                .progress(0.0)
                .sticky(),
            cx,
        );
        self.progress = Some(id);
        cx.spawn(async move |_, cx| {
            for step in 1..=10 {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let done = step == 10;
                let mut card = WindowNotification::new(if done {
                    "Export finished"
                } else {
                    "Exporting report"
                })
                .body("report-2026.pdf")
                .progress(step as f32 / 10.0);
                card = if done {
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
}

impl Render for WindowsDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buttons = [
            Button::new("nw-info")
                .label("Info")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show(
                        WindowNotification::new("New message")
                            .body("Ada: the build is green, shipping 0.2 now."),
                        cx,
                    )
                })),
            Button::new("nw-actions")
                .label("With actions")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show(
                        WindowNotification::new("Update ready")
                            .body("Restart to finish installing GUIraffe 0.2.")
                            .kind(NotificationKind::Success)
                            .action("later", "Later")
                            .primary_action("restart", "Restart")
                            .timeout(Some(Duration::from_secs(10))),
                        cx,
                    )
                })),
            Button::new("nw-error")
                .label("Error")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show(
                        WindowNotification::new("Sync failed")
                            .body("The server could not be reached.")
                            .kind(NotificationKind::Error)
                            .action("retry", "Retry")
                            .sticky(),
                        cx,
                    )
                })),
            Button::new("nw-progress")
                .label("Progress (updates in place)")
                .on_click(cx.listener(|this, _, _, cx| this.progress(cx))),
            Button::new("nw-corner")
                .label(format!("Corner: {}", CORNERS[self.corner].1))
                .on_click(cx.listener(|this, _, _, cx| this.cycle_corner(cx))),
            Button::new("nw-clear")
                .label("Clear")
                .on_click(cx.listener(|this, _, _, cx| {
                    let service = this.service(cx);
                    service.clear(cx);
                })),
        ];
        column([
            example(
                "Shown outside this window, stacked in a screen corner; hover pauses the countdown",
                div().flex().flex_col().gap_2().child(row(buttons)).child(
                    Text::new("Wayland: a layer-shell overlay where the compositor supports it.")
                        .size(Size::Xs)
                        .dimmed(true),
                ),
            ),
            example(
                "The card itself (usable in-app too)",
                div()
                    .w(gpui_kit::px(360.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(NotificationCard::new(
                        "nw-preview-1",
                        WindowNotification::new("Download complete")
                            .body("guiraffe-0.2.0.tar.gz (42 MB)")
                            .kind(NotificationKind::Success)
                            .primary_action("open", "Open"),
                    ))
                    .child(NotificationCard::new(
                        "nw-preview-2",
                        WindowNotification::new("Uploading photos")
                            .body("128 of 300")
                            .progress(0.43),
                    )),
            ),
            example("Event log", self.log.clone()),
        ])
    }
}

// ------------------------------------------------------------ system notifications

struct SystemDemo {
    log: Entity<EventLog>,
    last: Option<desktop::NotificationId>,
    _sub: Option<Subscription>,
}

fn system_notifications(_: &mut Window, cx: &mut App) -> AnyView {
    let log = EventLog::new(cx);
    cx.new(|_| SystemDemo {
        log,
        last: None,
        _sub: None,
    })
    .into()
}

impl SystemDemo {
    fn service(&mut self, cx: &mut Context<Self>) -> Option<SystemNotifications> {
        desktop::init(cx);
        let service = SystemNotifications::global(cx).ok()?;
        if self._sub.is_none() {
            let log = self.log.clone();
            self._sub = Some(service.subscribe(cx, move |e, cx| {
                EventLog::log(&log, format!("{:?} {:?}", e.id, e.kind), cx)
            }));
        }
        Some(service)
    }
    fn show(&mut self, note: DesktopNotification, cx: &mut Context<Self>) {
        if let Some(service) = self.service(cx) {
            self.last = Some(service.show(note.app_name("GUIraffe showcase"), cx));
        }
    }
}

impl Render for SystemDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buttons = [
            Button::new("sn-simple")
                .label("Simple")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show(
                        DesktopNotification::new(
                            "Hello from GUIraffe",
                            "A plain system notification.",
                        ),
                        cx,
                    )
                })),
            Button::new("sn-actions")
                .label("With actions")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show(
                        DesktopNotification::new("Meeting in 5 minutes", "Design review — room 2")
                            .icon_name("appointment-soon")
                            .action("join", "Join")
                            .action("snooze", "Snooze"),
                        cx,
                    )
                })),
            Button::new("sn-critical")
                .label("Critical")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show(
                        DesktopNotification::new("Battery low", "5% remaining")
                            .urgency(Urgency::Critical)
                            .icon_name("battery-caution"),
                        cx,
                    )
                })),
            Button::new("sn-update")
                .label("Update last in place")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let (Some(id), Some(service)) = (this.last, this.service(cx)) {
                        service.update(
                            id,
                            DesktopNotification::new("Updated", "Replaced in place, same id.")
                                .app_name("GUIraffe showcase"),
                            cx,
                        );
                    }
                })),
            Button::new("sn-close")
                .label("Close last")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let (Some(id), Some(service)) = (this.last, this.service(cx))
                        && let Err(e) = service.close(id, cx)
                    {
                        EventLog::log(&this.log, e.to_string(), cx);
                    }
                })),
        ];
        column([
            example(
                "Delivered by the OS notification service; clicks, actions and closes come back as events",
                row(buttons),
            ),
            example("Event log", self.log.clone()),
        ])
    }
}

// ------------------------------------------------------------ tray

struct TrayDemo {
    log: Entity<EventLog>,
    tray: Option<Tray>,
    badge: u32,
    _sub: Option<Subscription>,
}

fn tray(_: &mut Window, cx: &mut App) -> AnyView {
    let log = EventLog::new(cx);
    cx.new(|_| TrayDemo {
        log,
        tray: None,
        badge: 0,
        _sub: None,
    })
    .into()
}

impl TrayDemo {
    fn toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(tray) = self.tray.take() {
            tray.remove(cx);
            self._sub = None;
            EventLog::log(&self.log, "tray removed", cx);
            cx.notify();
            return;
        }
        let tray = Tray::new(
            TrayOptions {
                id: "guiraffe-showcase".into(),
                title: "GUIraffe showcase".into(),
                tooltip_title: "GUIraffe showcase".into(),
                tooltip_body: "Right-click for the menu".into(),
                icon: Some(TrayIcon::Rgba(RgbaIcon::disc(32, [250, 176, 5, 255]))),
                ..Default::default()
            },
            cx,
        );
        tray.set_menu(
            guiraffe_tray::build_menu(
                &[MenuEntry::Custom {
                    id: "hello".into(),
                    label: "Say hello".into(),
                    enabled: true,
                }],
                &guiraffe_commands::Registry::<()>::default(),
                &Default::default(),
            ),
            cx,
        );
        let log = self.log.clone();
        self._sub = Some(tray.subscribe(cx, move |e: &TrayEvent, cx| {
            EventLog::log(&log, format!("{e:?}"), cx)
        }));
        self.tray = Some(tray);
        cx.notify();
    }
}

impl Render for TrayDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status =
            self.tray
                .as_ref()
                .map(|t| t.status(cx))
                .map_or("off".to_string(), |s| match s {
                    TrayStatus::Failed(e) => format!("failed: {e}"),
                    other => format!("{other:?}"),
                });
        let on = self.tray.is_some();
        let buttons = [
            Button::new("tray-toggle")
                .label(if on {
                    "Remove tray icon"
                } else {
                    "Show tray icon"
                })
                .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
            Button::new("tray-badge")
                .label("Badge +1")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.badge += 1;
                    if let Some(tray) = &this.tray {
                        tray.set_badge(Badge::Count(this.badge), cx);
                    }
                })),
            Button::new("tray-clear")
                .label("Clear badge")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.badge = 0;
                    if let Some(tray) = &this.tray {
                        tray.set_badge(Badge::None, cx);
                    }
                })),
        ];
        column([
            example(
                "StatusNotifierItem on Linux (needs a tray host: KDE, waybar, GNOME + AppIndicator)",
                div().flex().flex_col().gap_2().child(row(buttons)).child(
                    Text::new(format!("Status: {status}"))
                        .size(Size::Sm)
                        .dimmed(true),
                ),
            ),
            example("Event log", self.log.clone()),
        ])
    }
}
