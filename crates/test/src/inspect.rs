//! The in-process inspect API (spec/21 §2).
//!
//! Every node of the app under inspection carries a stable id, and the API
//! answers what the last frame says about it: where it is, what an assistive
//! technology sees, which tokens it was painted with and which events the app
//! emitted.
//!
//! The facts come from the framework's own observation of the last frame (the
//! element and text registries of the test support), so the answers are the
//! ones the platform gives, not a second opinion. A widget joins the registry
//! by observing itself (`.test_support()`), which is also what makes its
//! focus binding observable.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use gpui_kit::{App, Bounds, ElementId, Pixels, Role, VisualTestContext, Window};
use guiraffe_core::{ColorName, GuiraffeTheme, StyleContext, Variant, color::Rgb};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// The stable id of a node in the app under inspection.
///
/// A widget carries the id in its element id (the id a document node is
/// rendered with, or the id a static widget is built with), and the inspector
/// finds the node by that id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(String);

impl NodeId {
    /// The id of a node.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The element id a node id names.
    pub fn element_id(&self) -> ElementId {
        ElementId::Name(self.0.clone().into())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for NodeId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

impl From<String> for NodeId {
    fn from(id: String) -> Self {
        Self::new(id)
    }
}

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// The rectangle of laid-out bounds.
    pub fn of(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: bounds.origin.x.into(),
            y: bounds.origin.y.into(),
            width: bounds.size.width.into(),
            height: bounds.size.height.into(),
        }
    }

    /// The right edge.
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge.
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Whether the rectangle holds a point.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
}

/// The accessibility facts of a node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct A11yFacts {
    /// The role the platform gives the node.
    pub role: Option<String>,
    /// The accessible name.
    pub name: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// The value (the native value, not the rendered text).
    pub value: Option<String>,
    /// The placeholder, for inputs that have one.
    pub placeholder: Option<String>,
    /// The state of the node: checked, selected, expanded, disabled.
    pub state: BTreeMap<String, bool>,
    /// Whether the node takes focus.
    pub focusable: bool,
    /// Whether the node has the keyboard focus now.
    pub focused: bool,
}

/// The layout facts of a node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutFacts {
    /// The bounds of the node.
    pub bounds: Rect,
    /// The size of the content of the node, when it is known (the text it
    /// painted).
    pub content_size: Option<(f32, f32)>,
    /// Whether the content is bigger than the box (it overflows).
    pub overflows: bool,
    /// Whether the content is clipped by the box.
    pub clipped: bool,
    /// Whether the text of the node is truncated.
    pub truncated: bool,
    /// Whether the node is visible.
    pub visible: bool,
    /// The scroll offset of the node, when it scrolls.
    pub scroll_offset: Option<(f32, f32)>,
}

/// What the last frame says about one node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeFacts {
    /// The stable id of the node.
    pub id: NodeId,
    /// The element id path of the node, outermost first.
    pub path: Vec<String>,
    /// The layout facts.
    pub layout: LayoutFacts,
    /// The accessibility facts.
    pub a11y: A11yFacts,
}

/// The nodes of the last frame, by their stable ids.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeIndex {
    nodes: Vec<NodeFacts>,
}

impl NodeIndex {
    /// The nodes of the window's last frame.
    ///
    /// Every observed element is a node; its id is the last element of its
    /// path, which is the id the widget carries.
    pub fn collect(window: &Window) -> Self {
        use gpui_kit::test::TestQueryExt;

        let texts = gpui_base::test_support::texts(window);
        let mut nodes: Vec<NodeFacts> = window
            .elements()
            .into_iter()
            .filter_map(|element| {
                let id = node_id_of(element.path().last()?)?;
                let text = texts.iter().find(|t| t.path() == element.path());
                let mut state = BTreeMap::new();
                if let Some(checked) = element.checked() {
                    state.insert("checked".into(), checked);
                }
                if let Some(indeterminate) = element.indeterminate() {
                    state.insert("indeterminate".into(), indeterminate);
                }
                if let Some(selected) = element.selected() {
                    state.insert("selected".into(), selected);
                }
                if let Some(expanded) = element.expanded() {
                    state.insert("expanded".into(), expanded);
                }
                if let Some(disabled) = element.disabled() {
                    state.insert("disabled".into(), disabled);
                }
                let bounds = element.bounds();
                let content = text.map(|t| (t.bounds().size.width, t.bounds().size.height));
                let layout = LayoutFacts {
                    bounds: Rect::of(bounds),
                    content_size: content.map(|(w, h)| (w.into(), h.into())),
                    overflows: content
                        .is_some_and(|(w, h)| w > bounds.size.width || h > bounds.size.height),
                    clipped: text.is_some_and(|t| !t.visible()),
                    truncated: content
                        .is_some_and(|(w, h)| w > bounds.size.width || h > bounds.size.height),
                    visible: element.visible(),
                    scroll_offset: None,
                };
                Some(NodeFacts {
                    id,
                    path: element.path().iter().map(|id| format!("{id:?}")).collect(),
                    layout,
                    a11y: A11yFacts {
                        role: element.role().map(role_name),
                        name: element.label().map(str::to_owned),
                        description: element.description().map(str::to_owned),
                        value: element.value().map(str::to_owned),
                        placeholder: element.placeholder().map(str::to_owned),
                        state,
                        // The focus binding is what makes a node focusable;
                        // without one the platform reports no focus at all.
                        focusable: element.focused().is_some(),
                        focused: element.focused().unwrap_or(false),
                    },
                })
            })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        Self { nodes }
    }

    /// The node with this id.
    pub fn node(&self, id: &NodeId) -> Option<&NodeFacts> {
        self.nodes.iter().find(|node| node.id == *id)
    }

    /// Every node, by id.
    pub fn nodes(&self) -> &[NodeFacts] {
        &self.nodes
    }

    /// The ids of every node, in id order.
    pub fn ids(&self) -> Vec<NodeId> {
        self.nodes.iter().map(|node| node.id.clone()).collect()
    }
}

/// The stable id of an element id, when it has one.
fn node_id_of(id: &ElementId) -> Option<NodeId> {
    match id {
        ElementId::Name(name) => Some(NodeId::new(name.to_string())),
        _ => None,
    }
}

/// The name of an accessibility role.
pub fn role_name(role: Role) -> String {
    format!("{role:?}")
}

/// An event the app under inspection emitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// The name of the event.
    pub name: String,
    /// The payload of the event, as text.
    pub payload: Option<String>,
}

impl RecordedEvent {
    /// An event with a name and a payload.
    pub fn new(name: impl Into<String>, payload: Option<String>) -> Self {
        Self {
            name: name.into(),
            payload,
        }
    }
}

/// The sink the app under inspection pushes its events into.
///
/// The inspector reads the events from the sink; the app writes them.
#[derive(Clone, Default)]
pub struct EventSink {
    events: Rc<RefCell<Vec<RecordedEvent>>>,
}

impl EventSink {
    /// A sink with no events.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an event.
    pub fn record(&self, event: RecordedEvent) {
        let mut events = self.events.borrow_mut();
        events.push(event);
    }

    /// Records an event with a name and a payload.
    pub fn emit(&self, name: impl Into<String>, payload: Option<String>) {
        self.record(RecordedEvent::new(name, payload));
    }

    /// The events, oldest first.
    pub fn events(&self) -> Vec<RecordedEvent> {
        self.events.borrow().clone()
    }

    /// The number of events.
    pub fn len(&self) -> usize {
        self.events.borrow().len()
    }

    /// Whether no event was recorded.
    pub fn is_empty(&self) -> bool {
        self.events.borrow().is_empty()
    }

    /// Forgets every event.
    pub fn clear(&self) {
        self.events.borrow_mut().clear();
    }
}

/// The resolved tokens of the app, and the context they came from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleReport {
    /// The density of the context.
    pub density: String,
    /// The control size of the context.
    pub control_size: String,
    /// The surface of the context.
    pub surface: String,
    /// The emphasis of the context.
    pub emphasis: String,
    /// Whether the context lets controls take keyboard focus.
    pub keyboard_nav: bool,
    /// The radius of the theme.
    pub radius: f32,
    /// The colours of the theme.
    pub colors: BTreeMap<String, String>,
}

/// The layout of every node (or of one node).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutReport {
    /// The nodes and their layout.
    pub nodes: Vec<NodeFacts>,
}

/// The accessibility facts of every node, with the order the keyboard visits
/// them in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct A11yReport {
    /// The nodes and their facts.
    pub nodes: Vec<NodeFacts>,
    /// The ids of the focusable nodes, in the order the Tab key visits them.
    pub focus_order: Vec<NodeId>,
    /// The nodes the Tab key visited, with the repeats.
    ///
    /// A Tab key that stays on a node is a focus trap: the focus cannot
    /// leave it.
    pub tab_walk: Vec<NodeId>,
}

impl A11yReport {
    /// The node with this id.
    pub fn node(&self, id: &NodeId) -> Option<&NodeFacts> {
        self.nodes.iter().find(|node| node.id == *id)
    }
}

/// A node of the accessibility tree the platform was given.
///
/// The tree is what an assistive technology sees. It is only built when the
/// platform asks for it, so it is missing in a headless test; the facts of
/// the observation registry answer the same questions there.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccessKitNode {
    /// The element id of the node, as the tree reports it.
    pub element_id: Option<String>,
    /// The role of the node.
    pub role: Option<String>,
    /// The accessible name.
    pub name: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// The value.
    pub value: Option<String>,
    /// The state of the node.
    pub state: BTreeMap<String, bool>,
    /// Whether the node takes focus.
    pub focusable: bool,
}

/// The accessibility tree of the last frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccessKitTree {
    /// The nodes, by the element id they belong to.
    pub nodes: Vec<AccessKitNode>,
    /// The id of the node that has the keyboard focus.
    pub focused: Option<String>,
}

impl AccessKitTree {
    /// The tree the platform was given, when it was given one.
    pub fn of_window(window: &Window) -> Option<Self> {
        let json = window.debug_a11y_tree_json()?;
        Self::parse(&json)
    }

    /// The tree of a debug dump of the accessibility tree.
    pub fn parse(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let focused = value
            .get("gpui_focus")
            .and_then(|id| id.as_str())
            .map(String::from);
        let mut nodes = Vec::new();
        let map = value.get("nodes")?.as_object()?;
        for (_, node) in map {
            let aria = node.get("aria");
            let mut state = BTreeMap::new();
            if let Some(aria) = aria {
                for (key, name) in [
                    ("checked", "checked"),
                    ("selected", "selected"),
                    ("expanded", "expanded"),
                    ("disabled", "disabled"),
                ] {
                    if let Some(value) = aria.get(key).and_then(|v| v.as_bool()) {
                        state.insert(name.to_string(), value);
                    }
                }
            }
            let actions = aria
                .and_then(|aria| aria.get("on_action"))
                .and_then(|actions| actions.as_array())
                .map(|actions| {
                    actions
                        .iter()
                        .filter_map(|action| action.as_str())
                        .any(|action| action == "Focus")
                })
                .unwrap_or(false);
            nodes.push(AccessKitNode {
                element_id: node
                    .get("element_id")
                    .and_then(|id| id.as_str())
                    .map(String::from),
                role: aria
                    .and_then(|aria| aria.get("role"))
                    .and_then(|role| role.as_str())
                    .map(String::from),
                name: aria
                    .and_then(|aria| aria.get("label"))
                    .and_then(|label| label.as_str())
                    .map(String::from),
                description: aria
                    .and_then(|aria| aria.get("description"))
                    .and_then(|aria| aria.as_str())
                    .map(String::from),
                value: aria
                    .and_then(|aria| aria.get("value"))
                    .and_then(|value| value.as_str())
                    .map(String::from),
                state,
                focusable: actions,
            });
        }
        Some(Self { nodes, focused })
    }

    /// The node of the element with this id.
    pub fn node(&self, element_id: &str) -> Option<&AccessKitNode> {
        self.nodes
            .iter()
            .find(|node| node.element_id.as_deref() == Some(element_id))
    }
}

/// The app the inspector looks at: a window and the context it runs in.
///
/// The test context and the headless context both are one, so the same API
/// works in a test and in a running app (which is what the CLI serves).
pub trait InspectTarget {
    /// Runs `f` against the window of the app.
    ///
    /// The answer is `None` when the window is gone.
    fn with_window<R>(&mut self, f: impl FnOnce(&mut Window, &mut App) -> R) -> Option<R>;

    /// Runs until the app has nothing left to do.
    fn park(&mut self);
}

impl InspectTarget for VisualTestContext {
    fn with_window<R>(&mut self, f: impl FnOnce(&mut Window, &mut App) -> R) -> Option<R> {
        Some(self.update(|window, cx| f(window, cx)))
    }

    fn park(&mut self) {
        self.run_until_parked();
    }
}

/// The headless app the inspector looks at: the headless context and the
/// window of the app.
///
/// The headless context has the renderer of the snapshots, so a capture of
/// the window is a picture and not nothing.
pub struct HeadlessTarget<'a> {
    cx: &'a mut gpui_kit::HeadlessAppContext,
    window: gpui_kit::AnyWindowHandle,
}

impl<'a> HeadlessTarget<'a> {
    /// The headless app and the window of it.
    pub fn new(
        cx: &'a mut gpui_kit::HeadlessAppContext,
        window: gpui_kit::AnyWindowHandle,
    ) -> Self {
        Self { cx, window }
    }
}

impl InspectTarget for HeadlessTarget<'_> {
    fn with_window<R>(&mut self, f: impl FnOnce(&mut Window, &mut App) -> R) -> Option<R> {
        self.cx
            .update_window(self.window, |_, window, app| f(window, app))
            .ok()
    }

    fn park(&mut self) {
        self.cx.run_until_parked();
    }
}

/// The in-process inspector: the API an agent uses to look at an app.
pub struct Inspector<'a, T> {
    target: &'a mut T,
    events: EventSink,
}

impl<'a, T: InspectTarget> Inspector<'a, T> {
    /// An inspector over a running window.
    pub fn new(target: &'a mut T) -> Self {
        Self {
            target,
            events: EventSink::new(),
        }
    }

    /// The sink the app under inspection pushes its events into.
    pub fn events(&self) -> &EventSink {
        &self.events
    }

    /// The nodes of the last frame.
    pub fn nodes(&mut self) -> NodeIndex {
        self.target
            .with_window(|window, _| NodeIndex::collect(window))
            .unwrap_or_default()
    }

    /// The layout of one node, or of every node.
    pub fn layout(&mut self, node: Option<&NodeId>) -> LayoutReport {
        layout_report(self.target, node)
    }

    /// The accessibility facts of every node.
    ///
    /// The focus order is the order the Tab key visits the focusable nodes
    /// in, measured by pressing Tab and watching where the focus goes.
    pub fn a11y(&mut self) -> A11yReport {
        let index = self.nodes();
        let nodes = index.nodes().to_vec();
        let (focus_order, tab_walk) = tab_walk(self.target, &nodes);
        A11yReport {
            nodes,
            focus_order,
            tab_walk,
        }
    }

    /// The resolved tokens of the app and the context they came from.
    pub fn style(&mut self) -> StyleReport {
        style_report()
    }

    /// A PNG of the window, or of the bounds of a node.
    pub fn screenshot(&mut self, node: Option<&NodeId>) -> Option<Vec<u8>> {
        screenshot_of(self.target, node)
    }

    /// The events the app emitted, oldest first.
    pub fn event_log(&mut self) -> Vec<RecordedEvent> {
        self.events.events()
    }
}

/// The layout of one node, or of every node.
pub fn layout_report<T: InspectTarget>(target: &mut T, node: Option<&NodeId>) -> LayoutReport {
    let index = target
        .with_window(|window, _| NodeIndex::collect(window))
        .unwrap_or_default();
    let nodes = match node {
        Some(id) => index
            .nodes()
            .iter()
            .filter(|node| node.id == *id)
            .cloned()
            .collect(),
        None => index.nodes().to_vec(),
    };
    LayoutReport { nodes }
}

/// The resolved tokens of the app and the context they came from.
pub fn style_report() -> StyleReport {
    let context = StyleContext::current();
    let theme = GuiraffeTheme::default();
    let mut colors = BTreeMap::new();
    let surfaces = theme.surfaces();
    colors.insert("body".into(), hex(surfaces.body));
    colors.insert("raised".into(), hex(surfaces.raised));
    colors.insert("border".into(), hex(surfaces.border));
    colors.insert("primary".into(), hex(theme.primary_color()));
    colors.insert(
        "danger".into(),
        hex(theme
            .variant(Some(ColorName::Red), Variant::Filled)
            .background),
    );
    colors.insert(
        "success".into(),
        hex(theme
            .variant(Some(ColorName::Green), Variant::Filled)
            .background),
    );
    colors.insert(
        "warning".into(),
        hex(theme
            .variant(Some(ColorName::Yellow), Variant::Filled)
            .background),
    );
    colors.insert(
        "info".into(),
        hex(theme
            .variant(Some(ColorName::Blue), Variant::Filled)
            .background),
    );
    StyleReport {
        density: format!("{:?}", context.density),
        control_size: format!("{:?}", context.control_size),
        surface: format!("{:?}", context.surface),
        emphasis: format!("{:?}", context.emphasis),
        keyboard_nav: context.keyboard_nav,
        radius: theme.default_radius.radius(),
        colors,
    }
}

/// Crops an image to a rectangle.
fn crop(image: &RgbaImage, rect: Rect) -> Option<RgbaImage> {
    let x = rect.x.max(0.).round() as u32;
    let y = rect.y.max(0.).round() as u32;
    let width = rect.width.max(0.).round() as u32;
    let height = rect.height.max(0.).round() as u32;
    if width == 0 || height == 0 {
        return None;
    }
    let mut out = RgbaImage::new(width, height);
    for (index, pixel) in out.pixels_mut().enumerate() {
        let column = (index as u32) % width;
        let row = (index as u32) / width;
        *pixel = *image.get_pixel(x + column, y + row);
    }
    Some(out)
}

/// Encodes an image as a PNG.
fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    image::ImageEncoder::write_image(
        encoder,
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(bytes)
}

/// The accessibility facts of every node, with the order the Tab key visits
/// the focusable nodes in.
///
/// The order is measured: the Tab key is pressed and the focus is watched.
pub fn a11y_report<T: InspectTarget>(target: &mut T) -> A11yReport {
    let index = target
        .with_window(|window, _| NodeIndex::collect(window))
        .unwrap_or_default();
    let nodes = index.nodes().to_vec();
    let (focus_order, tab_walk) = tab_walk(target, &nodes);
    A11yReport {
        nodes,
        focus_order,
        tab_walk,
    }
}

/// The order the Tab key visits the focusable nodes in, and the walk itself.
///
/// The walk keeps the repeats: a Tab key that stays on a node is a focus
/// trap, and the order alone would not show it.
fn tab_walk<T: InspectTarget>(target: &mut T, nodes: &[NodeFacts]) -> (Vec<NodeId>, Vec<NodeId>) {
    let mut order: Vec<NodeId> = Vec::new();
    let mut walk: Vec<NodeId> = Vec::new();
    for _ in 0..nodes.len().saturating_add(3) {
        target.with_window(|window, cx| {
            window.focus_next(cx);
        });
        target.park();
        let Some(next) = focused_node(target, nodes) else {
            break;
        };
        walk.push(next.clone());
        if !order.contains(&next) {
            order.push(next);
        }
    }
    (order, walk)
}

/// The node that has the keyboard focus now.
fn focused_node<T: InspectTarget>(target: &mut T, nodes: &[NodeFacts]) -> Option<NodeId> {
    let index = target
        .with_window(|window, _| NodeIndex::collect(window))
        .unwrap_or_default();
    index
        .nodes()
        .iter()
        .find(|node| node.a11y.focused)
        .map(|node| node.id.clone())
        .or_else(|| {
            nodes
                .iter()
                .find(|node| node.a11y.focused)
                .map(|n| n.id.clone())
        })
}

/// The layout of one node, or of every node.
pub fn screenshot_of<T: InspectTarget>(target: &mut T, node: Option<&NodeId>) -> Option<Vec<u8>> {
    let image = target.with_window(|window, _| window.render_to_image().ok())??;
    let image = match node {
        Some(id) => {
            let bounds = layout_report(target, Some(id)).nodes.first()?.layout.bounds;
            crop(&image, bounds)?
        }
        None => image,
    };
    encode_png(&image).ok()
}

/// The hexadecimal colour of a colour.
fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dump of an accessibility tree, as the platform receives it.
    const TREE: &str = r#"{
        "root": "0",
        "gpui_focus": "1",
        "frame": {"node_count": 2, "tab_stop_count": 1},
        "nodes": {
            "0": {
                "accesskit_id": "0",
                "children": ["1"],
                "element_id": "Name(\"panel\")",
                "aria": {"role": "Group", "label": "Panel"}
            },
            "1": {
                "accesskit_id": "1",
                "element_id": "Name(\"save\")",
                "aria": {"role": "Button", "label": "Save", "on_action": ["Focus", "Click"], "disabled": false}
            }
        }
    }"#;

    #[test]
    fn the_accessibility_tree_maps_the_element_ids_to_the_roles_and_names() {
        let tree = AccessKitTree::parse(TREE).expect("the tree parses");
        assert_eq!(tree.focused.as_deref(), Some("1"));
        let save = tree
            .node("Name(\"save\")")
            .expect("the save node of the tree");
        assert_eq!(save.role.as_deref(), Some("Button"));
        assert_eq!(save.name.as_deref(), Some("Save"));
        assert!(save.focusable, "the node takes focus");
        assert_eq!(save.state.get("disabled").copied(), Some(false));
    }

    #[test]
    fn a_node_id_is_the_element_id_a_widget_carries() {
        let id = NodeId::new("save");
        assert_eq!(id.as_str(), "save");
        assert_eq!(id.to_string(), "save");
        assert_eq!(id.element_id(), ElementId::Name("save".into()));
    }

    #[test]
    fn a_rectangle_knows_its_edges() {
        let rect = Rect {
            x: 10.,
            y: 20.,
            width: 30.,
            height: 40.,
        };
        assert_eq!(rect.right(), 40.);
        assert_eq!(rect.bottom(), 60.);
        assert!(rect.contains(20., 30.));
        assert!(!rect.contains(5., 30.));
    }
}
