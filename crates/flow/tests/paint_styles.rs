//! Flow paint styles (#160): coloured edges, typed port rings (hollow until connected),
//! node frames (rounded, pill, none), a themed grid and no markers, with a coloured cable,
//! in both schemes; and ports keep their hit targets and anchors.

use gpui_kit::{
    AnyView, AppContext, Context, Entity, IntoElement, ParentElement, Render, StyleRefinement,
    Styled, TestAppContext, Window, div, px, size, test::TestWindowExt,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, apply};
use guiraffe_flow::{
    FlowCanvas, FlowOptions, FlowState, GridStyle,
    cables::{Cable, CableAnchorExt, CableEnd, CableLayer, Cables},
    core::{
        Edge, Editor, Graph, Handle, HandleKind, Hue, Node, NodeFrame, Point, PortRing, Side,
        StrokeStyle,
    },
};
use guiraffe_state::Store;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

/// A synth chain: audio ports in teal, modulation in orange; one modulation input is left
/// unconnected, so its ring is hollow.
pub fn graph() -> Graph {
    let ports = |node: Node| {
        let mut node = node;
        node.handles = vec![
            Handle::new("in", HandleKind::Target, Side::Left)
                .with_color(Hue::Teal)
                .with_ring(PortRing::Ring)
                .with_tooltip("Stereo in"),
            Handle::new("mod", HandleKind::Target, Side::Top)
                .with_color(Hue::Orange)
                .with_ring(PortRing::Ring)
                .with_tooltip("Modulation, -1..1"),
            Handle::new("out", HandleKind::Source, Side::Right)
                .with_color(Hue::Teal)
                .with_ring(PortRing::Ring),
        ]
        .into();
        node
    };
    let mut lfo = Node::new(1, "LFO", Point::new(40., 40.));
    lfo.size = Point::new(96., 36.);
    lfo.frame = NodeFrame::Pill;
    lfo.handles = vec![
        Handle::new("out", HandleKind::Source, Side::Right)
            .with_color(Hue::Orange)
            .with_ring(PortRing::Ring),
    ]
    .into();
    let osc = ports(Node::new(2, "Osc", Point::new(40., 170.)));
    let filter = ports(Node::new(3, "Filter", Point::new(280., 170.)));
    let mut out = Node::new(4, "Out", Point::new(520., 190.));
    out.size = Point::new(60., 36.);
    out.frame = NodeFrame::None;
    out.handles = vec![
        Handle::new("in", HandleKind::Target, Side::Left)
            .with_color(Hue::Teal)
            .with_ring(PortRing::Ring),
    ]
    .into();
    let edge = |id, (s, sh): (u64, &str), (t, th): (u64, &str), color: Hue| {
        let mut edge = Edge::new(id, s, t);
        edge.source.handle = sh.into();
        edge.target.handle = th.into();
        edge.color = Some(color);
        edge
    };
    let mut graph = Graph::from_parts(
        [lfo, osc, filter, out],
        [
            edge(1, (2, "out"), (3, "in"), Hue::Teal),
            edge(2, (3, "out"), (4, "in"), Hue::Teal),
            {
                let mut e = edge(3, (1, "out"), (3, "mod"), Hue::Orange);
                e.stroke = Some(StrokeStyle::DASHED);
                e
            },
        ],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    graph.hide_markers = true;
    graph
}

fn options() -> FlowOptions {
    FlowOptions {
        toolbar: false,
        minimap: false,
        controls: false,
        fit_on_init: false,
        grid: GridStyle {
            color: Some(ColorName::Gray),
            step: Some(24.),
            dot: 2.,
        },
        ..FlowOptions::default()
    }
}

struct Patch {
    flow: Entity<FlowState>,
    layer: Entity<CableLayer>,
}

impl Render for Patch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .child(
                div()
                    .w(px(80.))
                    .h_full()
                    .bg(surfaces.body.to_hsla())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(24.))
                            .rounded_full()
                            .bg(surfaces.border.to_hsla())
                            .cable_anchor("macro")
                            .side(Side::Right),
                    ),
            )
            .child(div().flex_1().h_full().child(FlowCanvas::new(&self.flow)))
            .child(
                AnyView::from(self.layer.clone())
                    .cached(StyleRefinement::default().absolute().size_full()),
            )
    }
}

#[test]
fn typed_ports_coloured_edges_and_cables_in_both_schemes() -> anyhow::Result<()> {
    let editor = Editor::new(graph())?;
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "paint-styles-light"),
        (Scheme::Dark, "paint-styles-dark"),
    ] {
        let editor = editor.clone();
        let image = Snapshotter::new()?
            .with_setup(move |cx| {
                guiraffe_core::init(cx);
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            })
            .render(size(px(720.), px(320.)), move |_, cx| {
                let flow = cx.new(|cx| FlowState::from_editor(editor, cx).options(options()));
                let mut cables = Cables::new();
                cables.push(
                    1,
                    Cable::new(
                        CableEnd::element("macro"),
                        CableEnd::port("graph", 2, "mod"),
                    )
                    .style(StrokeStyle::DOTTED)
                    .color(ColorName::Grape),
                );
                let store = Store::new(cables, cx);
                let layer = cx.new(|cx| {
                    let mut layer = CableLayer::new(&store, cx);
                    layer.add_flow("graph", &flow, cx);
                    layer
                });
                cx.new(|_| Patch { flow, layer })
            })?;
        assert_snapshot(name, &image, &config)?;
    }
    Ok(())
}

#[gpui_kit::test]
fn styled_ports_keep_their_hit_targets_and_anchors(cx: &mut TestAppContext) {
    cx.update(guiraffe_core::init);
    let open = |cx: &mut TestAppContext, graph: Graph| {
        let (view, vcx) = cx.add_window_view(move |_, cx| {
            FlowState::new(graph, cx)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .options(options())
        });
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let painted = window
                .within(("flow-node", 3u64))
                .find("handle-in")
                .bounds();
            let anchor = view
                .read(cx)
                .port_anchor(3, "in", cx)
                .unwrap_or_else(|| panic!("anchor"));
            (painted, anchor.position)
        })
    };
    let styled = open(cx, graph());
    // The same graph with plain ports.
    let mut plain = graph();
    for id in plain.nodes.keys().copied().collect::<Vec<_>>() {
        let Some(node) = plain.nodes.get_mut(&id) else {
            continue;
        };
        for handle in node.handles.iter_mut() {
            handle.color = None;
            handle.ring = PortRing::Dot;
            handle.tooltip = None;
        }
    }
    let plain = open(cx, plain);
    assert_eq!(styled.0, plain.0, "a ring has the dot's hit target");
    assert_eq!(styled.1, plain.1, "and its anchor");
    assert_eq!(
        styled.0.center(),
        styled.1,
        "the anchor is the painted centre"
    );
}
