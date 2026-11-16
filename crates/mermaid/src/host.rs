//! Host used by the interaction tests: a diagram filling the window.

use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window, div, px,
};

use crate::{MermaidView, MermaidViewState};

pub struct Host {
    pub diagram: Entity<MermaidViewState>,
    source: String,
}

impl Host {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let diagram = MermaidView::create_state("diagram", cx);
        let mut host = Self {
            diagram,
            source: "flowchart LR\n  Start --> Finish".to_string(),
        };
        host.request_redraw(cx);
        host
    }

    /// Set the diagram source and start a render for it.
    pub fn set_source(&mut self, source: impl Into<String>, cx: &mut Context<Self>) {
        self.source = source.into();
        self.request_redraw(cx);
    }

    /// Start a render for the current source and scheme.
    pub fn request_redraw(&mut self, cx: &mut Context<Self>) {
        let diagram = self.diagram.clone();
        let source = self.source.clone();
        diagram.update(cx, |state, cx| {
            state.set_source(source.into(), state_is_dark(cx), cx)
        });
        cx.notify();
    }
}

fn state_is_dark(cx: &gpui_kit::App) -> bool {
    guiraffe_core::GuiraffeTheme::global(cx).scheme == guiraffe_core::Scheme::Dark
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let source: gpui_kit::SharedString = self.source.clone().into();
        let is_dark = state_is_dark(cx);
        self.diagram
            .update(cx, |state, cx| state.set_source(source, is_dark, cx));
        div().size_full().p(px(8.0)).child(
            div()
                .size_full()
                .child(MermaidView::new("diagram").state(&self.diagram)),
        )
    }
}
