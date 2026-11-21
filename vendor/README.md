# GPUI Kit 0.9.1 patches

The crates are from the crates.io releases of `gpui-base` and
`gpui-component`, both published from longbridge/gpui-kit commit
`39ab849a93b5f15507b969164d49891db8d5b470`. Each copy retains its Apache
license, published manifest, and sources. The root Cargo manifest replaces
only these two crates. The workspace pin remains `gpui-kit =0.9.1`.

| File | Local changes and reason |
|---|---|
| `gpui-base/src/text/state.rs` | Adds `markdown_with_estimated_block_height` and threads the estimate into list creation and population. List growth reseeds estimates and restores the logical anchor because neither backend exposes a height-hinted splice. The constructor also publishes a bounded, complete initial chunk when available while the existing revisioned worker parses the full document. The worker checks for a newer replacement or view drop between parse sections. Exposes the existing selection predicate so a host can preserve window-owned selection endpoints during eviction. |
| `gpui-base/src/text/document.rs` | Seeds unknown heights on list population and lets the existing viewport and overscan layout replace them with measurements. The default full-measure policy still uses exact heights. Tracks up to 256 recently visited top-level blocks for decoration and metric caching. |
| `gpui-base/src/text/mod.rs` | Re-exports the section visitor for metadata consumers that need the same source positions. |
| `gpui-base/src/text/format/markdown.rs` | Splits eligible Markdown at unindented ATX heading boundaries and translates chunk positions to document coordinates and consumes each raw AST independently. Falls back to the one-shot parser around document-wide references, footnotes, and ambiguous block constructs. Equivalence tests cover mixed GFM blocks, UTF-8 positions, and the fallback. |
| `gpui-base/src/text/node.rs` | Releases cached decorations, table metrics, and fence styles for a distant block independently of selection. |
| `gpui-base/src/text/inline.rs` | Caps retained shaped layouts at 4,096 entries per thread and discards older entries in batches. Keeps measured text in the element until paint, so eviction cannot remove text needed by the current frame. Selection remains in the corresponding inline state. Includes a live-state cache bound test. |
| `gpui-component/src/highlighter/highlighter.rs` | Shares compiled Tree-sitter queries via `Arc`, keyed by canonical language and query purpose, with grammar and source validation. The process-wide cache is capped at 128 entries. The separate combined, injection-discovery, and injected-highlight queries retain their existing semantics. |
| `gpui-component/src/text/mod.rs` | Keeps up to eight recent canonical fence parsers per thread, limits cached input to 64 KiB per parser, skips unknown languages, and invalidates on registry changes. |

The viewport policy bounds measurement work by the viewport and overscan
except for the list index rebuild when the item count changes. The index
stores one estimate or measurement per block. Paragraph decoration, table metrics, and fence highlight results for
scrollable Markdown are retained for at most 256 top-level blocks per view. Chat
retains selected rows until the window releases their selection endpoints.
