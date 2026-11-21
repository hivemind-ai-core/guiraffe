//! Cold 500-node Mermaid preview and refinement timings on the real GPUI scheduler.
//!
//! Run with `cargo run --release -p guiraffe-mermaid --example first_frame`.

use std::fmt::Write as _;
use std::time::Instant;

use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Point, Render,
    Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use guiraffe_mermaid::{MermaidView, MermaidViewState, view::Status};

struct Probe {
    diagram: Entity<MermaidViewState>,
    started: Instant,
    preview_seen: bool,
    finished: bool,
    _observe: gpui_kit::Subscription,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let diagram = self.diagram.read(cx);
        if !self.preview_seen && diagram.status() == &Status::Drawing && diagram.image().is_some() {
            self.preview_seen = true;
            println!("first useful frame: {:?}", self.started.elapsed());
        }
        if !self.finished {
            match diagram.status() {
                Status::Ready => {
                    self.finished = true;
                    assert!(
                        self.preview_seen,
                        "refinement landed before a preview frame"
                    );
                    println!("refined frame: {:?}", self.started.elapsed());
                    cx.defer(|cx| cx.quit());
                }
                Status::Failed(error) => panic!("Mermaid render failed: {error}"),
                _ => {}
            }
        }
        div()
            .size_full()
            .child(MermaidView::new("mermaid-cold-mount").state(&self.diagram))
    }
}

fn main() {
    let mut source = String::from("flowchart TD\n");
    for i in 0..500 {
        writeln!(source, "  n{i}[Node {i}]").expect("write node");
    }
    for i in 1..500 {
        writeln!(source, "  n{} --> n{i}", (i - 1) / 3).expect("write tree edge");
        if i % 10 == 0 {
            writeln!(source, "  n{i} -.-> n{}", (i * 7) % 500).expect("write cross link");
        }
    }

    gpui_kit::application().run(move |cx: &mut App| {
        guiraffe_core::init(cx);
        let bounds = Bounds::new(Point::default(), size(px(1280.0), px(800.0)));
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            move |_, cx| {
                let diagram = MermaidView::create_state("mermaid-cold-mount", cx);
                let started = Instant::now();
                diagram.update(cx, |state, cx| state.set_source(source.into(), false, cx));
                cx.new(|cx| Probe {
                    _observe: cx.observe(&diagram, |_, _, cx| cx.notify()),
                    diagram,
                    started,
                    preview_seen: false,
                    finished: false,
                })
            },
        )
        .expect("open probe window");
    });
}
