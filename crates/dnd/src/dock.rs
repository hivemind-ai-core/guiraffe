//! In-window docking with managed gestures over Kit's versioned layout model.
//!
//! A screen document can supply plain-text `panels` or a serialized `layout`. A
//! `persist-key` connects it to the host's layout store, and `layout-change`
//! emits the same snapshot shape accepted by `layout`. Rust hosts can register
//! their own panel factories for layouts containing application-specific panels.

mod managed;
mod multi_window;
pub use managed::DockDragOverlay;
pub use multi_window::{DockAction, DockControl, DockablePanel};

use gpui_kit::{App, Entity, SharedString, Subscription, Window};
use guiraffe_layout::LayoutStore;

pub use gpui_kit::component::dock::{
    DockArea, DockAreaState, DockEvent, DockPlacement, DockSkin, Panel, PanelBuildContext,
    PanelEvent, PanelRegistry, PanelState, panel_handle, register_panel,
};

/// Construct a dock and its managed pointer-gesture overlay.
///
/// Mount the overlay beside the cached dock host in the same window.
pub fn new_dock(
    id: impl Into<SharedString>,
    version: usize,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<DockArea>, Entity<DockDragOverlay>) {
    init(cx);
    managed::new(id.into(), version, window, cx)
}

/// Opt in to Kit's native drag and dock chrome.
pub fn new_native_dock(
    id: impl Into<SharedString>,
    version: usize,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<DockArea>, std::rc::Rc<DockSkin>) {
    init(cx);
    DockSkin::dock_area(id, Some(version), window, cx)
}

fn init(cx: &mut App) {
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::init(cx);
    }
    if !cx.has_global::<LayoutStore>() {
        cx.set_global(LayoutStore::in_memory());
    }
}

/// Store a snapshot under an application-owned layout key.
pub fn save(
    store: &LayoutStore,
    key: &str,
    state: &DockAreaState,
) -> Result<(), serde_json::Error> {
    store.persist().save(key, &serde_json::to_string(state)?);
    Ok(())
}

/// Read a compatible snapshot. An older version leaves the caller's default
/// layout in place. A malformed snapshot is returned as an error.
pub fn load(
    store: &LayoutStore,
    key: &str,
    version: usize,
) -> Result<Option<DockAreaState>, serde_json::Error> {
    let Some(json) = store.persist().load(key) else {
        return Ok(None);
    };
    let state: DockAreaState = serde_json::from_str(&json)?;
    Ok((state.version == Some(version)).then_some(state))
}

/// Persist layout changes while the returned subscription is held.
pub fn persist_changes(
    area: &Entity<DockArea>,
    key: String,
    on_error: impl Fn(serde_json::Error, &mut App) + 'static,
    cx: &mut App,
) -> Subscription {
    cx.subscribe(area, move |area, event: &DockEvent, cx| {
        if matches!(event, DockEvent::LayoutChanged)
            && let Some(store) = cx.try_global::<LayoutStore>()
            && let Err(error) = save(store, &key, &area.read(cx).dump(cx))
        {
            on_error(error, cx);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_store_round_trip_and_version_check() -> Result<(), serde_json::Error> {
        let store = LayoutStore::in_memory();
        let state = DockAreaState {
            version: Some(2),
            center: PanelState::new("editor"),
            ..Default::default()
        };
        assert_eq!(load(&store, "ide", 2)?, None);
        save(&store, "ide", &state)?;
        assert_eq!(load(&store, "ide", 1)?, None);
        assert_eq!(load(&store, "ide", 2)?, Some(state));
        store.persist().save("ide", "{");
        assert!(load(&store, "ide", 2).is_err());
        Ok(())
    }
}

pub(crate) mod document;
