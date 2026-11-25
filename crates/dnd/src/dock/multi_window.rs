//! Explicit window moves for application-owned panel instances.
use super::{DockArea, DockPlacement, Panel, panel_handle};
use gpui_kit::{
    AnyWindowHandle, App, Entity, SharedString, Subscription, Window, WindowOptions,
    component::dock::{InsertTarget, NodeId, PaneRef, PaneTree, PanelId},
};
use guiraffe_layout::LayoutStore;
use guiraffe_windows::{OpenMode, WindowKey, WindowManager};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

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

#[derive(Clone, Copy)]
struct ReturnSlot {
    region: DockPlacement,
    // Node ids belong to this live dock. A reconstructed or collapsed group falls
    // back to the persisted region's first group.
    tab: Option<(NodeId, usize)>,
}

impl ReturnSlot {
    fn locate(tree: &PaneTree, region: DockPlacement, panel: PanelId) -> Option<Self> {
        let node = tree.find_panel_node(panel)?;
        let PaneRef::Tabs { panels, .. } = tree.find_node(node)?.kind() else {
            return None;
        };
        let index = panels.iter().position(|id| *id == panel)?;
        Some(Self {
            region,
            tab: Some((node, index)),
        })
    }

    fn target(self, tree: &PaneTree) -> Option<InsertTarget> {
        let (node, index) = self.tab?;
        let PaneRef::Tabs { .. } = tree.find_node(node)?.kind() else {
            return None;
        };
        Some(InsertTarget::Tabs {
            node,
            ix: Some(index),
            activate: true,
        })
    }
}

/// Commands for one panel. Construct once with an application-wide stable instance key.
/// Reuse the same key and panel factory on relaunch. The key is the panel's control-id
/// scope across windows; the live entity and its descendants survive each move.
pub struct DockablePanel<P: Panel> {
    key: SharedString,
    area: Entity<DockArea>,
    panel: Entity<P>,
    home: AnyWindowHandle,
    close_subscription: RefCell<Option<Subscription>>,
    detached: Rc<Cell<Option<(AnyWindowHandle, ReturnSlot)>>>,
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
            close_subscription: RefCell::default(),
        }
    }

    /// Stable registration scope for the control protocol.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Open the panel in a new window and remember its home region.
    pub fn pop_out(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let id = PanelId::from(self.panel.entity_id());
        let Some(slot) = REGIONS.into_iter().find_map(|(region, _)| {
            ReturnSlot::locate(self.area.read(cx).layout(region)?, region, id)
        }) else {
            return Ok(false);
        };
        self.open(slot, window, cx)
    }

    /// After constructing the default panel, reopen its saved detached window, if any.
    /// Install a durable LayoutStore before constructing the dock for cross-launch restore.
    /// Configure WindowManager with a layout path to also restore display and window bounds.
    pub fn restore(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let region = cx
            .global::<LayoutStore>()
            .persist()
            .load(&self.key)
            .as_deref()
            .and_then(stored_region);
        match region {
            Some(region) => self.open(ReturnSlot { region, tab: None }, window, cx),
            None => Ok(false),
        }
    }

    fn open(&self, slot: ReturnSlot, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        if self.detached.get().is_some() || window.window_handle() != self.home {
            return Ok(false);
        }
        let panel = self.panel.clone();
        let area = self.area.clone();
        let detached = self.detached.clone();
        let key = self.key.clone();
        let home = self.home;
        let window_key = WindowKey::instance("dock-panel", key.clone());
        if WindowManager::find(&window_key, cx).is_some() {
            return Ok(false);
        }
        let handle = WindowManager::open(
            window_key,
            OpenMode::New,
            WindowOptions::default(),
            |_, _| panel.clone(),
            cx,
        )?;
        let child: AnyWindowHandle = handle.into();
        let subscription = cx.on_window_closed(move |cx, id| {
            if id == child.window_id() && detached.get().is_some_and(|(active, _)| active == child)
            {
                detached.set(None);
                let (area, panel, key) = (area.clone(), panel.clone(), key.clone());
                cx.defer(move |cx| {
                    let _ = home.update(cx, |_, window, cx| {
                        attach(&area, &panel, slot, window, cx);
                        cx.global::<LayoutStore>().persist().save(&key, "docked");
                    });
                });
            }
        });
        self.close_subscription.replace(Some(subscription));
        self.area.update(cx, |area, cx| {
            area.remove_panel(self.panel.clone(), window, cx)
        });
        self.detached.set(Some((handle.into(), slot)));
        if let Some((_, name)) = REGIONS.iter().find(|(r, _)| *r == slot.region) {
            cx.global::<LayoutStore>().persist().save(&self.key, name);
        }
        Ok(true)
    }

    /// Command suitable for a keybinding or action bus in the home window.
    pub fn dock_back(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let Some((child, slot)) = self.detached.get() else {
            return Ok(false);
        };
        if window.window_handle() != self.home {
            return Ok(false);
        }
        // The explicit command owns reattachment. The close observer handles native closes.
        self.detached.set(None);
        if !WindowManager::request_close(child, cx) {
            self.detached.set(Some((child, slot)));
            return Ok(false);
        }
        self.close_subscription.take();
        attach(&self.area, &self.panel, slot, window, cx);
        cx.global::<LayoutStore>()
            .persist()
            .save(&self.key, "docked");
        Ok(true)
    }
}

fn attach<P: Panel>(
    area: &Entity<DockArea>,
    panel: &Entity<P>,
    slot: ReturnSlot,
    window: &mut Window,
    cx: &mut App,
) {
    area.update(cx, |area, cx| {
        area.add_panel_view(panel_handle(panel.clone()), slot.region, None, window, cx);
        if let Some(target) = area.layout(slot.region).and_then(|tree| slot.target(tree)) {
            area.move_panel(PanelId::from(panel.entity_id()), target, window, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn returns_to_saved_tab_and_falls_back_when_group_is_removed() {
        use gpui_kit::{base::Placement, component::dock::RootKind};
        let mut tree = PaneTree::new(RootKind::Any);
        let first = PanelId::from_u64(1);
        let second = PanelId::from_u64(2);
        tree.split(tree.root().id(), first, Placement::Right, None);
        let group = tree.find_panel_node(first).unwrap();
        tree.insert_panel(
            second,
            InsertTarget::Tabs {
                node: group,
                ix: None,
                activate: true,
            },
        );
        let slot = ReturnSlot::locate(&tree, DockPlacement::Right, first).unwrap();
        tree.remove_panel(first);
        tree.insert_panel(first, slot.target(&tree).unwrap());
        assert_eq!(tree.panels().collect::<Vec<_>>(), [first, second]);
        tree.remove_panel(first);
        tree.remove_panel(second);
        assert!(slot.target(&tree).is_none());
    }

    #[test]
    fn restore_only_recognizes_detached_regions() {
        for (region, key) in REGIONS {
            assert_eq!(stored_region(key), Some(region));
        }
        assert_eq!(stored_region("docked"), None);
        assert_eq!(stored_region("unknown"), None);
    }
}
