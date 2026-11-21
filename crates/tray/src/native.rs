//! macOS and Windows: `tray-icon` (NSStatusItem / Shell_NotifyIcon) with `muda` menus.
//!
//! **UNTESTED**: written against tray-icon 0.26 / muda 0.21 sources but never built or run on
//! macOS or Windows (no hardware or SDKs in this project's CI). See KNOWN_ISSUES.md.
//!
//! The icon must be created on the main thread, which GPUI's foreground executor is; GPUI runs
//! the platform event loop those APIs need.
use std::sync::{Mutex, OnceLock};

use gpui_kit::{App, Task};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon as NativeTray, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::{
    TrayError,
    backend::{BackendEvent, EventSink, TrayBackend, TrayState},
    icon::TrayIcon,
    menu::{TrayItemId, TrayMenuItem},
};

/// tray-icon/muda event handlers are process-wide and settable once: route through a slot.
fn sink() -> &'static Mutex<Option<EventSink>> {
    static SINK: OnceLock<Mutex<Option<EventSink>>> = OnceLock::new();
    SINK.get_or_init(|| {
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            let raw = match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => Some(BackendEvent::Activate),
                TrayIconEvent::Click {
                    button: MouseButton::Middle,
                    button_state: MouseButtonState::Up,
                    ..
                } => Some(BackendEvent::SecondaryActivate),
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => Some(BackendEvent::DoubleActivate),
                _ => None,
            };
            if let Some(raw) = raw {
                send(raw);
            }
        }));
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            if let Some(id) = decode_id(event.id.as_ref()) {
                send(BackendEvent::MenuItem(id));
            }
        }));
        Mutex::new(None)
    })
}

fn send(event: BackendEvent) {
    if let Some(s) = sink().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        s.send(event);
    }
}

fn encode_id(id: &TrayItemId) -> String {
    match id {
        TrayItemId::Command(c) => format!("c:{c}"),
        TrayItemId::Custom(s) => format!("x:{s}"),
    }
}

fn decode_id(raw: &str) -> Option<TrayItemId> {
    if let Some(c) = raw.strip_prefix("c:") {
        guiraffe_commands::CommandId::parse(c)
            .ok()
            .map(TrayItemId::Command)
    } else {
        raw.strip_prefix("x:").map(|s| TrayItemId::Custom(s.into()))
    }
}

/// muda treats `&` as a mnemonic marker.
fn label(text: &str) -> String {
    text.replace('&', "&&")
}

fn build_menu(items: &[TrayMenuItem]) -> Result<Menu, TrayError> {
    let menu = Menu::new();
    for item in items {
        append(item, |i| menu.append(i))?;
    }
    Ok(menu)
}

fn append(
    item: &TrayMenuItem,
    add: impl Fn(&dyn tray_icon::menu::IsMenuItem) -> tray_icon::menu::Result<()>,
) -> Result<(), TrayError> {
    let err = |e: tray_icon::menu::Error| TrayError::Platform(e.to_string());
    match item {
        TrayMenuItem::Separator => add(&PredefinedMenuItem::separator()).map_err(err),
        TrayMenuItem::Item {
            id,
            label: text,
            enabled,
            checked: None,
        } => add(&MenuItem::with_id(
            encode_id(id),
            label(text),
            *enabled,
            None,
        ))
        .map_err(err),
        TrayMenuItem::Item {
            id,
            label: text,
            enabled,
            checked: Some(checked),
        } => add(&CheckMenuItem::with_id(
            encode_id(id),
            label(text),
            *enabled,
            *checked,
            None,
        ))
        .map_err(err),
        TrayMenuItem::Submenu { label: text, items } => {
            let sub = Submenu::new(label(text), true);
            for child in items {
                append(child, |i| sub.append(i))?;
            }
            add(&sub).map_err(err)
        }
    }
}

fn icon(state: &TrayState) -> Option<Icon> {
    match &state.icon {
        Some(TrayIcon::Rgba(rgba)) => {
            let badged = rgba.with_badge(state.badge, state.badge_color);
            Icon::from_rgba(badged.rgba().to_vec(), badged.width(), badged.height()).ok()
        }
        // Icon-theme names are a freedesktop concept.
        _ => None,
    }
}

fn tooltip(state: &TrayState) -> String {
    if state.tooltip_body.is_empty() {
        state.tooltip_title.clone()
    } else {
        format!("{}\n{}", state.tooltip_title, state.tooltip_body)
    }
}

/// The macOS / Windows backend.
#[derive(Default)]
pub struct NativeBackend {
    tray: Option<NativeTray>,
}

impl NativeBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TrayBackend for NativeBackend {
    fn name(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "NSStatusItem"
        } else {
            "Shell_NotifyIcon"
        }
    }

    fn connect(
        &mut self,
        state: TrayState,
        events: EventSink,
        cx: &mut App,
    ) -> Task<Result<(), TrayError>> {
        *sink().lock().unwrap_or_else(|e| e.into_inner()) = Some(events);
        let result = build_menu(&state.menu).and_then(|menu| {
            let mut builder = TrayIconBuilder::new()
                .with_id(state.id.clone())
                .with_tooltip(tooltip(&state))
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false);
            if let Some(icon) = icon(&state) {
                builder = builder.with_icon(icon);
            }
            if cfg!(target_os = "macos") && !state.title.is_empty() && state.icon.is_none() {
                builder = builder.with_title(state.title.clone());
            }
            builder
                .build()
                .map_err(|e| TrayError::Platform(e.to_string()))
        });
        let result = result.map(|tray| {
            self.tray = Some(tray);
        });
        cx.background_executor().spawn(async move { result })
    }

    fn update(&mut self, state: TrayState, _: &mut App) {
        let Some(tray) = &self.tray else { return };
        let _ = tray.set_icon(icon(&state));
        let _ = tray.set_tooltip(Some(tooltip(&state)));
        if let Ok(menu) = build_menu(&state.menu) {
            tray.set_menu(Some(Box::new(menu)));
        }
    }

    fn shutdown(&mut self, _: &mut App) {
        self.tray = None;
        *sink().lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
