//! Linux/BSD: StatusNotifierItem over D-Bus via `ksni` (pure Rust, zbus; no GTK, works on
//! Wayland). Shown by KDE Plasma, waybar/ironbar/eww trays, GNOME with the AppIndicator
//! extension, and other SNI hosts. Tested on Linux.
use std::sync::{Arc, Mutex};

use gpui_kit::{App, Task};
use ksni::{
    Icon, MenuItem, Status, ToolTip, TrayMethods,
    menu::{CheckmarkItem, StandardItem, SubMenu},
};

use crate::{
    TrayError,
    backend::{BackendEvent, EventSink, TrayBackend, TrayState},
    icon::{Badge, RgbaIcon, TrayIcon},
    menu::TrayMenuItem,
};

struct SniTray {
    state: TrayState,
    events: EventSink,
}

fn pixmap(icon: &RgbaIcon) -> Icon {
    Icon {
        width: icon.width() as i32,
        height: icon.height() as i32,
        data: icon.to_argb32_be(),
    }
}

/// SNI labels treat `_` as a mnemonic marker; show underscores literally.
fn label(text: &str) -> String {
    text.replace('_', "__")
}

fn menu(items: &[TrayMenuItem]) -> Vec<MenuItem<SniTray>> {
    items
        .iter()
        .map(|item| match item {
            TrayMenuItem::Separator => MenuItem::Separator,
            TrayMenuItem::Submenu { label: text, items } => SubMenu {
                label: label(text),
                submenu: menu(items),
                ..Default::default()
            }
            .into(),
            TrayMenuItem::Item {
                id,
                label: text,
                enabled,
                checked,
            } => {
                let id = id.clone();
                let activate = Box::new(move |tray: &mut SniTray| {
                    tray.events.send(BackendEvent::MenuItem(id.clone()));
                });
                match checked {
                    Some(checked) => CheckmarkItem {
                        label: label(text),
                        enabled: *enabled,
                        checked: *checked,
                        activate,
                        ..Default::default()
                    }
                    .into(),
                    None => StandardItem {
                        label: label(text),
                        enabled: *enabled,
                        activate,
                        ..Default::default()
                    }
                    .into(),
                }
            }
        })
        .collect()
}

impl ksni::Tray for SniTray {
    fn id(&self) -> String {
        self.state.id.clone()
    }
    fn title(&self) -> String {
        self.state.title.clone()
    }
    fn status(&self) -> Status {
        if self.state.attention {
            Status::NeedsAttention
        } else {
            Status::Active
        }
    }
    fn icon_name(&self) -> String {
        match &self.state.icon {
            Some(TrayIcon::Named(name)) => name.clone(),
            _ => String::new(),
        }
    }
    fn icon_pixmap(&self) -> Vec<Icon> {
        match &self.state.icon {
            Some(TrayIcon::Rgba(icon)) => {
                vec![pixmap(
                    &icon.with_badge(self.state.badge, self.state.badge_color),
                )]
            }
            _ => Vec::new(),
        }
    }
    fn attention_icon_pixmap(&self) -> Vec<Icon> {
        self.icon_pixmap()
    }
    fn attention_icon_name(&self) -> String {
        self.icon_name()
    }
    fn overlay_icon_pixmap(&self) -> Vec<Icon> {
        // Named icons can't be composited here: hosts that support overlays draw the badge.
        match (&self.state.icon, self.state.badge) {
            (Some(TrayIcon::Named(_)), badge) if badge != Badge::None => vec![pixmap(
                &RgbaIcon::badge_overlay(32, 32, badge, self.state.badge_color),
            )],
            _ => Vec::new(),
        }
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.state.tooltip_title.clone(),
            description: self.state.tooltip_body.clone(),
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.events.send(BackendEvent::Activate);
    }
    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        self.events.send(BackendEvent::SecondaryActivate);
    }
    fn scroll(&mut self, delta: i32, orientation: ksni::Orientation) {
        self.events.send(BackendEvent::Scroll {
            delta,
            horizontal: matches!(orientation, ksni::Orientation::Horizontal),
        });
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        menu(&self.state.menu)
    }
    fn watcher_online(&self) {
        self.events.send(BackendEvent::HostAvailable(true));
    }
    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        // Keep the service: the icon reappears when a host (panel) starts again.
        self.events.send(BackendEvent::HostAvailable(false));
        true
    }
}

#[derive(Default)]
struct Shared {
    handle: Option<ksni::Handle<SniTray>>,
    /// The newest snapshot not yet applied (updates coalesce; order can't invert).
    pending: Option<TrayState>,
}

/// The StatusNotifierItem backend.
#[derive(Default)]
pub struct SniBackend {
    shared: Arc<Mutex<Shared>>,
}

impl SniBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

async fn flush(shared: Arc<Mutex<Shared>>) {
    let (handle, state) = {
        let mut s = lock(&shared);
        (s.handle.clone(), s.pending.take())
    };
    if let (Some(handle), Some(state)) = (handle, state) {
        handle.update(move |tray| tray.state = state).await;
    }
}

impl TrayBackend for SniBackend {
    fn name(&self) -> &'static str {
        "StatusNotifierItem"
    }

    fn connect(
        &mut self,
        state: TrayState,
        events: EventSink,
        cx: &mut App,
    ) -> Task<Result<(), TrayError>> {
        let shared = self.shared.clone();
        cx.background_executor().spawn(async move {
            let tray = SniTray { state, events };
            let handle = tray.spawn().await.map_err(|e| match e {
                ksni::Error::Watcher(_) | ksni::Error::WontShow => {
                    TrayError::Unavailable(format!("no StatusNotifierItem host: {e}"))
                }
                other => TrayError::Platform(other.to_string()),
            })?;
            lock(&shared).handle = Some(handle);
            // Apply anything that changed while connecting.
            flush(shared).await;
            Ok(())
        })
    }

    fn update(&mut self, state: TrayState, cx: &mut App) {
        lock(&self.shared).pending = Some(state);
        cx.background_executor()
            .spawn(flush(self.shared.clone()))
            .detach();
    }

    fn shutdown(&mut self, cx: &mut App) {
        let handle = lock(&self.shared).handle.take();
        if let Some(handle) = handle {
            let done = handle.shutdown();
            cx.background_executor().spawn(done).detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::TrayItemId;

    #[test]
    fn menu_converts_items_checks_submenus_and_escapes_mnemonics() {
        let items = vec![
            TrayMenuItem::custom("a", "Open_file"),
            TrayMenuItem::Separator,
            TrayMenuItem::Item {
                id: TrayItemId::Custom("dnd".into()),
                label: "Do not disturb".into(),
                enabled: false,
                checked: Some(true),
            },
            TrayMenuItem::Submenu {
                label: "More".into(),
                items: vec![TrayMenuItem::custom("b", "B")],
            },
        ];
        let out = menu(&items);
        assert_eq!(out.len(), 4);
        match &out[0] {
            MenuItem::Standard(s) => assert_eq!(s.label, "Open__file"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(out[1], MenuItem::Separator));
        match &out[2] {
            MenuItem::Checkmark(c) => assert!(c.checked && !c.enabled),
            other => panic!("unexpected {other:?}"),
        }
        match &out[3] {
            MenuItem::SubMenu(s) => assert_eq!(s.submenu.len(), 1),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn menu_activation_sends_the_item_id() {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let mut tray = SniTray {
            state: TrayState {
                id: "t".into(),
                title: String::new(),
                tooltip_title: String::new(),
                tooltip_body: String::new(),
                icon: None,
                badge: Badge::None,
                badge_color: [0; 4],
                attention: false,
                menu: vec![TrayMenuItem::custom("x", "X")],
            },
            events: EventSink(tx),
        };
        let items = ksni::Tray::menu(&tray);
        if let Some(MenuItem::Standard(item)) = items.first() {
            (item.activate)(&mut tray);
        }
        assert_eq!(
            rx.try_recv().ok(),
            Some(BackendEvent::MenuItem(TrayItemId::Custom("x".into())))
        );
    }
}
