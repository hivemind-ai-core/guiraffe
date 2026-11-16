//! Check rendered diagram ink in the viewport, not only a Ready state or allocated raster.
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_markdown::{MarkdownState, MarkdownView};
use guiraffe_mermaid::{MermaidView, MermaidViewState, view::Status};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use std::{cell::Cell, rc::Rc};

const SOURCE: &str = "flowchart LR\n  A[Source] --> B[Layout]\n  B --> C[Visible]";
struct Standalone {
    state: Entity<MermaidViewState>,
    ready: Rc<Cell<bool>>,
    height: Rc<Cell<f32>>,
    _observe: gpui_kit::Subscription,
}
impl Render for Standalone {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        self.ready.set(state.status() == &Status::Ready);
        self.height.set(state.viewport().bounds().1);
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(surfaces.body.to_hsla())
            .child(MermaidView::new("visible-diagram").state(&self.state))
    }
}
struct Embedded(Entity<MarkdownState>);
impl Render for Embedded {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .child(MarkdownView::new("embedded").state(&self.0))
    }
}
#[test]
fn standalone_and_markdown_paint_diagram_pixels_in_both_themes()
-> Result<(), Box<dyn std::error::Error>> {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let dark = scheme == Scheme::Dark;
        let background = GuiraffeTheme {
            scheme,
            ..Default::default()
        }
        .surfaces()
        .body;
        let mut snap = Snapshotter::new()?.with_setup(move |cx| {
            guiraffe_core::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let ready = Rc::new(Cell::new(false));
        let observed_ready = ready.clone();
        let height = Rc::new(Cell::new(0.));
        let observed_height = height.clone();
        let standalone = snap.render(size(px(640.), px(420.)), move |_, cx| {
            let state = MermaidView::create_state("visible-diagram", cx);
            state.update(cx, |s, cx| s.set_source(SOURCE.into(), dark, cx));
            cx.new(|cx| Standalone {
                _observe: cx.observe(&state, |_, _, cx| cx.notify()),
                state,
                ready: observed_ready,
                height: observed_height,
            })
        })?;
        assert!(ready.get(), "render completed before the capture");
        assert!(
            height.get() > 350.,
            "diagram body has real height, got {}",
            height.get()
        );
        // Exclude the footer; an opaque page or Ready/status caption cannot satisfy this.
        let ink = standalone
            .enumerate_pixels()
            .filter(|(_, y, p)| {
                *y < standalone.height() - 60
                    && p.0[..3]
                        .iter()
                        .zip([background.r, background.g, background.b])
                        .any(|(actual, expected)| actual.abs_diff(expected) > 20)
            })
            .count();
        assert!(
            ink > 1000,
            "standalone diagram has no visible ink ({ink} pixels)"
        );
        assert_snapshot(
            if dark {
                "standalone-dark"
            } else {
                "standalone-light"
            },
            &standalone,
            &cfg,
        )?;
        let embedded = snap.render(size(px(640.), px(420.)), |_, cx| {
            let state = cx.new(|cx| MarkdownState::new(format!("```mermaid\n{SOURCE}\n```\n"), cx));
            cx.new(|_| Embedded(state))
        })?;
        // The middle of the 260px diagram block excludes the status footer and the
        // top-left Drawing placeholder. This must be actual diagram geometry/text.
        let ink = embedded
            .enumerate_pixels()
            .filter(|(_, y, p)| {
                *y >= 140
                    && *y < 420
                    && p.0[..3]
                        .iter()
                        .zip([background.r, background.g, background.b])
                        .any(|(actual, expected)| actual.abs_diff(expected) > 20)
            })
            .count();
        assert!(
            ink > 1000,
            "embedded diagram has no visible ink ({ink} pixels)"
        );
        assert_snapshot(
            if dark {
                "markdown-dark"
            } else {
                "markdown-light"
            },
            &embedded,
            &cfg,
        )?;
    }
    Ok(())
}
