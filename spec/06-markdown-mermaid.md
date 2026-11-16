# guiraffe-markdown / guiraffe-mermaid
Markdown: CommonMark + GFM (tables, task lists, strikethrough, footnotes, autolinks), syntax highlighting for code, selectable text, copy code, link handler callback, image loading, heading anchors + TOC extraction, incremental re-parse for streaming (agent chat), virtualization for big docs, theming from core. Feature `mermaid` renders ```mermaid fences via guiraffe-mermaid.
Mermaid: standalone viewer element; parse+layout off the main thread, cache by source hash, pan/zoom/fit, theme-aware colors, error display, panic isolation. Port from ../gui/workbench/crates/mer (mermaid-rs-renderer).

## Required shared viewers: standalone and embedded

- Syntax-highlighted code and Mermaid diagrams each need a standalone viewer and an embedded Markdown form.
- Markdown fenced code blocks use the shared read-only `CodeBlock` viewer from spec/03, carrying the fence language and viewer options into it. Highlighting, selection/copy, theme handling and fallback behavior have one implementation.
- With the `mermaid` feature enabled, Markdown routes Mermaid fences to the standalone `guiraffe-mermaid` component. Parsing, layout, caching, theming, errors and panic isolation stay in that component rather than being reimplemented in Markdown.
- Markdown owns document parsing, fence dispatch and embedding layout. Viewers support sizing appropriate to both standalone windows and inline blocks; diagrams must not require a separate window.
- Preserve the source for copy. Any presentation reformatting must remain separate from the original code; editing is deferred.
- Acceptance: the same code source/language and Mermaid source work standalone and embedded; test fence dispatch and plain-text/error fallback, and inspect light/dark gallery examples for both forms. Shared rendering code must receive fixes in one place.
