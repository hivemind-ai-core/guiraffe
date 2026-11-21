# Chat performance

The [long-history interaction tests](src/gpui_tests.rs) cover batched list
insertion, lazy Markdown state creation, and traversal with eviction.
History keeps text state for 64 recently used rows and evicts the row
farthest from the current row being laid out. When a row returns to the list,
it recreates Markdown from raw text. Reasoning parses only when open and
recreates its state if the row was evicted.

Rows participating in a selection retain their text entities until Kit
releases the selection. This preserves window-owned copy and selection
endpoints. That exception can grow during a selection across history.
Separately, Kit retains up to 4,096 shaped layouts per UI thread, up to
eight fence parsers per thread with at most 64 KiB of cached source per
parser, and up to 128 compiled Tree-sitter query entries process-wide.
Unknown fence names do not create parser entries.

One message view and its underlying data are still allocated per history
row. Cold mount, peak RSS with a selection spanning history, and warm
repaint allocations remain open.
