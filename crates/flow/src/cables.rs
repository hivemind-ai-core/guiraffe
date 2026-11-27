//! Cables between any element and flow ports, drawn by a window-level overlay (#152).
//!
//! Mark an element with [`CableAnchorExt::cable_anchor`]: each frame it records where it is
//! in window coordinates. Keep the cables, host data keyed by [`CableId`], in a
//! [`Store<Cables>`]. Put one [`CableLayer`] last in the window's root, absolutely over the
//! content (and under popups), and register the flows whose ports cables reach with
//! [`CableLayer::add_flow`]. The layer draws each cable with the flow edge look: the same
//! curve ([`cable_curve`](crate::core::cable_curve)), width and colour, solid, dashed or
//! dotted ([`StrokeStyle`]).
//!
//! # Hidden cables
//!
//! A cable is drawn only while both of its ends are visible, and is hidden otherwise
//! rather than clamped to an edge:
//!
//! - an element end is visible while its anchor point lies inside the clip it was painted
//!   in (so a knob scrolled out of its panel hides its cables) and inside the window;
//! - a port end is visible while its port lies on the flow canvas
//!   ([`PortAnchor::visible`](crate::PortAnchor));
//! - an end that names an element never painted, or a flow or port the layer does not know,
//!   is not visible.
//!
//! An element keeps its last painted place until it paints again: remove the cables of an
//! element you take away, or [`forget_cable_anchor`] it.
//!
//! # Cost
//!
//! A frame that moves no anchor allocates nothing for cables: anchors compare their place
//! with the last one, and the layer is not drawn again. When an anchor moves, the layer
//! draws again in time proportional to the number of cables, with one path per cable.
//! Mount the layer as a cached view (`AnyView::cached`) so that other views drawing again
//! do not draw it. On gpui-fast a window with a flow canvas is drawn in full, which builds
//! cached views again, so there the layer draws on every frame (#153).

use crate::core::{self, Side, StrokeStyle};
use crate::{AnchorsMoved, FlowState};
use gpui_kit::{prelude::*, *};
use guiraffe_core::{ColorName, GuiraffeTheme};
use guiraffe_state::{Event, Fx, KeyedList, Selected, Store};
use std::collections::HashMap;

/// A cable's key in [`Cables`].
pub type CableId = u64;

/// The host's cables, keyed by id, in drawing order.
pub type Cables = KeyedList<CableId, Cable>;

/// One end of a cable.
#[derive(Clone, Debug, PartialEq)]
pub enum CableEnd {
    /// An element marked with [`CableAnchorExt::cable_anchor`] under this id.
    Element(SharedString),
    /// A port of a node in a flow registered with [`CableLayer::add_flow`] under `flow`.
    Port {
        flow: SharedString,
        node: core::NodeId,
        port: SharedString,
    },
}

impl CableEnd {
    /// The element anchored as `id`.
    pub fn element(id: impl Into<SharedString>) -> Self {
        Self::Element(id.into())
    }
    /// Port `port` of `node` in the flow registered as `flow`.
    pub fn port(
        flow: impl Into<SharedString>,
        node: core::NodeId,
        port: impl Into<SharedString>,
    ) -> Self {
        Self::Port {
            flow: flow.into(),
            node,
            port: port.into(),
        }
    }
}

/// A cable from one end to another.
#[derive(Clone, Debug, PartialEq)]
pub struct Cable {
    pub from: CableEnd,
    pub to: CableEnd,
    pub style: StrokeStyle,
    /// A theme palette colour, shaded for the scheme like a coloured flow edge; `None` is
    /// the dimmed colour of an idle edge.
    pub color: Option<ColorName>,
    /// Caps on how far the curve's handles reach from each end; see [`Cable::handle`].
    pub handles: core::HandleCaps,
}

impl Cable {
    /// Caps both handles at `len` pixels, so the cable turns within about `len` of each end
    /// (by default a handle reaches `min(0.4 × distance + 24, 300)`).
    pub fn handle(mut self, len: f32) -> Self {
        self.handles = core::HandleCaps {
            from: Some(len),
            to: Some(len),
        };
        self
    }
    /// Caps the handle at the `from` end.
    pub fn from_handle(mut self, len: f32) -> Self {
        self.handles.from = Some(len);
        self
    }
    /// Caps the handle at the `to` end.
    pub fn to_handle(mut self, len: f32) -> Self {
        self.handles.to = Some(len);
        self
    }
    /// A solid cable from `from` to `to`.
    pub fn new(from: CableEnd, to: CableEnd) -> Self {
        Self {
            from,
            to,
            style: StrokeStyle::Solid,
            color: None,
            handles: core::HandleCaps::default(),
        }
    }
    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }
    pub fn style(mut self, style: StrokeStyle) -> Self {
        self.style = style;
        self
    }
}

/// Adds the cable, or replaces the one with its id in place.
#[derive(Debug)]
pub struct SetCable(pub CableId, pub Cable);

impl Event<Cables> for SetCable {
    fn handle(self, fx: &mut Fx<'_, Cables>) {
        let SetCable(id, cable) = self;
        fx.update(|cables| match cables.get_mut(&id) {
            Some(existing) => *existing = cable,
            None => cables.push(id, cable),
        });
    }
}

/// Removes the cable with this id, if there is one.
#[derive(Debug)]
pub struct RemoveCable(pub CableId);

impl Event<Cables> for RemoveCable {
    fn handle(self, fx: &mut Fx<'_, Cables>) {
        fx.update(|cables| {
            cables.remove(&self.0);
        });
    }
}

/// Where an anchored element was last painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementAnchor {
    /// The element's bounds in window coordinates.
    pub bounds: Bounds<Pixels>,
    /// The clip it was painted in.
    pub clip: Bounds<Pixels>,
    /// The edge cables attach to, or `None` for the centre.
    pub side: Option<Side>,
}

impl ElementAnchor {
    /// Where cables attach: the centre, or the middle of the anchor's side.
    pub fn point(&self) -> Point<Pixels> {
        let b = self.bounds;
        let c = b.center();
        match self.side {
            None => c,
            Some(Side::Left) => point(b.left(), c.y),
            Some(Side::Right) => point(b.right(), c.y),
            Some(Side::Top) => point(c.x, b.top()),
            Some(Side::Bottom) => point(c.x, b.bottom()),
        }
    }
}

#[derive(Default)]
struct WindowAnchors {
    anchors: HashMap<SharedString, ElementAnchor>,
    layer: Option<WeakEntity<CableLayer>>,
    /// An anchor moved after the layer last painted.
    stale: bool,
    /// A check that the layer caught up is queued.
    check_queued: bool,
}

#[derive(Default)]
struct CableAnchors(HashMap<WindowId, WindowAnchors>);

impl Global for CableAnchors {}

/// Where the element anchored as `id` was last painted in `window`.
pub fn cable_anchor(id: &str, window: &Window, cx: &App) -> Option<ElementAnchor> {
    cx.try_global::<CableAnchors>()?
        .0
        .get(&window.window_handle().window_id())?
        .anchors
        .get(id)
        .copied()
}

/// Forgets the anchor `id` in `window`: its cables hide until it paints again.
pub fn forget_cable_anchor(id: &str, window: &Window, cx: &mut App) {
    let window_id = window.window_handle().window_id();
    let queue = cx
        .try_global::<CableAnchors>()
        .is_some()
        .then(|| {
            let w = cx.global_mut::<CableAnchors>().0.get_mut(&window_id)?;
            w.anchors.remove(id)?;
            Some(mark_stale(w))
        })
        .flatten()
        .unwrap_or(false);
    if queue {
        queue_check(window_id, cx);
    }
}

/// Notes that an anchor moved; returns whether a catch-up check must be queued.
fn mark_stale(w: &mut WindowAnchors) -> bool {
    w.stale = true;
    !std::mem::replace(&mut w.check_queued, true)
}

/// After the frame: if the layer did not paint since an anchor moved (it was not drawn
/// again this frame), draw it next frame.
fn queue_check(window_id: WindowId, cx: &mut App) {
    cx.defer(move |cx| {
        let Some(w) = cx.global_mut::<CableAnchors>().0.get_mut(&window_id) else {
            return;
        };
        w.check_queued = false;
        if w.stale
            && let Some(layer) = w.layer.clone()
        {
            let _ = layer.update(cx, |_, cx| cx.notify());
        }
    });
}

/// Marks any element as a cable end. See [`CableAnchorExt::cable_anchor`].
pub struct CableAnchor {
    id: SharedString,
    side: Option<Side>,
    child: AnyElement,
}

impl CableAnchor {
    /// Attach cables to the middle of this side instead of the centre.
    pub fn side(mut self, side: Side) -> Self {
        self.side = Some(side);
        self
    }
}

/// Marks an element as an end cables can reach.
pub trait CableAnchorExt: IntoElement + Sized {
    /// Records where this element is painted under `id` (unique in its window), so that
    /// [`CableEnd::Element`] cables reach it.
    fn cable_anchor(self, id: impl Into<SharedString>) -> CableAnchor {
        CableAnchor {
            id: id.into(),
            side: None,
            child: self.into_any_element(),
        }
    }
}

impl<E: IntoElement> CableAnchorExt for E {}

impl IntoElement for CableAnchor {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for CableAnchor {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
        let anchor = ElementAnchor {
            bounds,
            clip: window.content_mask().bounds,
            side: self.side,
        };
        let window_id = window.window_handle().window_id();
        // Read first: a frame that moves nothing writes nothing.
        let unchanged = cx
            .try_global::<CableAnchors>()
            .and_then(|anchors| anchors.0.get(&window_id))
            .and_then(|w| w.anchors.get(&*self.id))
            == Some(&anchor);
        if unchanged {
            return;
        }
        let w = cx
            .default_global::<CableAnchors>()
            .0
            .entry(window_id)
            .or_default();
        match w.anchors.get_mut(&*self.id) {
            Some(last) => *last = anchor,
            None => {
                w.anchors.insert(self.id.clone(), anchor);
            }
        }
        let queue = mark_stale(w);
        if queue {
            queue_check(window_id, cx);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

/// A resolved cable end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CableEndpoint {
    /// Where the cable attaches, in window coordinates.
    pub position: Point<Pixels>,
    /// The side it leaves along, or `None` to leave towards the other end.
    pub side: Option<Side>,
    /// Whether the end is visible; see the module docs.
    pub visible: bool,
}

/// The registered flows, shared with the layer's paint without copying.
type Flows = std::rc::Rc<[(SharedString, Entity<FlowState>)]>;

/// The window-level overlay that draws [`Cables`]. See the module docs.
pub struct CableLayer {
    cables: Selected<Cables>,
    flows: Flows,
    subscriptions: Vec<Subscription>,
    scratch: std::rc::Rc<std::cell::RefCell<Vec<(f32, f32)>>>,
}

impl CableLayer {
    pub fn new(store: &Store<Cables>, cx: &mut Context<Self>) -> Self {
        Self {
            cables: store.select(cx, Cables::clone),
            flows: Flows::default(),
            subscriptions: Vec::new(),
            scratch: Default::default(),
        }
    }

    /// Lets [`CableEnd::Port`] cables reach `flow`'s ports under `key`; the layer draws
    /// again when its ports move. A second flow under the same key replaces the first.
    pub fn add_flow(
        &mut self,
        key: impl Into<SharedString>,
        flow: &Entity<FlowState>,
        cx: &mut Context<Self>,
    ) {
        let key = key.into();
        let canvas = flow.read(cx).viewport_entity().clone();
        let subscription = cx.subscribe(&canvas, |_, _, _: &AnchorsMoved, cx| cx.notify());
        let mut flows: Vec<_> = self
            .flows
            .iter()
            .filter(|(k, _)| *k != key)
            .cloned()
            .collect();
        let at = self.flows.iter().position(|(k, _)| *k == key);
        flows.push((key, flow.clone()));
        if let Some(at) = at {
            drop(self.subscriptions.remove(at));
        }
        self.subscriptions.push(subscription);
        self.flows = flows.into();
        cx.notify();
    }

    /// Where a cable's ends are, as last painted, or `None` for an unknown cable.
    pub fn endpoints(
        &self,
        id: CableId,
        window: &Window,
        cx: &App,
    ) -> Option<(CableEndpoint, CableEndpoint)> {
        let cable = self.cables.get(cx).get(&id)?;
        Some((
            resolve(&cable.from, &self.flows, window, cx),
            resolve(&cable.to, &self.flows, window, cx),
        ))
    }
}

fn resolve(
    end: &CableEnd,
    flows: &[(SharedString, Entity<FlowState>)],
    window: &Window,
    cx: &App,
) -> CableEndpoint {
    let hidden = CableEndpoint {
        position: Point::default(),
        side: None,
        visible: false,
    };
    match end {
        CableEnd::Element(id) => {
            let Some(anchor) = cable_anchor(id, window, cx) else {
                return hidden;
            };
            let position = anchor.point();
            let screen = Bounds::new(Point::default(), window.viewport_size());
            CableEndpoint {
                position,
                side: anchor.side,
                visible: contains(&anchor.clip, position) && contains(&screen, position),
            }
        }
        CableEnd::Port { flow, node, port } => {
            let Some((_, flow)) = flows.iter().find(|(key, _)| key == flow) else {
                return hidden;
            };
            match flow.read(cx).port_anchor(*node, port, cx) {
                Some(anchor) => CableEndpoint {
                    position: anchor.position,
                    side: Some(anchor.side),
                    visible: anchor.visible,
                },
                None => hidden,
            }
        }
    }
}

/// Inclusive of the far edges, so an anchor on an element's border still counts.
fn contains(bounds: &Bounds<Pixels>, p: Point<Pixels>) -> bool {
    p.x >= bounds.left() && p.x <= bounds.right() && p.y >= bounds.top() && p.y <= bounds.bottom()
}

/// The direction a cable leaves `end` towards `other`.
fn outward(end: &CableEndpoint, other: &CableEndpoint) -> core::Point {
    match end.side {
        Some(side) => side.direction(),
        None if other.position.x < end.position.x => core::Point::new(-1., 0.),
        None => core::Point::new(1., 0.),
    }
}

const CABLE_STROKE: f32 = 1.5;

impl Render for CableLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_id = window.window_handle().window_id();
        let me = cx.entity().downgrade();
        let w = cx
            .default_global::<CableAnchors>()
            .0
            .entry(window_id)
            .or_default();
        if w.layer
            .as_ref()
            .is_none_or(|layer| layer.entity_id() != me.entity_id())
        {
            w.layer = Some(me);
        }
        let cables = self.cables.get(cx).clone();
        let flows = self.flows.clone();
        let scratch = self.scratch.clone();
        let theme = GuiraffeTheme::global(cx);
        let dimmed = theme.surfaces().dimmed;
        // Resolved only when a cable is coloured.
        let palette = cables
            .iter()
            .any(|(_, cable)| cable.color.is_some())
            .then(|| crate::ink::palette(theme));
        canvas(
            |_, _, _| {},
            move |_, (), window, cx| {
                let stale = cx
                    .try_global::<CableAnchors>()
                    .and_then(|anchors| anchors.0.get(&window_id))
                    .is_some_and(|w| w.stale);
                if stale && let Some(w) = cx.global_mut::<CableAnchors>().0.get_mut(&window_id) {
                    w.stale = false;
                }
                let mut scratch = scratch.borrow_mut();
                for (_, cable) in cables.iter() {
                    let from = resolve(&cable.from, &flows, window, cx);
                    let to = resolve(&cable.to, &flows, window, cx);
                    if !(from.visible && to.visible) {
                        continue;
                    }
                    let at = |p: Point<Pixels>| core::Point::new(p.x.as_f32(), p.y.as_f32());
                    let curve = core::cable_curve_capped(
                        at(from.position),
                        outward(&from, &to),
                        at(to.position),
                        outward(&to, &from),
                        cable.handles,
                    );
                    let mut builder = PathBuilder::stroke(px(CABLE_STROKE));
                    match cable.style.pattern(CABLE_STROKE, 1.) {
                        None => {
                            for (i, p) in curve.enumerate() {
                                if i == 0 {
                                    builder.move_to(point(px(p.x), px(p.y)));
                                } else {
                                    builder.line_to(point(px(p.x), px(p.y)));
                                }
                            }
                        }
                        Some(pattern) => core::dashes(
                            curve.map(|p| (p.x, p.y)),
                            pattern,
                            0.,
                            &mut scratch,
                            |dash| {
                                if let Some((first, rest)) = dash.split_first() {
                                    builder.move_to(point(px(first.0), px(first.1)));
                                    for p in rest {
                                        builder.line_to(point(px(p.0), px(p.1)));
                                    }
                                }
                            },
                        ),
                    }
                    let ink = palette
                        .as_ref()
                        .zip(cable.color)
                        .and_then(|(palette, c)| {
                            crate::ink::resolve(palette, Some(crate::ink::hue(c)))
                        })
                        .unwrap_or(dimmed);
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, ink.to_hsla());
                    }
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }
}
