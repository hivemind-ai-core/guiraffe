use gpui_kit::{AppContext, IntoElement, ParentElement, Render, Styled, Window, div, px, size};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_flow::{
    FlowCanvas, FlowOptions, FlowState,
    core::{
        AutoLayout, Edge, Editor, Graph, LayoutCancellation, LayoutDirection, LayoutEngine,
        LayoutOptions, Node, Point,
    },
};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct LayoutView(gpui_kit::Entity<FlowState>);

impl Render for LayoutView {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div().size_full().child(FlowCanvas::new(&self.0))
    }
}

#[test]
fn layered_port_routes_in_both_schemes() -> anyhow::Result<()> {
    let graph = Graph::from_parts(
        [
            "Webhook", "Validate", "Enrich", "Archive", "Notify", "Audit",
        ]
        .into_iter()
        .enumerate()
        .map(|(id, label)| Node::new(id as u64, label, Point::default())),
        [(0, 3), (1, 2), (2, 4), (3, 5), (2, 5)]
            .into_iter()
            .enumerate()
            .map(|(id, (a, b))| Edge::new(id as u64 + 1, a, b)),
    )?;
    let result = AutoLayout::Layered.layout(
        &graph,
        &LayoutOptions {
            direction: LayoutDirection::TopToBottom,
            ..LayoutOptions::default()
        },
        &LayoutCancellation::default(),
    )?;
    let mut editor = Editor::new(graph)?;
    editor.apply_layout(&result)?;
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "layout-light"),
        (Scheme::Dark, "layout-dark"),
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
            .render(size(px(680.), px(420.)), move |_, cx| {
                let state = cx.new(|cx| {
                    FlowState::from_editor(editor, cx).options(FlowOptions {
                        toolbar: false,
                        ..FlowOptions::default()
                    })
                });
                cx.new(|_| LayoutView(state))
            })?;
        assert_snapshot(name, &image, &config)?;
    }
    Ok(())
}
