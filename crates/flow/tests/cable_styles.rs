//! Edge line patterns (issue 150): solid, dashed and dotted edges at zoom 1 and 1.5 in both
//! themes. The pattern is laid out in world units, so it scales with the zoom.
use gpui_kit::{AppContext, IntoElement, ParentElement, Render, Styled, Window, div, px, size};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_flow::{
    FlowCanvas, FlowOptions, FlowState,
    core::{Edge, Graph, Node, Point, StrokeStyle, Viewport},
};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct View(gpui_kit::Entity<FlowState>);

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div().size_full().child(FlowCanvas::new(&self.0))
    }
}

fn graph() -> anyhow::Result<Graph> {
    let styles = [StrokeStyle::Solid, StrokeStyle::DASHED, StrokeStyle::DOTTED];
    let nodes = (0..3u64).flat_map(|row| {
        let y = 20. + row as f32 * 100.;
        [
            Node::new(row * 2, format!("From {row}"), Point::new(20., y)),
            Node::new(row * 2 + 1, format!("To {row}"), Point::new(380., y + 20.)),
        ]
    });
    let mut graph = Graph::from_parts(
        nodes,
        styles.iter().enumerate().map(|(row, style)| Edge {
            stroke: Some(*style),
            ..Edge::new(row as u64 + 1, row as u64 * 2, row as u64 * 2 + 1)
        }),
    )?;
    // The per-graph default applies to edges without their own style (none here).
    graph.edge_stroke = StrokeStyle::Solid;
    Ok(graph)
}

#[test]
fn solid_dashed_and_dotted_at_two_zooms_in_both_themes() -> anyhow::Result<()> {
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, theme) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
        for (zoom, pan, label) in [
            (1.0_f32, Point::new(0., 0.), "1x"),
            (1.5, Point::new(-225., -15.), "1.5x"),
        ] {
            let graph = graph()?;
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
                .render(size(px(600.), px(560.)), move |_, cx| {
                    let state = cx.new(|cx| {
                        let mut state = FlowState::new(graph, cx)
                            .unwrap_or_else(|e| panic!("{e:?}"))
                            .options(FlowOptions {
                                toolbar: false,
                                minimap: false,
                                controls: false,
                                fit_on_init: false,
                                ..FlowOptions::default()
                            });
                        let _ = state.set_viewport(Viewport { pan, zoom }, cx);
                        state
                    });
                    cx.new(|_| View(state))
                })?;
            assert_snapshot(&format!("cables-{theme}-{label}"), &image, &config)?;
        }
    }
    Ok(())
}
