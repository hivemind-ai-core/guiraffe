# Markdown performance

The [streaming tests](src/streaming.rs) compare incremental metadata with a
one-shot GFM analysis across every character split of a mixed document.
After initial analysis the background worker receives the unsettled source
tail. It holds one in-flight analysis and coalesces newer updates. Prose
appends reuse settled metadata.

The scrollable viewer seeds Kit's list with 28 px per block. The list
updates heights after measuring visible blocks and overscan. It stores one
height per block. Growth rebuilds the height index and preserves the
logical top item. Kit retains up to 4,096 shaped layouts per UI thread
across all text views. The scrollable view caches paragraph decorations,
table metrics, and fence highlights for up to 256 top-level blocks. A
single large block may still use memory proportional to its content.

The renderer parses eligible documents in sections, using unindented ATX
headings as boundaries. When the first complete section is at most 32 KiB,
the initial view can show it while the background parser works. The parser
checks for source replacement and view teardown between sections. Streaming versions
share existing paragraph runs and copy them only when edited. The append
path still copies the raw source and top-level block vector. Documents
with link definitions, footnotes, or ambiguous blocks use the one-shot
parser. The background renderer and metadata pass consume one raw AST section at a
time. The published background result still holds the entire render tree.
[AST equivalence tests](../../vendor/gpui-base/src/text/format/markdown.rs)
cover GFM block combinations and source positions.

An isolated release parser run on the 1,761,032-byte, 50k-line soak fixture
produced 10,205 blocks in both modes. On the shared machine, stock parsing
took 20.21 s and peaked at 226,948 KiB. Section parsing took 2.22 s and
peaked at 51,392 KiB. Callgrind at 5,000 lines counted 617,890,968 stock
instructions and 375,697,874 with sections, a 39.2% reduction. These
measure the isolated parser process. The section visitor that drops each
AST after counting its blocks took 1.60 s and used 14,184 KiB peak RSS on
the same fixture. Mounted
latency, memory, and repaint allocations remain open.
