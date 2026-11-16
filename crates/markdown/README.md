# guiraffe-markdown

Markdown viewer for documents and agent chat: CommonMark + GFM through the
Kit's `TextView`, incremental streaming, heading anchors and a table of
contents, and — behind the `mermaid` feature — diagrams drawn where their
fences are.

## Layers

| module | what it owns | testable without a window |
|---|---|---|
| `document` | one parse: headings/anchors, Mermaid fences, task markers; `apply_task_toggle` | yes |
| `streaming` | append vs replace classification, settled-prefix planning, render coalescing, unfinished-fence detection | yes |
| `view` | `MarkdownState` / `MarkdownView`: analysis off-thread, streamed appends, links, task toggles, code copy, reveal | GPUI tests |

## Streaming

`MarkdownState::append` is the path an agent chat uses: it appends the chunk to
the `TextViewState` and nothing else, so every settled block keeps its layout
and its selection. `set_source` classifies an update first and falls back to a
full replace only when the text diverges in the middle (a tool result patching
an earlier paragraph). Document analysis runs on a background task and is
dropped when a newer revision supersedes it.

`streaming::settled_prefix` answers "how much of this is closed" — a paragraph
before a blank line, a fence after its closing run, a heading after its newline
— and `unfinished_mermaid_fence` says whether the open fence is a diagram, so a
half-streamed diagram is not re-laid out per token.

## Facts the viewer reports

- `headings()` — depth, plain text, GitHub-style unique anchor, source range.
- `document().fences` — every Mermaid fence with its de-indented source.
- `document().tasks` — checked state and the single-byte range of each `[ ]`.
- `reveal_heading` — maps a heading's source range through the rendered tree and
  scrolls to it.

The viewer never edits the source. `apply_task_toggle` is the host's helper for
a click: it validates the marker range (one character, still a marker) and
rewrites exactly that byte, so a stale range cannot corrupt the document.

## Mermaid diagrams

`guiraffe-markdown/mermaid` registers a block parser and renderer with the text
view: a `mermaid` fence becomes a `MermaidView` at a fixed height, keyed by the
hash of its source so a streaming document keeps one diagram identity. Without
the feature the fence renders as a code block.

## Syntax highlighting

`guiraffe-markdown/syntect` forwards to `gpui-kit/syntect`, which installs a
code-block highlighter every text view inherits. Off by default: it costs a few
megabytes and a couple of hundred grammars, and a chat that never shows code
should not pay for it.

## Known gaps

- **Virtualization is the Kit's, at document level.** Very large documents are
  still one `TextView` list; block-level virtualization exists in Base but this
  crate does not add windowing on top of it.
- **Image loading is the Kit's default.** The `image_source` override is exposed
  by `TextView` but not yet surfaced on `MarkdownView`.
- **Footnotes and autolinks** come from the parser and are checked by reading
  the tree, not by eye; no visual test pins them.
- **Selection copy format** defaults to the Kit's (`TextView::selection_format`
  is not surfaced on `MarkdownView`).
- **A mid-document rewrite** re-parses the whole document (the text view's
  append path does not apply). That is correct but not incremental.

## Example

```sh
cargo run -p guiraffe-markdown --features mermaid --example viewer
```

Streams a reply chunk by chunk into the viewer with a live table of contents,
code copy, link and task callbacks.
