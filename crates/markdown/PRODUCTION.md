# Markdown performance

The [streaming tests](src/streaming.rs) compare incremental metadata with a
one-shot GFM analysis across every character split of a mixed document.
After initial analysis the background worker receives the unsettled source
tail. It holds one in-flight analysis and coalesces newer updates. Prose
appends reuse settled metadata. The [Mermaid fence scan](src/streaming.rs)
resumes at the last completed line and rechecks only the unfinished line.
The prior full scan accounted for 6.24% of samples in a mounted dev
profile. For 128 appends after a 50,000-line prefix, an optimized isolated
Callgrind run counted 1.770 billion instructions before and 17.09 million
after, including the initial scan. Tests cover every three-chunk split of
a mixed UTF-8 document and confirm equal suffix work after short and long
prefixes.

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
share existing paragraph runs and copy them only when edited. Appends
reparse from the final block’s source span and share settled block leaves
through the persistent vector. The raw source remains contiguous and is
copied on append. A [Kit scaling test](../../vendor/gpui-base/src/text/state.rs)
observed zero settled block copies at both 256 and 4,096 blocks. Documents
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

In the loaded upstream dev soak, sharing paragraph runs reduced sampled
paragraph-clone self time from 9.41% to 2.26%. The two frame intervals
measured 504–551 ms p99 before sharing and 134–152 ms afterward. The
settled mount was 4.83 s and sampled RSS was 170–171 MiB after sharing.
The settled-mount probe drains background work, and the measurements do
not establish the quiet release budgets.


The upstream dev short soak first drew published content at 48 ms, reached
background completion at 854 ms, and measured 4.0–4.5 ms p99 for scrolling
and appending. Boxing rare image and custom payloads, using exact-size
source maps, and trimming run capacity reduced the inline run from 424 to
96 bytes on a 64-bit build. The process peak delta over the empty window
fell from 145,256 to 74,488 KiB. The 50k fixture has a 67,686 KiB peak delta
budget. These shared-machine development measurements leave production
RSS and warm repaint allocation checks open.

Large estimated-height views retain cold descriptions for settled text-only
top-level paragraphs. The descriptions keep source ranges, exact marks
and source maps. Transformed runs store their normalized text. Layout
materializes the inline runs near the viewport and shares the 256-block
limit with decoration and metric caches. Logical selection is separate
from materialized runs. The final block, headings, tables, lists and
paragraphs with inline objects use the ordinary representation.

The [Kit tests](../../vendor/gpui-base/src/text/document.rs) verify eviction
during traversal and source and plain copy across cold paragraphs,
including after evicting both selection endpoints. With cold paragraphs,
the shared-machine upstream dev probe drew the first useful frame in
57 ms, measured 5.0–6.5 ms scroll/append p99, and peaked at 66,656 KiB
above the empty window. The full parse and metadata settled in 1.03 s.
