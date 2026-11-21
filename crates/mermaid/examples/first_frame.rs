//! Cold 500-node Mermaid preview and refinement timings on the real GPUI scheduler.
//!
//! Run with `cargo run --release -p guiraffe-mermaid --example first_frame`.
//! Pass `-- --window-only` for a process-memory control without diagram data.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Point, Render,
    Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use guiraffe_mermaid::{MermaidView, MermaidViewState, view::Status};

struct Probe {
    diagram: Entity<MermaidViewState>,
    started: Instant,
    window_only: bool,
    preview_seen: bool,
    finished: bool,
    _observe: gpui_kit::Subscription,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let diagram = self.diagram.read(cx);
        if !self.preview_seen && diagram.has_preview() {
            self.preview_seen = true;
            let frame = if diagram.image().is_none() {
                "native preview"
            } else {
                "raster preview"
            };
            println!("first useful frame: {:?} ({frame})", self.started.elapsed());
        }
        if !self.finished {
            match diagram.status() {
                Status::Empty if self.window_only => {
                    self.finished = true;
                    println!("window-only frame: {:?}", self.started.elapsed());
                    let timer = cx.background_executor().timer(Duration::from_millis(50));
                    cx.spawn(async move |_, cx| {
                        timer.await;
                        cx.update(|cx| cx.quit());
                    })
                    .detach();
                }
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
        let window = div().size_full();
        if self.window_only {
            window
        } else {
            window.child(MermaidView::new("mermaid-cold-mount").state(&self.diagram))
        }
    }
}

fn main() {
    let window_only = std::env::args().any(|arg| arg == "--window-only");
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
                if !window_only {
                    diagram.update(cx, |state, cx| state.set_source(source.into(), false, cx));
                }
                cx.new(|cx| Probe {
                    _observe: cx.observe(&diagram, |_, _, cx| cx.notify()),
                    diagram,
                    started,
                    window_only,
                    preview_seen: false,
                    finished: false,
                })
            },
        )
        .expect("open probe window");
    });
}
