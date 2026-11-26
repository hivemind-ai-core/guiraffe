//! Cables from side-panel knobs to flow ports and between knobs, in both schemes (#152).

use gpui_kit::{
    AnyView, AppContext, Context, Entity, IntoElement, ParentElement, Render, StyleRefinement,
    Styled, Window, div, px, size,
};
use guiraffe_components::controls::KnobControl;
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_flow::{
    FlowCanvas, FlowOptions, FlowState,
    cables::{Cable, CableAnchorExt, CableEnd, CableLayer, Cables},
    core::{Edge, Editor, Graph, Node, Point, Side, StrokeStyle},
};
use guiraffe_state::Store;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Patch {
    flow: Entity<FlowState>,
    knobs: [Entity<KnobControl>; 2],
    layer: Entity<CableLayer>,
}

impl Render for Patch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let knob = |id: &'static str, knob: &Entity<KnobControl>| {
            div()
                .size(px(56.))
                .child(knob.clone())
                .cable_anchor(id)
                .side(Side::Right)
        };
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .child(
                div()
                    .w(px(120.))
                    .h_full()
                    .bg(GuiraffeTheme::global(cx).surfaces().body.to_hsla())
                    .flex()
                    .flex_col()
                    .justify_around()
                    .items_center()
                    .child(knob("cutoff", &self.knobs[0]))
                    .child(knob("resonance", &self.knobs[1])),
            )
            .child(div().flex_1().h_full().child(FlowCanvas::new(&self.flow)))
            .child(
                AnyView::from(self.layer.clone())
                    .cached(StyleRefinement::default().absolute().size_full()),
            )
    }
}

#[test]
fn cables_in_both_schemes() -> anyhow::Result<()> {
    let graph = Graph::from_parts(
        [
            Node::new(1, "Filter", Point::new(40., 50.)),
            Node::new(2, "Output", Point::new(260., 170.)),
        ],
        [Edge::new(1, 1, 2)],
    )?;
    let editor = Editor::new(graph)?;
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "cables-light"),
        (Scheme::Dark, "cables-dark"),
    ] {
        let editor = editor.clone();
        let image = Snapshotter::new()?
            .with_setup(move |cx| {
                guiraffe_components::init(cx);
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            })
            .render(size(px(560.), px(320.)), move |window, cx| {
                let flow = cx.new(|cx| {
                    FlowState::from_editor(editor, cx).options(FlowOptions {
                        toolbar: false,
                        minimap: false,
                        controls: false,
                        fit_on_init: false,
                        ..FlowOptions::default()
                    })
                });
                let mut cables = Cables::new();
                for (id, cable) in [
                    Cable::new(
                        CableEnd::element("cutoff"),
                        CableEnd::port("graph", 1, "in"),
                    ),
                    Cable::new(
                        CableEnd::element("resonance"),
                        CableEnd::port("graph", 2, "in"),
                    )
                    .style(StrokeStyle::DASHED),
                    Cable::new(CableEnd::element("cutoff"), CableEnd::element("resonance"))
                        .style(StrokeStyle::DOTTED),
                ]
                .into_iter()
                .enumerate()
                {
                    cables.push(id as u64, cable);
                }
                let knobs = [
                    KnobControl::new().label("Cutoff").build(window, cx),
                    KnobControl::new().label("Res").build(window, cx),
                ];
                let store = Store::new(cables, cx);
                let layer = cx.new(|cx| {
                    let mut layer = CableLayer::new(&store, cx);
                    layer.add_flow("graph", &flow, cx);
                    layer
                });
                cx.new(|_| Patch { flow, knobs, layer })
            })?;
        assert_snapshot(name, &image, &config)?;
    }
    Ok(())
}
