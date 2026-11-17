//! guiraffe-mermaid: native Mermaid diagrams with pan, zoom and fit.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};
use gpui_kit::component::{Sizable, button::Button};
use guiraffe_components::Text;
use guiraffe_core::{GuiraffeTheme, Scheme, Size};
use guiraffe_mermaid::{MermaidView, MermaidViewState};

use super::{column, example};
use crate::catalog::Entry;

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "mermaid",
        name: "Mermaid diagrams",
        summary: "Flowcharts, sequence and state diagrams drawn off the UI thread and themed to the scheme. Drag to pan, scroll to zoom, double-click to fit.",
        covers: &["MermaidView", "MermaidViewState"],
        demo: mermaid,
    }]
}

const FLOW: &str = "flowchart LR
  PR[Pull request] --> CI{CI green?}
  CI -->|yes| Review[Code review]
  CI -->|no| Fix[Fix build]
  Fix --> CI
  Review --> Merge[Merge to release]
  Merge --> Ship((Ship))";

const SEQUENCE: &str = "sequenceDiagram
  participant U as User
  participant A as App
  participant S as Server
  U->>A: Sign in
  A->>S: POST /session
  S-->>A: 200 token
  A-->>U: Dashboard";

struct MermaidDemo {
    flow: Entity<MermaidViewState>,
    sequence: Entity<MermaidViewState>,
    /// Scheme each diagram was last drawn for (`None` until it has been laid out).
    drawn: [Option<bool>; 2],
    _observe: [gpui_kit::Subscription; 2],
}

fn mermaid(_: &mut Window, cx: &mut App) -> AnyView {
    let flow = MermaidView::create_state("showcase-mermaid-flow", cx);
    let sequence = MermaidView::create_state("showcase-mermaid-seq", cx);
    cx.new(|cx| MermaidDemo {
        _observe: [
            cx.observe(&flow, |_, _, cx| cx.notify()),
            cx.observe(&sequence, |_, _, cx| cx.notify()),
        ],
        flow,
        sequence,
        drawn: [None; 2],
    })
    .into()
}

impl Render for MermaidDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Diagrams are rasterized for the scheme and the laid-out width, so the source is set
        // once the view has bounds (a request before layout rasterizes at a 100px floor and
        // is not redrawn on resize), and again whenever the scheme changes.
        let dark = GuiraffeTheme::global(cx).scheme == Scheme::Dark;
        for (i, (state, source)) in [(self.flow.clone(), FLOW), (self.sequence.clone(), SEQUENCE)]
            .into_iter()
            .enumerate()
        {
            let laid_out = state.read(cx).viewport().bounds().0 > 0.0;
            if laid_out && self.drawn[i] != Some(dark) {
                self.drawn[i] = Some(dark);
                state.update(cx, |s, cx| s.set_source(source.into(), dark, cx));
            }
        }
        type ZoomFn = fn(&mut MermaidViewState, &mut Context<MermaidViewState>);
        let zoom = |id: &'static str, label: &'static str, f: ZoomFn| {
            let state = self.flow.clone();
            Button::new(id)
                .label(label)
                .small()
                .on_click(move |_, _, cx| state.update(cx, f))
        };
        let flow = self.flow.read(cx);
        let caption = format!(
            "zoom {:.0}% · status {:?}",
            flow.viewport().zoom() * 100.,
            flow.status()
        );
        column([
            example(
                "Zoom controls: zoom_in / zoom_out / fit / reset_zoom",
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(zoom("mm-in", "Zoom in", |s, cx| s.zoom_in(cx)))
                    .child(zoom("mm-out", "Zoom out", |s, cx| s.zoom_out(cx)))
                    .child(zoom("mm-fit", "Fit", |s, cx| s.fit(cx)))
                    .child(zoom("mm-reset", "100%", |s, cx| s.reset_zoom(cx)))
                    .child(Text::new(caption).size(Size::Sm).dimmed(true)),
            ),
            example(
                "Flowchart: release pipeline",
                div()
                    .h(px(240.))
                    .child(MermaidView::new("showcase-mermaid-flow").state(&self.flow)),
            ),
            example(
                "Sequence: sign-in",
                div()
                    .h(px(280.))
                    .child(MermaidView::new("showcase-mermaid-seq").state(&self.sequence)),
            ),
        ])
    }
}
