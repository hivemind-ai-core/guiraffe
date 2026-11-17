//! guiraffe-markdown: streaming GFM viewer with highlighted code and embedded diagrams.

use gpui_kit::{AnyView, App, AppContext, IntoElement, ParentElement, Styled, Window, div, px};
use guiraffe_markdown::{MarkdownState, MarkdownView, document::apply_task_toggle};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{Knob, playground_with};

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "markdown",
        name: "Markdown viewer",
        summary: "GFM tables, task lists, highlighted code with copy, and mermaid fences drawn inline. MarkdownState streams appends incrementally; task checkboxes toggle the source.",
        covers: &["MarkdownView", "MarkdownState"],
        demo: markdown,
    }]
}

const DOC: &str = "\
# Release 0.4 — *Aurora*

Aurora makes large documents **fast**: only the open block is re-measured while a reply
streams, so a 10k-line transcript scrolls at full frame rate. See the
[changelog](https://example.com/changelog) for details.

## Checklist

- [x] Incremental parsing for appends
- [x] Syntax highlighting (200+ languages)
- [ ] Table of contents sidebar

| metric | 0.3 | 0.4 |
|---|---:|---:|
| first paint | 38 ms | **9 ms** |
| append (1 token) | 2.1 ms | 0.08 ms |
| memory / 1k lines | 4.2 MB | 1.3 MB |

```rust
let doc = cx.new(|cx| MarkdownState::new(\"\", cx));
doc.update(cx, |d, cx| d.append(token, cx));
```

```mermaid
flowchart LR
  Token[token] --> Append[append]
  Append --> Open{open block?}
  Open -->|yes| Measure[re-measure]
  Open -->|no| Keep[keep layout]
```

> Settled blocks are never laid out twice.
";

fn markdown(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Release notes (click a task to toggle it, click a link)",
        &[
            Knob::toggle("Selectable").default(1),
            Knob::toggle("Code copy").default(1),
            Knob::toggle("Scrollable"),
        ],
        |_, cx| (cx.new(|cx| MarkdownState::new(DOC, cx)), EventLog::new(cx)),
        |(state, log), v, _, _| {
            let (toggle_state, toggle_log, link_log) = (state.clone(), log.clone(), log.clone());
            let view = MarkdownView::new("showcase-markdown")
                .state(state)
                .selectable(v.on(0))
                .code_copy(v.on(1))
                .scrollable(v.on(2))
                .on_link_click(move |url, _, cx| {
                    EventLog::log(&link_log, format!("on_link_click {url}"), cx)
                })
                .on_task_toggle(move |checked, marker, _, cx| {
                    EventLog::log(
                        &toggle_log,
                        format!("on_task_toggle checked={checked} at {marker:?}"),
                        cx,
                    );
                    toggle_state.update(cx, |s, cx| {
                        let next = apply_task_toggle(s.source(), marker, checked);
                        s.set_source(next, cx);
                    });
                });
            column([
                example("Events", log.clone()),
                if v.on(2) {
                    div().h(px(360.)).child(view).into_any_element()
                } else {
                    div().child(view).into_any_element()
                },
            ])
        },
        window,
        cx,
    )
}
