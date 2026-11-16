# guiraffe-code

A read-only code surface with shared source, visible-line rendering and independent analysis overlays. Editing is outside this crate's scope.

Create `CodeHandle` once, keep it in your host, and render `CodeViewer::new(&handle)`. The viewer implements `Styled`; its default size comes from `StyleContext`, font scale/surfaces from GUIraffe, and monospace family from Controls. `CodeOptions` provides explicit overrides for size, line numbers, minimap and analysis-margin width.

```rust,no_run
use std::sync::Arc;
use gpui_kit::{App, IntoElement};
use guiraffe_code::{CodeDocument, CodeHandle, CodeViewer};
fn code(cx: &mut App) -> impl IntoElement {
    let document = Arc::new(CodeDocument::new(Arc::<str>::from("fn main() {}\n")));
    let handle = CodeHandle::new(1, document, cx);
    handle.highlight("rust".into(), cx);
    CodeViewer::new(&handle)
}
```

For a large file, index it on your worker before creating the handle, or use `set_source(Arc<str>, language, cx)`, which indexes and highlights on the background executor. At most one analysis runs at a time and one latest request is retained. Older completions are discarded. Rust, JSON and JavaScript grammars ship by default; hosts may enable/register additional Controls grammars. Cargo unifies dependency features: enabling this crate selects Controls’ Tree-sitter backend in other Kit widgets too, rather than its Syntect-only backend. Applications needing more languages should enable/register those grammars. Unknown languages show plain source. Parser panic/timeout preserves the source and exposes `highlight_error()`. Both light/dark highlight styles are prepared in the worker so switching schemes needs no parse.

`CodeDocument` shares its input `Arc<str>` and owns a UTF-8 byte/line index, content hashes and a bounded minimap summary. Only visible source and gutter rows are shaped; a bounded 2,048-entry cache checks full content on hash matches. Fold and lens mappings are sparse segments. Source positions are **zero-based UTF-8 byte columns**, and source ranges are half-open. Invalid annotation ranges are omitted. Fold ranges retain their first line and hide `start+1..end`.

Selection works by mouse drag or Ctrl/Cmd+A; Ctrl/Cmd+C copies selected source, or the whole source without a selection. PageUp/PageDown and wheel scrolling move the viewport. Clicking a folding marker toggles that fold; clicking the minimap resolves source lines, including folded regions. Inline hints insert display-only text with a reversible source mapping; they never enter copied source or symbol coordinates. Lens annotations add rows between source lines.

Supply `Annotation { id, range, kind, payload, color }` values for highlights, diagnostic underlines, inline hints, lens rows, Markdown bubbles and symbols. `GutterColumn` supplies sparse blame/coverage/etc. cells. Small batches can use `set_annotations`; large producers should construct `PreparedAnalysis::new(document, annotations, folds)` on a worker and publish with `set_analysis`. This moves the precomputed index, display hints and row map. A result for an older document returns false without changing state. Applying a prepared snapshot replaces the supplied fold state as part of that snapshot.

`set_symbol_provider` supplies hover Markdown and occurrence ranges; `on_definition` receives Ctrl/Cmd-clicks. These changes and `set_traces` notify the overlay leaf rather than writing the text entity. The gpui-fast interaction tests assert unchanged host/text render counts. Upstream GPUI can redraw ancestors, as documented in the API conventions.

For cross-view connections, create `TraceHandle::new(&[left, right], traces, cx)` and place its `element()` after the viewers in their common `relative` container. Viewer ids must be unique. Endpoints are source positions resolved in the same window coordinate space and follow scrolling. Folded/offscreen endpoints are hidden. Cross-window traces are outside this coordinate space. Bubbles are created only for visible annotations and packed without overlap in the reserved margin; overflow cards are omitted.

`code_fences(existing_extensions, max_lines)` embeds the same `CodeViewer` in Controls Markdown, with its own bounded-height scrolling. Construct the registry once and clone it while rendering. It preserves existing extensions and leaves Mermaid fences to the Mermaid renderer. The `markdown` example demonstrates this adapter. A generic extension hook in `guiraffe-markdown` is an integration follow-up with the foreman; this crate does not create a dependency cycle by making Markdown depend on code.

Examples:

- `cargo run -p guiraffe-code --example stress --features gpui-fast -- --million`: million-line viewer plus a second linked viewer. Omit `--million` for a small highlighted source; add `--dark` for dark mode.
- `cargo run -p guiraffe-code --example markdown --features gpui-fast`: the shared viewer inside Markdown.
- `cargo run -p guiraffe-code --example scroll_bench --features gpui-fast`: 100 measured headless draw iterations over a million distinct lines, after warmup; prints JSON.

The bench harness registers `code-scroll-1m` (indexed viewport traversal) and `code-snapshot-handoff` (Arc publication cost). These model suites do not measure desktop frame rate. The native headless example measures draw work, not input latency, display presentation, or a release-mode 120 fps guarantee.

Current bounds: vertical virtualization is implemented; each visible logical line is shaped whole, so an exceptionally long single line is not horizontally window-shaped. The minimap is a 256-bin density overview, not a miniature syntax rendering. Fold ranges come from analysis data; this is not a folding-language server. The margin uses fixed-height cards, and long annotation payloads may require a larger host-provided presentation. Snapshot goldens are per backend; validation is in `VALIDATION.md`.
