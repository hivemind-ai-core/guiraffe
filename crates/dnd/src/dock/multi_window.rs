//! Explicit window moves for application-owned panel instances.
use super::{DockArea, DockPlacement, Panel, panel_handle};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, Entity, Focusable, SharedString, Subscription,
    WeakEntity, Window, WindowOptions,
    component::dock::{InsertTarget, NodeId, PaneRef, PaneTree, PanelId},
};
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe, names,
};
use guiraffe_layout::LayoutStore;
use guiraffe_windows::{OpenMode, WindowKey, WindowManager};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const KIND: &str = "dock-panel";

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
/// path, and the control follows the panel between windows. The same live entity
/// becomes the detached window's root. Commands work from any window: outside the home
/// window they are deferred to it. Dropping the controller while detached docks the
/// panel back (or closes its window when the home window is gone), so no window is
/// ever orphaned.
///
/// The control accepts the shared `open` (pop out) and `close` (dock back) actions on
/// the [`ActionBus`], so ribbons, shortcuts and scripts can drive it.
///
/// ```
/// use gpui_kit::{App, Entity, Window};
/// use guiraffe_dnd::dock::{DockArea, DockablePanel, Panel};
///
/// fn mount_inspector<P: Panel>(
///     dock: Entity<DockArea>, panel: Entity<P>, window: &mut Window, cx: &mut App,
/// ) -> anyhow::Result<DockablePanel<P>> {
///     let inspector = DockablePanel::new("workspace/inspector", dock, panel, window, cx);
///     inspector.restore(window, cx)?;
///     Ok(inspector)
/// }
/// ```
pub struct DockablePanel<P: Panel> {
    inner: Rc<Inner<P>>,
    _control: Entity<DockControl<P>>,
}

struct Inner<P: Panel> {
    key: SharedString,
    id: Cell<Option<ControlId>>,
    area: Entity<DockArea>,
    panel: Entity<P>,
    home: AnyWindowHandle,
    close_subscription: RefCell<Option<Subscription>>,
    detached: Cell<Option<(AnyWindowHandle, ReturnSlot)>>,
    control: RefCell<Option<WeakEntity<DockControl<P>>>>,
}

/// Bus endpoint for a [`DockablePanel`]. Its release docks a detached panel back.
pub struct DockControl<P: Panel> {
    inner: Rc<Inner<P>>,
}

/// Bus actions of a dockable panel's control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAction {
    /// Shared `open`: move the panel to its own window.
    PopOut,
    /// Shared `close`: return the panel to its home dock.
    DockBack,
}

impl<P: Panel> Observe for DockControl<P> {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            open: Some(self.inner.detached.get().is_some()),
            label: Some(self.inner.key.as_ref().into()),
            ..ControlSnapshot::default()
        }
    }
}

impl<P: Panel> Drive for DockControl<P> {
    type Action = DockAction;
    type Output = ();
    fn decode(action: &Action) -> Result<DockAction, ActionError> {
        match action.name.as_ref() {
            names::OPEN => Ok(DockAction::PopOut),
            names::CLOSE => Ok(DockAction::DockBack),
            _ => Err(ActionError::Unsupported),
        }
    }
    fn apply(
        &mut self,
        action: DockAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ActionError> {
        let result = match action {
            DockAction::PopOut => self.inner.pop_out(window, cx),
            DockAction::DockBack => self.inner.dock_back(window, cx),
        };
        match result {
            Ok(true) => Ok(()),
            Ok(false) => Err(ActionError::Disabled),
            Err(_) => Err(ActionError::Invalid("dock window could not be opened")),
        }
    }
}

impl<P: Panel> DockablePanel<P> {
    /// Bind an existing panel to its home dock and register its control there.
    /// Use a dedicated layout persistence key.
    pub fn new(
        key: impl Into<SharedString>,
        area: Entity<DockArea>,
        panel: Entity<P>,
        window: &Window,
        cx: &mut App,
    ) -> Self {
        let inner = Rc::new(Inner {
            key: key.into(),
            id: Cell::default(),
            area,
            panel,
            home: window.window_handle(),
            detached: Cell::default(),
            close_subscription: RefCell::default(),
            control: RefCell::default(),
        });
        let control = cx.new(|_| DockControl {
            inner: inner.clone(),
        });
        cx.observe_release(&control, |control: &mut DockControl<P>, cx| {
            control.inner.release(cx);
        })
        .detach();
        inner.control.replace(Some(control.downgrade()));
        inner.id.set(Some(ActionBus::register(
            &inner.key,
            &control,
            KIND,
            Capabilities::OPEN | Capabilities::CLOSE,
            window,
            cx,
        )));
        Self {
            inner,
            _control: control,
        }
    }

    /// Stable registration path for the control protocol.
    pub fn key(&self) -> &str {
        &self.inner.key
    }

    /// The panel's control id. It stays the same while the panel moves between windows.
    pub fn control_id(&self) -> Option<ControlId> {
        self.inner.id.get()
    }

    /// Whether the panel currently lives in its own window.
    pub fn is_detached(&self) -> bool {
        self.inner.detached.get().is_some()
    }

    /// Open the panel in a new window and remember its home slot. From another window
    /// the move is deferred to the home window and `Ok(true)` means it was scheduled.
    pub fn pop_out(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        self.inner.pop_out(window, cx)
    }

    /// After constructing the default panel, reopen its saved detached window, if any.
    /// Install a durable LayoutStore before constructing the dock for cross-launch restore.
    /// Configure WindowManager with a layout path to also restore display and window bounds.
    pub fn restore(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let region = cx
            .global::<LayoutStore>()
            .persist()
            .load(&self.inner.key)
            .as_deref()
            .and_then(stored_region);
        match region {
            Some(region) if window.window_handle() == self.inner.home => {
                self.inner
                    .open(ReturnSlot { region, tab: None }, window, cx)
            }
            _ => Ok(false),
        }
    }

    /// Return the panel to its home dock. Works from any window, including a button
    /// inside the detached window: there the call is deferred to the home window and
    /// `Ok(true)` means it was scheduled. Close guards on the detached window can veto.
    pub fn dock_back(&self, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        self.inner.dock_back(window, cx)
    }
}

impl<P: Panel> Inner<P> {
    fn in_home(
        self: &Rc<Self>,
        window: &Window,
        cx: &mut App,
        op: fn(&Rc<Self>, &mut Window, &mut App) -> anyhow::Result<bool>,
    ) -> Option<anyhow::Result<bool>> {
        if window.window_handle() == self.home {
            return None;
        }
        let this = self.clone();
        cx.defer(move |cx| {
            let _ = this.home.update(cx, |_, window, cx| op(&this, window, cx));
        });
        Some(Ok(true))
    }

    fn pop_out(self: &Rc<Self>, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        if self.detached.get().is_some() {
            return Ok(false);
        }
        if let Some(scheduled) = self.in_home(window, cx, Self::pop_out) {
            return scheduled;
        }
        let id = PanelId::from(self.panel.entity_id());
        let Some(slot) = REGIONS.into_iter().find_map(|(region, _)| {
            ReturnSlot::locate(self.area.read(cx).layout(region)?, region, id)
        }) else {
            return Ok(false);
        };
        self.open(slot, window, cx)
    }

    fn open(
        self: &Rc<Self>,
        slot: ReturnSlot,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<bool> {
        if self.detached.get().is_some() {
            return Ok(false);
        }
        let window_key = WindowKey::instance("dock-panel", self.key.clone());
        if WindowManager::find(&window_key, cx).is_some() {
            return Ok(false);
        }
        let panel = self.panel.clone();
        let handle = WindowManager::open(
            window_key,
            OpenMode::New,
            WindowOptions::default(),
            |_, _| panel,
            cx,
        )?;
        let child: AnyWindowHandle = handle.into();
        let weak = Rc::downgrade(self);
        let subscription = cx.on_window_closed(move |cx, id| {
            let Some(this) = weak.upgrade() else { return };
            if id == child.window_id()
                && this
                    .detached
                    .get()
                    .is_some_and(|(active, _)| active == child)
            {
                this.detached.set(None);
                cx.defer(move |cx| {
                    let _ = this.home.update(cx, |_, window, cx| {
                        this.reattach(slot, window, cx);
                    });
                });
            }
        });
        self.close_subscription.replace(Some(subscription));
        self.area.update(cx, |area, cx| {
            area.remove_panel(self.panel.clone(), window, cx)
        });
        self.detached.set(Some((child, slot)));
        if let Some((_, name)) = REGIONS.iter().find(|(r, _)| *r == slot.region) {
            cx.global::<LayoutStore>().persist().save(&self.key, name);
        }
        self.rebind(self.home, child, cx);
        Ok(true)
    }

    fn dock_back(self: &Rc<Self>, window: &mut Window, cx: &mut App) -> anyhow::Result<bool> {
        let Some((child, slot)) = self.detached.get() else {
            return Ok(false);
        };
        if let Some(scheduled) = self.in_home(window, cx, Self::dock_back) {
            return scheduled;
        }
        // The explicit command owns reattachment. The close observer handles native closes.
        self.detached.set(None);
        // A child the platform already destroyed cannot veto; reattach without closing it.
        if alive(child, cx) && !WindowManager::request_close(child, cx) {
            self.detached.set(Some((child, slot)));
            return Ok(false);
        }
        self.close_subscription.take();
        self.reattach(slot, window, cx);
        Ok(true)
    }

    /// Home-window side of every return path.
    fn reattach(self: &Rc<Self>, slot: ReturnSlot, window: &mut Window, cx: &mut App) {
        attach(&self.area, &self.panel, slot, window, cx);
        cx.global::<LayoutStore>()
            .persist()
            .save(&self.key, "docked");
        // The closed child's bus already dropped the control; it answers at home again.
        let this = self.clone();
        cx.defer(move |cx| this.register_in(this.home, cx));
    }

    /// Move the control's registration so the same path and id answer in the panel's window.
    fn rebind(self: &Rc<Self>, from: AnyWindowHandle, to: AnyWindowHandle, cx: &mut App) {
        let this = self.clone();
        cx.defer(move |cx| {
            let (Some(id), Some(control)) = (this.id.get(), this.control_entity()) else {
                return;
            };
            let _ = from.update(cx, |_, window, cx| {
                ActionBus::unregister(id, &control, window, cx);
            });
            this.register_in(to, cx);
        });
    }

    fn register_in(&self, target: AnyWindowHandle, cx: &mut App) {
        let Some(control) = self.control_entity() else {
            return;
        };
        let _ = target.update(cx, |_, window, cx| {
            if let Some(id) = self.id.get()
                && ActionBus::describe(id, window, cx).is_some()
            {
                return;
            }
            let id = ActionBus::register(
                &self.key,
                &control,
                KIND,
                Capabilities::OPEN | Capabilities::CLOSE,
                window,
                cx,
            );
            self.id.set(Some(id));
        });
    }

    fn control_entity(&self) -> Option<Entity<DockControl<P>>> {
        self.control.borrow().as_ref().and_then(WeakEntity::upgrade)
    }

    /// The controller is gone. Never leave a detached window without an owner.
    fn release(self: &Rc<Self>, cx: &mut App) {
        let Some((child, slot)) = self.detached.take() else {
            return;
        };
        self.close_subscription.take();
        let this = self.clone();
        cx.defer(move |cx| {
            let _ = this.home.update(cx, |_, window, cx| {
                attach(&this.area, &this.panel, slot, window, cx);
                cx.global::<LayoutStore>()
                    .persist()
                    .save(&this.key, "docked");
            });
            // Close without guards: the panel already lives in the home dock, or the
            // app is shutting down and the saved region restores it on relaunch.
            let _ = child.update(cx, |_, window, _| window.remove_window());
        });
    }
}

fn alive(handle: AnyWindowHandle, cx: &App) -> bool {
    cx.windows()
        .iter()
        .any(|w| w.window_id() == handle.window_id())
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
    // Focus follows the panel home, so keyboard commands keep targeting it.
    let focus = panel.focus_handle(cx);
    window.focus(&focus, cx);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions")]
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
        // A sibling keeps the tree alive, so emptying the group collapses it away.
        let third = PanelId::from_u64(3);
        tree.split(tree.root().id(), third, Placement::Left, None);
        tree.remove_panel(first);
        tree.remove_panel(second);
        assert!(tree.find_node(group).is_none());
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
