//! GPUI interaction tests for [`MarkdownView`](crate::MarkdownView).

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, IntoElement, ParentElement as _, Point,
    Render, Styled as _, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
};

use crate::{MarkdownState, MarkdownView};

const REPLY: &str = "\
# Release notes

Intro paragraph with a [link](https://example.org).

## Tasks

- [ ] write docs
- [x] ship it

```rust
fn main() {}
```
";

/// The collection observed by the host's callbacks.
#[derive(Default)]
struct Collected {
    links: Vec<String>,
    toggles: Vec<(bool, std::ops::Range<usize>)>,
}

struct Host {
    markdown: Entity<MarkdownState>,
    source: String,
    collected: std::sync::Arc<std::sync::Mutex<Collected>>,
}

impl Host {
    fn new(cx: &mut gpui_kit::Context<Self>) -> Self {
        Self {
            markdown: cx.new(|cx| MarkdownState::new(REPLY, cx)),
            source: REPLY.to_string(),
            collected: Default::default(),
        }
    }

    fn state(&self) -> &Entity<MarkdownState> {
        &self.markdown
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let links = self.collected.clone();
        let toggles = self.collected.clone();
        div().size_full().p(px(8.0)).child(
            MarkdownView::new("markdown")
                .state(&self.markdown)
                .scrollable(true)
                .on_link_click(move |url, _, _| lock(&links).links.push(url.to_string()))
                .on_task_toggle(move |checked, range, _, _| {
                    lock(&toggles).toggles.push((checked, range))
                }),
        )
    }
}

fn lock(collected: &std::sync::Mutex<Collected>) -> std::sync::MutexGuard<'_, Collected> {
    collected.lock().unwrap_or_else(|error| error.into_inner())
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Host>) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Point::default(),
                    size(px(800.0), px(900.0)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(Host::new),
        )
        .expect("window opens")
    })
}

fn settle(cx: &mut TestAppContext) {
    cx.executor().run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.executor().run_until_parked();
}

#[gpui_kit::test]
fn a_document_renders_its_text_and_analysis(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).state().read(cx);
        assert_eq!(state.headings().len(), 2);
        assert_eq!(state.headings()[0].text, "Release notes");
        assert_eq!(state.headings()[1].anchor, "tasks");
        assert_eq!(state.document().tasks.len(), 2);
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

#[gpui_kit::test]
fn appended_chunks_extend_the_document_without_replacing_the_text_view(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    let before = cx.update(|cx| host.read(cx).state().read(cx).text().entity_id());
    cx.update(|cx| {
        host.update(cx, |host, cx| {
            host.markdown.update(cx, |state, cx| {
                state.append("\n\nStreamed paragraph arrives ", cx);
                state.append("one chunk at a time.", cx);
            });
        });
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).state().read(cx);
        assert_eq!(
            before,
            state.text().entity_id(),
            "appending keeps the text view, so its layout is retained"
        );
        assert!(state.source().ends_with("one chunk at a time."));
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

#[gpui_kit::test]
fn a_mid_document_rewrite_replaces_the_source(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        host.update(cx, |host, cx| {
            let rewritten = host.source.replace("Intro paragraph", "Summary");
            host.markdown
                .update(cx, |state, cx| state.set_source(rewritten, cx));
        });
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).state().read(cx);
        assert!(state.source().contains("Summary"));
        assert!(!state.source().contains("Intro paragraph"));
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

#[gpui_kit::test]
fn streaming_headings_keep_the_outline_current(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        host.update(cx, |host, cx| {
            host.markdown
                .update(cx, |state, cx| state.append("\n\n## Diagram\n\n", cx));
        });
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).state().read(cx);
        assert_eq!(
            state.headings().last().map(|h| h.text.as_str()),
            Some("Diagram")
        );
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

#[gpui_kit::test]
fn revealing_a_heading_scrolls_its_rendered_line_into_view(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
    let revealed = cx.update(|cx| {
        host.update(cx, |host, cx| {
            let heading = host
                .markdown
                .read(cx)
                .headings()
                .last()
                .cloned()
                .expect("a last heading");
            host.markdown
                .update(cx, |state, cx| state.reveal_heading(&heading, cx))
        })
    });
    assert!(revealed, "the heading maps into the rendered document");
}

#[gpui_kit::test]
fn an_empty_document_still_renders(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    cx.update(|cx| {
        host.update(cx, |host, cx| {
            host.markdown
                .update(cx, |state, cx| state.set_source("", cx));
        });
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).state().read(cx);
        assert!(state.headings().is_empty());
        assert!(state.document().tasks.is_empty());
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

#[cfg(feature = "mermaid")]
#[gpui_kit::test]
fn a_mermaid_fence_becomes_a_diagram_block(cx: &mut TestAppContext) {
    let (handle, host) = open(cx);
    settle(cx);
    let source = "Before\n\n```mermaid\nflowchart LR\n  A --> B\n```\n\nAfter";
    cx.update(|cx| {
        host.update(cx, |host, cx| {
            host.markdown
                .update(cx, |state, cx| state.set_source(source, cx));
        });
    });
    settle(cx);
    cx.update(|cx| {
        let state = host.read(cx).state().read(cx);
        assert_eq!(state.document().fences.len(), 1);
        assert_eq!(state.document().fences[0].source, "flowchart LR\n  A --> B");
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
}

/// A non-scrolling document, the shape an item page has, where every block is
/// laid out (a scrollable view virtualizes off-screen ones).
#[cfg(feature = "mermaid")]
struct Static {
    markdown: Entity<MarkdownState>,
}

#[cfg(feature = "mermaid")]
impl Render for Static {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div().size_full().child(
            MarkdownView::new("static")
                .state(&self.markdown)
                .scrollable(false),
        )
    }
}

#[cfg(feature = "mermaid")]
#[gpui_kit::test]
fn a_mermaid_fence_paints_a_diagram_in_place_of_its_code_block(cx: &mut TestAppContext) {
    let source = "Before\n\n```mermaid\nflowchart LR\n  A --> B\n```\n\nAfter";
    cx.update(gpui_kit::init);
    let (handle, _view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Point::default(),
                    size(px(800.0), px(900.0)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let markdown = cx.new(|cx| MarkdownState::new(source, cx));
                cx.new(|_| Static { markdown })
            },
        )
        .expect("window opens")
    });
    settle(cx);
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("frame renders");
    let id: &'static str = Box::leak(
        format!(
            "mermaid-{}",
            crate::document::stable_hash("flowchart LR\n  A --> B")
        )
        .into_boxed_str(),
    );
    let painted = cx
        .update_window(handle, |_, window, _| window.try_find(id).is_some())
        .expect("window");
    assert!(painted, "diagram block {id} painted into the document");
}
