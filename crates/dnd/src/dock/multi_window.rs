//! Explicit window moves for application-owned panel instances.
use super::{DockArea, DockPlacement, Panel, panel_handle};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Entity, SharedString, Window, WindowOptions,
    component::dock::PanelId,
};
use guiraffe_layout::LayoutStore;
use std::{cell::Cell, rc::Rc};

const REGIONS: [(DockPlacement, &str); 4] = [
    (DockPlacement::Center, "center"),
    (DockPlacement::Left, "left"),
    (DockPlacement::Right, "right"),
    (DockPlacement::Bottom, "bottom"),
];

fn stored_region(value: &str) -> Option<DockPlacement> {
    REGIONS
        .iter()
        .find(|(_, name)| *name == value)
        .map(|(region, _)| *region)
}

/// Commands for one panel. Construct once with an application-wide stable instance key.
/// Reuse the same key and panel factory on relaunch. The key is the panel's control-id
/// scope across windows; the live entity and its descendants survive each move.
pub struct DockablePanel<P: Panel> {
    key: SharedString,
    area: Entity<DockArea>,
    panel: Entity<P>,
    home: AnyWindowHandle,
    detached: Rc<Cell<Option<(AnyWindowHandle, DockPlacement)>>>,
}

impl<P: Panel> DockablePanel<P> {
    /// Bind an existing panel to its home dock. Use a dedicated layout persistence key.
    pub fn new(
        key: impl Into<SharedString>,
        area: Entity<DockArea>,
        panel: Entity<P>,
        window: &Window,
    ) -> Self {
        Self {
            key: key.into(),
            area,
            panel,
            home: window.window_handle(),
            detached: Rc::default(),
        }
    }

    /// Stable registration scope for the control protocol.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Open the panel in a new window and remember its home region.
    pub fn pop_out(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let id = PanelId::from(self.panel.entity_id());
        let Some((region, _)) = REGIONS.into_iter().find(|(r, _)| {
            self.area
                .read(cx)
                .layout(*r)
                .is_some_and(|tree| tree.find_panel_node(id).is_some())
        }) else {
            return Ok(false);
        };
        self.open(region, window, cx)
    }

    /// After constructing the default panel, reopen its saved detached window, if any.
    /// Install a durable LayoutStore before constructing the dock for cross-launch restore.
    pub fn restore(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let region = cx
            .global::<LayoutStore>()
            .persist()
            .load(&self.key)
            .as_deref()
            .and_then(stored_region);
        match region {
            Some(region) => self.open(region, window, cx),
            None => Ok(false),
        }
    }

    fn open(
        &self,
        region: DockPlacement,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<bool> {
        if self.detached.get().is_some() || window.window_handle() != self.home {
            return Ok(false);
        }
        let panel = self.panel.clone();
        let area = self.area.clone();
        let detached = self.detached.clone();
        let key = self.key.clone();
        let home = self.home;
        let handle = cx.open_window(WindowOptions::default(), |window, cx| {
            let content = panel.clone();
            window.on_window_should_close(cx, move |_, cx| {
                home.update(cx, |_, window, cx| {
                    attach(&area, &panel, region, window, cx);
                    detached.set(None);
                    cx.global::<LayoutStore>().persist().save(&key, "docked");
                })
                .is_ok()
                    || !cx.windows().contains(&home)
            });
            content
        })?;
        self.area.update(cx, |area, cx| {
            area.remove_panel(self.panel.clone(), window, cx)
        });
        self.detached.set(Some((handle.into(), region)));
        if let Some((_, name)) = REGIONS.iter().find(|(r, _)| *r == region) {
            cx.global::<LayoutStore>().persist().save(&self.key, name);
        }
        Ok(true)
    }

    /// Command suitable for a keybinding or action bus in the home window.
    pub fn dock_back(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let Some((child, region)) = self.detached.get() else {
            return Ok(false);
        };
        if window.window_handle() != self.home {
            return Ok(false);
        }
        child.update(cx, |_, window, _| window.remove_window())?;
        attach(&self.area, &self.panel, region, window, cx);
        self.detached.set(None);
        cx.global::<LayoutStore>()
            .persist()
            .save(&self.key, "docked");
        Ok(true)
    }
}

fn attach<P: Panel>(
    area: &Entity<DockArea>,
    panel: &Entity<P>,
    region: DockPlacement,
    window: &mut Window,
    cx: &mut App,
) {
    area.update(cx, |area, cx| {
        area.add_panel_view(panel_handle(panel.clone()), region, None, window, cx)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_only_recognizes_detached_regions() {
        for (region, key) in REGIONS {
            assert_eq!(stored_region(key), Some(region));
        }
        assert_eq!(stored_region("docked"), None);
        assert_eq!(stored_region("unknown"), None);
    }
}
