use crate::board::{DropOutline, Preview};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Axis, Bounds, ClickEvent, Context, CursorStyle, DispatchPhase,
    Entity, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, Role, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
    base::{ElementExt, Placement, TestSupportExt},
    canvas,
    component::dock::{
        DockArea, DockAreaRenderer, DockContext, DockPlacement, DropPlaceholderBounds,
        InsertTarget, NodeId, PaneNode, PaneRef, PanelHandle, PanelId, TabGroupContext,
        TabGroupRenderer,
    },
    div, point, px, size,
};
use guiraffe_core::GuiraffeTheme;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ZoneKind {
    Tab(NodeId, usize),
    Content(NodeId),
    Frame(NodeId),
}

struct Zone {
    kind: ZoneKind,
    bounds: Cell<Bounds<Pixels>>,
}

type Zones = Rc<RefCell<HashMap<ZoneKind, Rc<Zone>>>>;

#[derive(Clone)]
struct SplitResize {
    node: NodeId,
    axis: Axis,
    before: usize,
    sizes: Vec<Pixels>,
    handle: Bounds<Pixels>,
}

impl SplitResize {
    fn displacement(&self, delta: Point<Pixels>) -> Pixels {
        let amount = match self.axis {
            Axis::Horizontal => delta.x,
            Axis::Vertical => delta.y,
        };
        // The two slots keep their total size and a usable minimum.
        let minimum = px(48.).min(self.sizes[self.before].min(self.sizes[self.before + 1]));
        amount.clamp(
            minimum - self.sizes[self.before],
            self.sizes[self.before + 1] - minimum,
        )
    }

    fn sizes_at(&self, delta: Point<Pixels>) -> Vec<Pixels> {
        let amount = self.displacement(delta);
        let mut sizes = self.sizes.clone();
        sizes[self.before] += amount;
        sizes[self.before + 1] -= amount;
        sizes
    }

    fn outline_at(&self, delta: Point<Pixels>) -> Bounds<Pixels> {
        let amount = self.displacement(delta);
        let offset = match self.axis {
            Axis::Horizontal => point(amount, px(0.)),
            Axis::Vertical => point(px(0.), amount),
        };
        Bounds::new(self.handle.origin + offset, self.handle.size)
    }
}

enum Gesture {
    Tab {
        panel: PanelId,
        source: NodeId,
        index: usize,
        origin: Point<Pixels>,
        active: bool,
        preview: Entity<Preview>,
    },
    Split {
        resize: SplitResize,
        origin: Point<Pixels>,
    },
    Side {
        dock: DockContext,
        origin: Point<Pixels>,
        outline: Bounds<Pixels>,
    },
}

/// Mount beside the cached dock host. Pointer moves update only the preview and outline.
pub struct DockDragOverlay {
    area: WeakEntity<DockArea>,
    zones: Zones,
    gesture: Option<Gesture>,
    outline: Entity<DropOutline>,
    origin: Rc<Cell<Point<Pixels>>>,
    escape: Option<Subscription>,
}

impl DockDragOverlay {
    fn outline(&self, bounds: Option<Bounds<Pixels>>, cx: &mut App) {
        if self.outline.read(cx).bounds != bounds {
            self.outline.update(cx, |outline, cx| {
                outline.bounds = bounds;
                cx.notify();
            });
        }
    }

    fn begin(&mut self, gesture: Gesture, window: &Window, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let window_id = window.window_handle().window_id();
        self.escape = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if event.keystroke.key == "escape"
                && window.window_handle().window_id() == window_id
                && weak
                    .update(cx, |overlay, cx| overlay.cancel(cx).is_some())
                    .unwrap_or(false)
            {
                cx.stop_propagation();
            }
        }));
        self.gesture = Some(gesture);
    }

    fn cancel(&mut self, cx: &mut Context<Self>) -> Option<Gesture> {
        let finished = self.gesture.take()?;
        self.escape = None;
        self.outline(None, cx);
        cx.notify();
        Some(finished)
    }

    fn resolve(
        &self,
        pointer: Point<Pixels>,
        source: NodeId,
    ) -> Option<(InsertTarget, Bounds<Pixels>)> {
        self.zones.borrow().values().find_map(|zone| {
            let bounds = zone.bounds.get();
            if bounds.size.width <= px(0.)
                || bounds.size.height <= px(0.)
                || !bounds.contains(&pointer)
            {
                return None;
            }
            match zone.kind {
                ZoneKind::Tab(node, index) => {
                    let after = pointer.x >= bounds.center().x;
                    let edge = if after { bounds.right() } else { bounds.left() };
                    let marker = Bounds::new(
                        point(edge - px(1.), bounds.top()),
                        size(px(2.), bounds.size.height),
                    );
                    Some((
                        InsertTarget::Tabs {
                            node,
                            ix: Some(index + usize::from(after)),
                            activate: true,
                        },
                        marker,
                    ))
                }
                ZoneKind::Content(node) => {
                    let placement = split_edge(bounds, pointer);
                    let target = match placement {
                        Some(placement) => InsertTarget::Split {
                            node,
                            placement,
                            size: None,
                        },
                        None if source == node => return None,
                        None => InsertTarget::Tabs {
                            node,
                            ix: None,
                            activate: true,
                        },
                    };
                    let placeholder = DropPlaceholderBounds::for_placement(bounds, placement);
                    Some((
                        target,
                        Bounds::new(bounds.origin + placeholder.origin(), placeholder.size()),
                    ))
                }
                ZoneKind::Frame(_) => None,
            }
        })
    }

    fn frame_bounds(&self, node: NodeId) -> Option<Bounds<Pixels>> {
        self.zones.borrow().values().find_map(|zone| {
            matches!(zone.kind, ZoneKind::Frame(id) if id == node).then(|| zone.bounds.get())
        })
    }

    fn split_handle(&self, pointer: Point<Pixels>, cx: &App) -> Option<SplitResize> {
        let area = self.area.upgrade()?;
        let area = area.read(cx);
        if area.is_locked() {
            return None;
        }
        for location in [
            DockPlacement::Center,
            DockPlacement::Left,
            DockPlacement::Right,
            DockPlacement::Bottom,
        ] {
            let Some(tree) = area.layout(location) else {
                continue;
            };
            let mut result = None;
            tree.root().walk(&mut |node| {
                if let Some(handle) = self.node_handle(node, pointer) {
                    result = Some(handle);
                }
            });
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn node_handle(&self, node: &PaneNode, pointer: Point<Pixels>) -> Option<SplitResize> {
        let PaneRef::Split { axis, children, .. } = node.kind() else {
            return None;
        };
        let frame = self.frame_bounds(node.id())?;
        if !frame.contains(&pointer) {
            return None;
        }
        let children = children
            .iter()
            .map(|child| self.frame_bounds(child.id()))
            .collect::<Option<Vec<_>>>()?;
        let sizes: Vec<_> = children
            .iter()
            .map(|child| match axis {
                Axis::Horizontal => child.size.width,
                Axis::Vertical => child.size.height,
            })
            .collect();
        let (before, handle) = children
            .iter()
            .take(children.len().saturating_sub(1))
            .enumerate()
            .find_map(|(index, child)| {
                let handle = match axis {
                    Axis::Horizontal => Bounds::new(
                        point(child.right() - px(2.), frame.top()),
                        size(px(4.), frame.size.height),
                    ),
                    Axis::Vertical => Bounds::new(
                        point(frame.left(), child.bottom() - px(2.)),
                        size(frame.size.width, px(4.)),
                    ),
                };
                handle.contains(&pointer).then_some((index, handle))
            })?;
        Some(SplitResize {
            node: node.id(),
            axis,
            before,
            sizes,
            handle,
        })
    }

    fn moved(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.cancel(cx);
            return;
        }
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let next = match gesture {
            Gesture::Tab {
                panel,
                source,
                index,
                origin,
                mut active,
                preview,
            } => {
                let delta = event.position - origin;
                if active || delta.x.abs().max(delta.y.abs()) >= px(5.) {
                    let started = !active;
                    active = true;
                    preview.update(cx, |preview, cx| {
                        preview.position = event.position;
                        cx.notify();
                    });
                    self.outline(
                        self.resolve(event.position, source)
                            .map(|(_, bounds)| bounds),
                        cx,
                    );
                    if started {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
                Gesture::Tab {
                    panel,
                    source,
                    index,
                    origin,
                    active,
                    preview,
                }
            }
            Gesture::Split { resize, origin } => {
                self.outline(Some(resize.outline_at(event.position - origin)), cx);
                cx.stop_propagation();
                Gesture::Split { resize, origin }
            }
            Gesture::Side {
                dock,
                origin,
                outline,
            } => {
                let delta = event.position - origin;
                let delta = match dock.placement().axis() {
                    Axis::Horizontal => point(delta.x, px(0.)),
                    Axis::Vertical => point(px(0.), delta.y),
                };
                self.outline(Some(Bounds::new(outline.origin + delta, outline.size)), cx);
                cx.stop_propagation();
                Gesture::Side {
                    dock,
                    origin,
                    outline,
                }
            }
        };
        self.gesture = Some(next);
    }

    fn released(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(gesture) = self.cancel(cx) else {
            return;
        };
        match gesture {
            Gesture::Tab {
                panel,
                source,
                index,
                active,
                ..
            } => {
                if !active {
                    return;
                }
                if let Some((target, _)) = self.resolve(event.position, source)
                    && let Some(area) = self.area.upgrade()
                {
                    let target = match target {
                        InsertTarget::Tabs {
                            node,
                            ix: Some(slot),
                            activate,
                        } if node == source => {
                            let destination = slot - usize::from(slot > index);
                            if destination == index {
                                return;
                            }
                            InsertTarget::Tabs {
                                node,
                                ix: Some(destination),
                                activate,
                            }
                        }
                        target => target,
                    };
                    area.update(cx, |area, cx| area.move_panel(panel, target, window, cx));
                }
            }
            Gesture::Split { resize, origin } => {
                if let Some(area) = self.area.upgrade()
                    && origin != event.position
                {
                    area.update(cx, |area, cx| {
                        area.set_split_sizes(
                            resize.node,
                            resize.sizes_at(event.position - origin),
                            window,
                            cx,
                        )
                    });
                }
            }
            Gesture::Side { dock, .. } => {
                dock.resize_to(event.position, window, cx);
                dock.end_resize(window, cx);
            }
        }
        cx.stop_propagation();
    }
}

fn split_edge(bounds: Bounds<Pixels>, pointer: Point<Pixels>) -> Option<Placement> {
    if pointer.x < bounds.left() + bounds.size.width * 0.25 {
        Some(Placement::Left)
    } else if pointer.x > bounds.right() - bounds.size.width * 0.25 {
        Some(Placement::Right)
    } else if pointer.y < bounds.top() + bounds.size.height * 0.25 {
        Some(Placement::Top)
    } else if pointer.y > bounds.bottom() - bounds.size.height * 0.25 {
        Some(Placement::Bottom)
    } else {
        None
    }
}

impl Render for DockDragOverlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let down = cx.weak_entity();
        let movement = down.clone();
        let release = down.clone();
        let origin = self.origin.clone();
        let preview = match &self.gesture {
            Some(Gesture::Tab {
                preview,
                active: true,
                ..
            }) => Some(preview.clone()),
            _ => None,
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                canvas(
                    move |bounds, _, _| origin.set(bounds.origin),
                    move |_, _, window, _| {
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = down.update(cx, |overlay, cx| {
                                    if let Some(resize) = overlay.split_handle(event.position, cx) {
                                        overlay.begin(
                                            Gesture::Split {
                                                resize,
                                                origin: event.position,
                                            },
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = movement.update(cx, |overlay, cx| overlay.moved(event, cx));
                            }
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = release
                                    .update(cx, |overlay, cx| overlay.released(event, window, cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(self.outline.clone())
            .children(preview)
    }
}

#[derive(Clone)]
struct Renderer {
    overlay: WeakEntity<DockDragOverlay>,
    area: WeakEntity<DockArea>,
    zones: Zones,
}

impl Renderer {
    fn zone(&self, kind: ZoneKind) -> Rc<Zone> {
        let zone = Rc::new(Zone {
            kind,
            bounds: Cell::default(),
        });
        let mut zones = self.zones.borrow_mut();
        zones.insert(kind, zone.clone());
        zone
    }
}

fn measure(zone: Rc<Zone>) -> impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) {
    move |bounds, window, _| {
        zone.bounds
            .set(bounds.intersect(&window.content_mask().bounds))
    }
}

impl DockAreaRenderer for Renderer {
    fn frame(&self, _: &mut Window, _: &mut App) -> Stateful<gpui_kit::Div> {
        self.zones.borrow_mut().clear();
        div().id("managed-dock")
    }

    fn split_frame(
        &self,
        node: NodeId,
        _: Axis,
        _: &mut Window,
        _: &mut App,
    ) -> Stateful<gpui_kit::Div> {
        div()
            .id(("dock-split", node.as_u64()))
            .on_prepaint(measure(self.zone(ZoneKind::Frame(node))))
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        _: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let border = GuiraffeTheme::global(cx).surfaces().border.to_hsla();
        let dock = dock.clone();
        let placement = dock.placement();
        let overlay = self.overlay.clone();
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let grip = div()
            .id(("dock-side-resize", placement as usize))
            .absolute()
            .when(placement == DockPlacement::Left, |el| {
                el.right_0()
                    .top_0()
                    .h_full()
                    .w(px(4.))
                    .cursor(CursorStyle::ResizeLeftRight)
            })
            .when(placement == DockPlacement::Right, |el| {
                el.left_0()
                    .top_0()
                    .h_full()
                    .w(px(4.))
                    .cursor(CursorStyle::ResizeLeftRight)
            })
            .when(placement == DockPlacement::Bottom, |el| {
                el.left_0()
                    .top_0()
                    .w_full()
                    .h(px(4.))
                    .cursor(CursorStyle::ResizeUpDown)
            })
            .on_prepaint({
                let bounds = bounds.clone();
                move |value, _, _| bounds.set(value)
            })
            .on_mouse_down(MouseButton::Left, {
                let dock = dock.clone();
                move |event, window, cx| {
                    let _ = overlay.update(cx, |overlay, cx| {
                        overlay.begin(
                            Gesture::Side {
                                dock: dock.clone(),
                                origin: event.position,
                                outline: bounds.get(),
                            },
                            window,
                            cx,
                        )
                    });
                    cx.stop_propagation();
                }
            });
        div()
            .size_full()
            .relative()
            .border_1()
            .border_color(border)
            .child(content)
            .children(dock.is_open().then_some(grip))
            .into_any_element()
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(self.clone())
    }
}

impl TabGroupRenderer for Renderer {
    fn frame(
        &self,
        group: &TabGroupContext,
        _: &mut Window,
        _: &mut App,
    ) -> Stateful<gpui_kit::Div> {
        div()
            .id(("dock-group", group.node().as_u64()))
            .on_prepaint(measure(self.zone(ZoneKind::Frame(group.node()))))
    }

    fn content_frame(
        &self,
        group: &TabGroupContext,
        _: &mut Window,
        _: &mut App,
    ) -> Stateful<gpui_kit::Div> {
        div()
            .id(("dock-content", group.node().as_u64()))
            .when(group.is_droppable(), |el| {
                el.on_prepaint(measure(self.zone(ZoneKind::Content(group.node()))))
            })
    }

    fn render_tab_bar(&self, group: &TabGroupContext, _: &mut Window, cx: &mut App) -> AnyElement {
        let theme = GuiraffeTheme::global(cx);
        let fill = theme.surfaces().surface.to_hsla();
        let border = theme.surfaces().border.to_hsla();
        let tabs = group
            .panels()
            .iter()
            .enumerate()
            .filter(|(_, panel)| panel.visible(cx));
        let mut bar = div()
            .flex()
            .items_center()
            .h(px(30.))
            .flex_shrink_0()
            .overflow_hidden()
            .bg(fill)
            .border_b_1()
            .border_color(border);
        for (index, panel) in tabs {
            let id = panel.panel_id(cx);
            let label = PanelHandle::of(panel)
                .and_then(|panel| panel.tab_name(cx))
                .unwrap_or_else(|| panel.panel_name(cx).into());
            let active = group
                .active_panel()
                .is_some_and(|panel| panel.panel_id(cx) == id);
            let node = group.node();
            let mut tab = div().id(("dock-tab", id.as_u64())).test_support().role(Role::Tab)
                .aria_label(label.clone()).aria_selected(active).tab_stop(true)
                .aria_keyshortcuts("Alt+ArrowLeft Alt+ArrowRight Alt+Shift+ArrowLeft Alt+Shift+ArrowRight Alt+Shift+ArrowUp Alt+Shift+ArrowDown")
                .h_full().px_3().flex().items_center().border_r_1().border_color(border)
                .when(active, |el| el.bg(theme.surfaces().raised.to_hsla()))
                .child(label.clone());
            if group.is_droppable() {
                tab = tab.on_prepaint(measure(self.zone(ZoneKind::Tab(node, index))));
            }
            if group.is_draggable() {
                let overlay = self.overlay.clone();
                let area = self.area.clone();
                let count = group.panels().len();
                tab = tab
                    .cursor(CursorStyle::OpenHand)
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        let _ = overlay.update(cx, |overlay, cx| {
                            let preview = cx.new(|_| Preview {
                                label: label.clone(),
                                position: event.position,
                                origin: overlay.origin.clone(),
                            });
                            overlay.begin(
                                Gesture::Tab {
                                    panel: id,
                                    source: node,
                                    index,
                                    origin: event.position,
                                    active: false,
                                    preview,
                                },
                                window,
                                cx,
                            );
                        });
                    })
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        if !event.keystroke.modifiers.alt {
                            return;
                        }
                        let placement = match event.keystroke.key.as_str() {
                            "left" => Placement::Left,
                            "right" => Placement::Right,
                            "up" => Placement::Top,
                            "down" => Placement::Bottom,
                            _ => return,
                        };
                        let target = if event.keystroke.modifiers.shift {
                            InsertTarget::Split {
                                node,
                                placement,
                                size: None,
                            }
                        } else {
                            let ix = match placement {
                                Placement::Left if index > 0 => index - 1,
                                Placement::Right if index + 1 < count => index + 1,
                                _ => return,
                            };
                            InsertTarget::Tabs {
                                node,
                                ix: Some(ix),
                                activate: true,
                            }
                        };
                        let _ = area.update(cx, |area, cx| area.move_panel(id, target, window, cx));
                        cx.stop_propagation();
                    });
            }
            let group = group.clone();
            bar =
                bar.child(tab.on_click(move |_: &ClickEvent, window, cx| {
                    group.select_tab(index, window, cx)
                }));
        }
        bar.into_any_element()
    }
}

pub(super) fn new(
    id: SharedString,
    version: usize,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<DockArea>, Entity<DockDragOverlay>) {
    let zones = Zones::default();
    let origin = Rc::new(Cell::new(Point::default()));
    let outline = cx.new(|_| DropOutline {
        bounds: None,
        origin: origin.clone(),
    });
    let overlay = cx.new(|_| DockDragOverlay {
        area: WeakEntity::new_invalid(),
        zones: zones.clone(),
        gesture: None,
        outline,
        origin,
        escape: None,
    });
    let area = cx.new(|cx| {
        let renderer = Rc::new(Renderer {
            overlay: overlay.downgrade(),
            area: cx.weak_entity(),
            zones,
        });
        DockArea::new(id, Some(version), window, cx).with_renderer(renderer)
    });
    overlay.update(cx, |overlay, _| overlay.area = area.downgrade());
    (area, overlay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        EventEmitter, FocusHandle, Focusable, StyleRefinement, TestAppContext,
        component::dock::{BasePanel, DockLayout, Panel, PanelEvent, panel_handle},
        test::TestWindowExt,
    };

    struct TestPanel(FocusHandle, &'static str);
    impl BasePanel for TestPanel {
        fn panel_name(&self) -> &'static str {
            self.1
        }
    }
    impl Panel for TestPanel {}
    impl EventEmitter<PanelEvent> for TestPanel {}
    impl Focusable for TestPanel {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.0.clone()
        }
    }
    impl Render for TestPanel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.1)
        }
    }

    struct Host(Entity<DockArea>, Rc<Cell<usize>>);
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.1.set(self.1.get() + 1);
            div().size_full().child(self.0.clone())
        }
    }

    struct Root {
        area: Entity<DockArea>,
        host: Entity<Host>,
        overlay: Entity<DockDragOverlay>,
        panels: [PanelId; 3],
    }

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(
                    self.host
                        .clone()
                        .cached(StyleRefinement::default().w(px(720.)).h(px(420.))),
                )
                .child(
                    self.overlay
                        .clone()
                        .cached(StyleRefinement::default().absolute().size_full()),
                )
        }
    }

    fn fixture(window: &mut Window, cx: &mut Context<Root>, renders: Rc<Cell<usize>>) -> Root {
        let (area, overlay) = crate::dock::new_dock("test-dock", 1, window, cx);
        let views = ["Alpha", "Bravo", "Charlie"]
            .map(|name| cx.new(|cx| TestPanel(cx.focus_handle(), name)));
        let panels = views.each_ref().map(|view| PanelId::from(view.entity_id()));
        area.update(cx, |area, cx| {
            let tabs = views.into_iter().fold(DockLayout::tabs(), |tabs, panel| {
                tabs.panel_view(panel_handle(panel), cx)
            });
            area.set_center(tabs, window, cx);
        });
        let host = cx.new(|_| Host(area.clone(), renders));
        Root {
            area,
            host,
            overlay,
            panels,
        }
    }

    #[gpui_kit::test]
    fn tabs_split_resize_merge_and_keep_the_cached_host(cx: &mut TestAppContext) {
        let renders = Rc::new(Cell::new(0));
        let counter = renders.clone();
        let (root, window) = cx.add_window_view(|window, cx| fixture(window, cx, counter));
        let (from, to, source) = window.update(|window, cx| {
            window.render_frame(cx);
            let root = root.read(cx);
            let from = window
                .find(("dock-tab", root.panels[0].as_u64()))
                .bounds()
                .center();
            let tree = root
                .area
                .read(cx)
                .layout(DockPlacement::Center)
                .expect("center");
            let source = tree.find_panel_node(root.panels[0]).expect("group");
            let bounds = root.overlay.read(cx).frame_bounds(source).expect("frame");
            (
                from,
                point(bounds.right() - px(12.), bounds.center().y),
                source,
            )
        });
        window.simulate_mouse_move(from, None, Default::default());
        window.simulate_mouse_down(from, MouseButton::Left, Default::default());
        let start = from + point(px(8.), px(0.));
        window.simulate_mouse_move(start, Some(MouseButton::Left), Default::default());
        window.run_until_parked();
        let before = renders.get();
        window.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
        window.run_until_parked();
        assert_eq!(renders.get(), before);
        window.update(|_, cx| {
            let overlay = root.read(cx).overlay.read(cx);
            assert!(matches!(
                overlay.resolve(to, source),
                Some((
                    InsertTarget::Split {
                        placement: Placement::Right,
                        ..
                    },
                    _
                ))
            ));
            assert!(overlay.outline.read(cx).bounds.is_some());
            assert!(!cx.has_active_drag());
        });
        window.simulate_mouse_up(to, MouseButton::Left, Default::default());
        window.run_until_parked();
        let (split, grip, first_width, destination, source_tab) = window.update(|window, cx| {
            window.render_frame(cx);
            let root = root.read(cx);
            let tree = root
                .area
                .read(cx)
                .layout(DockPlacement::Center)
                .expect("center");
            let split = tree.root();
            let PaneRef::Split { children, .. } = split.kind() else {
                panic!("expected split")
            };
            assert_eq!(children.len(), 2);
            let overlay = root.overlay.read(cx);
            let left = overlay.frame_bounds(children[0].id()).expect("left group");
            let right = overlay.frame_bounds(children[1].id()).expect("right group");
            assert_eq!(tree.find_panel_node(root.panels[0]), Some(children[1].id()));
            let tab = window
                .find(("dock-tab", root.panels[0].as_u64()))
                .bounds()
                .center();
            let grip = point(right.left(), right.center().y);
            (split.id(), grip, left.size.width, left.center(), tab)
        });
        window.simulate_mouse_down(grip, MouseButton::Left, Default::default());
        let end = grip + point(px(32.), px(0.));
        let before = renders.get();
        window.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        window.run_until_parked();
        assert_eq!(renders.get(), before);
        window.update(|_, cx| {
            let root = root.read(cx);
            assert!(matches!(
                root.overlay.read(cx).gesture,
                Some(Gesture::Split { .. })
            ));
            assert!(!cx.has_active_drag());
        });
        window.simulate_mouse_up(end, MouseButton::Left, Default::default());
        window.run_until_parked();
        let (source_tab, destination) = window.update(|window, cx| {
            window.render_frame(cx);
            let root = root.read(cx);
            let tree = root
                .area
                .read(cx)
                .layout(DockPlacement::Center)
                .expect("center");
            let PaneRef::Split {
                children, sizes, ..
            } = tree.find_node(split).expect("split").kind()
            else {
                panic!("expected split")
            };
            assert!(sizes[0].is_some_and(|width| width > first_width));
            let left = root
                .overlay
                .read(cx)
                .frame_bounds(children[0].id())
                .expect("left group");
            let tab = window
                .find(("dock-tab", root.panels[0].as_u64()))
                .bounds()
                .center();
            assert!(left.contains(&destination));
            assert!(source_tab.y == tab.y);
            (tab, left.center())
        });
        window.simulate_mouse_down(source_tab, MouseButton::Left, Default::default());
        window.simulate_mouse_move(destination, Some(MouseButton::Left), Default::default());
        window.simulate_mouse_up(destination, MouseButton::Left, Default::default());
        window.run_until_parked();
        window.update(|_, cx| {
            let root = root.read(cx);
            let tree = root
                .area
                .read(cx)
                .layout(DockPlacement::Center)
                .expect("center");
            assert_eq!(
                tree.find_panel_node(root.panels[0]),
                tree.find_panel_node(root.panels[1])
            );
        });
    }

    #[gpui_kit::test]
    fn tabs_reorder_and_escape_keeps_the_layout(cx: &mut TestAppContext) {
        let (root, window) = cx.add_window_view(|window, cx| fixture(window, cx, Rc::default()));
        let (first, last) = window.update(|window, cx| {
            window.render_frame(cx);
            let root = root.read(cx);
            let first = window.find(("dock-tab", root.panels[0].as_u64())).bounds();
            let last = window.find(("dock-tab", root.panels[2].as_u64())).bounds();
            (
                first.center(),
                point(last.right() - px(2.), last.center().y),
            )
        });
        for cancel in [true, false] {
            window.simulate_mouse_down(first, MouseButton::Left, Default::default());
            window.simulate_mouse_move(last, Some(MouseButton::Left), Default::default());
            if cancel {
                window.simulate_keystrokes("escape");
            }
            window.simulate_mouse_up(last, MouseButton::Left, Default::default());
            window.run_until_parked();
            window.update(|_, cx| {
                let root = root.read(cx);
                let tree = root
                    .area
                    .read(cx)
                    .layout(DockPlacement::Center)
                    .expect("center");
                let panels: Vec<_> = tree.panels().collect();
                assert_eq!(panels[usize::from(!cancel) * 2], root.panels[0]);
            });
        }
    }
}
