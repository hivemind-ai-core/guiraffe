//! DataUi dock adapter and reconstructible plain-text panels.

use std::rc::Rc;

use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, ParentElement, Render, RenderOnce, SharedString, StyleRefinement, Styled,
    Subscription, Window,
    base::dock::{DockLayout, Panel as BasePanel, PanelInfo},
    div, px,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};
use guiraffe_layout::LayoutStore;

use super::{
    DockArea, DockAreaState, DockDragOverlay, DockEvent, DockPlacement, DockSkin, Panel,
    PanelEvent, PanelState, new_dock, new_native_dock, panel_handle, register_panel,
};
use crate::widget_specs;

const PANEL_NAME: &str = "guiraffe-dataui-panel";
struct Registered;
impl Global for Registered {}

pub(crate) fn register(cx: &mut App) {
    if !cx.has_global::<Registered>() {
        register_panel(cx, PANEL_NAME, |context, _, cx| {
            let data = match context.info() {
                PanelInfo::Panel(data) => data.clone(),
                _ => serde_json::Value::Null,
            };
            panel_handle(cx.new(|cx| TextPanel::new(&data, cx)))
        });
        cx.set_global(Registered);
    }
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[1],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
            render: |node, _, _, _| DockDocument::new(node).into_any_element(),
        }],
    );
}

struct TextPanel {
    title: SharedString,
    body: SharedString,
    focus: FocusHandle,
    visible: bool,
    closable: bool,
    zoomable: bool,
}

impl TextPanel {
    fn new(data: &serde_json::Value, cx: &mut Context<Self>) -> Self {
        Self {
            title: data["title"].as_str().unwrap_or("Panel").to_owned().into(),
            body: data["text"].as_str().unwrap_or_default().to_owned().into(),
            visible: data["visible"].as_bool().unwrap_or(true),
            closable: data["closable"].as_bool().unwrap_or(true),
            zoomable: data["zoomable"].as_bool().unwrap_or(true),
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for TextPanel {}
impl Focusable for TextPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for TextPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }
    fn visible(&self, _: &App) -> bool {
        self.visible
    }
    fn closable(&self, _: &App) -> bool {
        self.closable
    }
    fn zoomable(&self, _: &App) -> bool {
        self.zoomable
    }

    fn dump(&self, _: &App) -> PanelState {
        PanelState {
            panel_name: PANEL_NAME.into(),
            children: Vec::new(),
            info: PanelInfo::panel(serde_json::json!({
                "title":self.title.as_ref(),"text":self.body.as_ref(),"visible":self.visible,
                "closable":self.closable,"zoomable":self.zoomable
            })),
        }
    }
}

impl Panel for TextPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(self.title.clone())
    }
}

impl Render for TextPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p_2().child(self.body.clone())
    }
}

#[derive(Clone, PartialEq)]
struct DockConfig {
    panels: Value,
    layout: Value,
    version: usize,
    native_drag: bool,
    persist_key: Option<String>,
    sizes: [f32; 3],
}

#[derive(IntoElement)]
struct DockDocument {
    state_key: ElementId,
    dock_id: SharedString,
    config: DockConfig,
    handlers: [Option<EventHandler>; 2],
}

impl DockDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let size = |i, default| node.prop(i).as_f64().unwrap_or(default).clamp(1.0, 4096.0) as f32;
        Self {
            state_key: node.state_id(),
            dock_id: node.state_id().to_string().into(),
            config: DockConfig {
                panels: node.prop(0).clone(),
                layout: node.prop(1).clone(),
                version: node
                    .prop(2)
                    .as_f64()
                    .unwrap_or(1.0)
                    .clamp(1.0, u32::MAX.into()) as usize,
                native_drag: node.prop(3).as_bool().unwrap_or_default(),
                persist_key: node.prop(4).as_str().map(str::to_owned),
                sizes: [size(5, 170.0), size(6, 170.0), size(7, 130.0)],
            },
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct MountedDock {
    area: Entity<DockArea>,
    drag: Option<Entity<DockDragOverlay>>,
    _skin: Option<Rc<DockSkin>>,
    _subscription: Subscription,
}

struct DockDocumentState {
    mounted: MountedDock,
    config: DockConfig,
    handlers: [Option<EventHandler>; 2],
}

fn mount(
    id: SharedString,
    config: &DockConfig,
    window: &mut Window,
    cx: &mut Context<DockDocumentState>,
) -> MountedDock {
    let (area, drag, skin) = if config.native_drag {
        let (area, skin) = new_native_dock(id, config.version, window, cx);
        (area, None, Some(skin))
    } else {
        let (area, drag) = new_dock(id, config.version, window, cx);
        (area, Some(drag), None)
    };
    let layout = if matches!(config.layout, Value::Null) {
        config
            .persist_key
            .as_ref()
            .zip(cx.try_global::<LayoutStore>())
            .and_then(|(key, store)| super::load(store, key, config.version).ok().flatten())
    } else {
        serde_json::from_value::<DockAreaState>(config.layout.to_json()).ok()
    };
    let restored = layout
        .is_some_and(|layout| area.update(cx, |area, cx| area.load(layout, window, cx).is_ok()));
    if !restored {
        let panels = config.panels.to_json();
        let tabs = |side, cx: &mut App| {
            panels
                .as_array()
                .into_iter()
                .flatten()
                .filter(|panel| panel["placement"].as_str().unwrap_or("center") == side)
                .fold((0, DockLayout::tabs()), |(count, tabs), panel| {
                    let panel = cx.new(|cx| TextPanel::new(panel, cx));
                    (count + 1, tabs.panel_view(panel_handle(panel), cx))
                })
        };
        area.update(cx, |area, cx| {
            let (_, center) = tabs("center", cx);
            area.set_center(center, window, cx);
            for (index, (side, placement)) in [
                ("left", DockPlacement::Left),
                ("right", DockPlacement::Right),
                ("bottom", DockPlacement::Bottom),
            ]
            .into_iter()
            .enumerate()
            {
                let (count, layout) = tabs(side, cx);
                if count > 0 {
                    area.set_dock(placement, layout, window, cx);
                    area.set_dock_size(placement, px(config.sizes[index]), window, cx);
                }
            }
        });
    }
    let subscription = cx.subscribe_in(
        &area,
        window,
        |state: &mut DockDocumentState, area, event, window, cx| {
            if !matches!(event, DockEvent::LayoutChanged) {
                return;
            }
            let snapshot = area.read(cx).dump(cx);
            if let Some((key, store)) = state
                .config
                .persist_key
                .as_ref()
                .zip(cx.try_global::<LayoutStore>())
                && let Err(error) = super::save(store, key, &snapshot)
                && let Some(handler) = &state.handlers[1]
            {
                handler(error.to_string().into(), window, cx);
            }
            if let Some(handler) = &state.handlers[0]
                && let Ok(value) = serde_json::to_value(&snapshot)
                && let Ok(payload) = Value::item_from_json(&value)
            {
                handler(payload, window, cx);
            }
        },
    );
    MountedDock {
        area,
        drag,
        _skin: skin,
        _subscription: subscription,
    }
}

impl RenderOnce for DockDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.state_key, cx, |window, cx| DockDocumentState {
            mounted: mount(self.dock_id.clone(), &self.config, window, cx),
            config: self.config.clone(),
            handlers: [None, None],
        });
        state.update(cx, |state, cx| {
            if state.config != self.config {
                state.config = self.config;
                state.mounted = mount(self.dock_id, &state.config, window, cx);
            }
            state.handlers = self.handlers;
            div()
                .relative()
                .size_full()
                .child(
                    state
                        .mounted
                        .area
                        .clone()
                        .cached(StyleRefinement::default().size_full()),
                )
                .children(state.mounted.drag.clone().map(|overlay| {
                    overlay.cached(StyleRefinement::default().absolute().size_full())
                }))
        })
    }
}
