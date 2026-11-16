//! A window that streams Markdown into the viewer, the way an agent chat does.
//!
//! Run with `cargo run -p guiraffe-markdown --features mermaid --example viewer`.

use std::time::Duration;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Point, Render,
    Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, Size};
use guiraffe_markdown::{MarkdownState, MarkdownView, document::apply_task_toggle};

/// The reply the example types out, one chunk at a time.
const REPLY: &str = "\
# Streaming Markdown

Every chunk below is appended to the same text view, so nothing that has
already settled is laid out again.

## Features

- [x] GFM tables, task lists, footnotes and autolinks
- [x] Syntax highlighted code with a copy button
- [x] Heading anchors and a table of contents
- [ ] Streaming, incremental updates

| feature | where |
|---|---|
| streaming | `MarkdownState::append` |
| diagrams | `MarkdownView` + `mermaid` feature |

```rust
fn reply() -> &'static str {
    \"one token at a time\"
}
```

## A diagram

```mermaid
flowchart LR
  Chat[Agent chat] --> Append[append chunk]
  Append --> Settle{settled?}
  Settle -->|yes| Cache[keep layout]
  Settle -->|no| Draw[draw diagram]
```

> Streaming keeps the reader's place: the text view keeps its selection and
> only the open block is measured again.
";

struct Gallery {
    markdown: Entity<MarkdownState>,
    /// Byte offset streamed so far.
    streamed: usize,
    streaming: bool,
    dark: bool,
    last_link: Option<String>,
}

impl Gallery {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut gallery = Self {
            markdown: cx.new(|cx| MarkdownState::new("", cx)),
            streamed: 0,
            streaming: false,
            dark: false,
            last_link: None,
        };
        gallery.stream(cx);
        gallery
    }

    /// Stream the next chunk.
    fn stream(&mut self, cx: &mut Context<Self>) {
        // Chunks end at a word boundary, like a token stream does.
        let rest = &REPLY[self.streamed..];
        if rest.is_empty() {
            self.streaming = false;
            cx.notify();
            return;
        }
        let take = rest
            .char_indices()
            .take_while(|(index, _)| *index < 24)
            .last()
            .map_or(rest.len(), |(index, c)| index + c.len_utf8());
        let chunk = rest[..take].to_string();
        self.streamed += take;
        self.markdown
            .update(cx, |state, cx| state.append(&chunk, cx));
        if self.streaming {
            let this = cx.entity().downgrade();
            cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                this.update(cx, |this, cx| this.stream(cx)).ok();
            })
            .detach();
        }
        cx.notify();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        self.streamed = 0;
        self.streaming = true;
        self.markdown
            .update(cx, |state, cx| state.set_source("", cx));
        self.stream(cx);
    }

    fn toggle_scheme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        guiraffe_core::set_scheme(
            if self.dark {
                Scheme::Dark
            } else {
                Scheme::Light
            },
            cx,
        );
        window.refresh();
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let headings = self.markdown.read(cx).headings().to_vec();
        let progress = if REPLY.is_empty() {
            0
        } else {
            (self.streamed * 100) / REPLY.len()
        };

        let mut content = div().flex().flex_col().gap(px(Size::Xs.spacing()));
        for heading in headings.iter().take(12) {
            let indent = usize::from(heading.depth.saturating_sub(1));
            content = content.child(
                div()
                    .pl(px(Size::Sm.spacing() * indent as f32))
                    .text_size(px(12.0))
                    .text_color(surfaces.dimmed.to_hsla())
                    .child(heading.text.clone()),
            );
        }

        let last_link = self.last_link.clone();

        div()
            .flex()
            .size_full()
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .w(px(220.0))
                    .h_full()
                    .p(px(Size::Sm.spacing()))
                    .gap(px(Size::Xs.spacing()))
                    .border_r(px(1.0))
                    .border_color(surfaces.border.to_hsla())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(surfaces.dimmed.to_hsla())
                            .child("TABLE OF CONTENTS"),
                    )
                    .child(content)
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(surfaces.dimmed.to_hsla())
                            .child(format!("streamed {progress}%")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(Size::Sm.spacing()))
                            .px(px(Size::Lg.spacing()))
                            .py(px(Size::Sm.spacing()))
                            .border_b(px(1.0))
                            .border_color(surfaces.border.to_hsla())
                            .child(
                                div()
                                    .text_size(px(Size::Lg.font_size()))
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child("Markdown"),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("restart")
                                    .small()
                                    .label("Restart stream")
                                    .on_click(cx.listener(|this, _, _, cx| this.restart(cx))),
                            )
                            .child(
                                Button::new("theme")
                                    .small()
                                    .label(if self.dark { "Light" } else { "Dark" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_scheme(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .p(px(Size::Lg.spacing()))
                            .child(
                                MarkdownView::new("markdown")
                                    .state(&self.markdown)
                                    .scrollable(false)
                                    .on_link_click(cx.listener(
                                        |this, url: &str, window: &mut Window, cx| {
                                            let _ = window;
                                            this.last_link = Some(url.to_string());
                                            cx.notify();
                                        },
                                    ))
                                    .on_task_toggle({
                                        let markdown = self.markdown.clone();
                                        move |checked, marker, _, cx| {
                                            // The viewer never edits the source:
                                            // the host owns it and applies the
                                            // toggle itself.
                                            markdown.update(cx, |state, cx| {
                                                let next = apply_task_toggle(
                                                    state.source(),
                                                    marker,
                                                    checked,
                                                );
                                                state.set_source(next, cx);
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .px(px(Size::Lg.spacing()))
                            .py(px(Size::Xs.spacing()))
                            .border_t(px(1.0))
                            .border_color(surfaces.border.to_hsla())
                            .text_size(px(11.0))
                            .text_color(surfaces.dimmed.to_hsla())
                            .child(match &last_link {
                                Some(link) => format!("link clicked: {link}"),
                                None => "click a link or a task checkbox".to_string(),
                            }),
                    ),
            )
    }
}

fn main() {
    gpui_kit::application().run(|cx: &mut App| {
        guiraffe_core::init(cx);
        let bounds = Bounds::new(Point::default(), size(px(980.0), px(760.0)));
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(Gallery::new),
        )
        .expect("window opens");
    });
}
