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
updates heights after measuring visible blocks and 512 px of overscan on
each side. It stores one
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

Initial documents larger than 32 KiB can collect metadata from the renderer's raw AST
sections before each section is lowered to render nodes. The parser keeps
one section in memory at a time and publishes one metadata result. The
standalone GFM path still handles sources with dollar signs, an unfinished
Mermaid fence, or an initial parse superseded before completion. Streaming
updates continue to analyze the unsettled suffix. The mounted tests compare
shared metadata with canonical GFM after an append and verify that replacing
a document before the initial worker runs publishes the replacement.

The merged upstream release probe at 50k lines recorded about 700 ms
until background readiness under samply before sharing and 406 ms after.
Across the complete mounted probe, `markdown::to_mdast` accounted for
73.72% and 61.66% of inclusive samples respectively. The independent
metadata traversal accounted for 37.23% before sharing and did not register
among the sampled hot paths afterward. Callgrind at 5k lines and ten frames
counted 1,203,756,098 instructions on the original integration commit and
831,752,373 with shared parsing and the height-tree reuse change. Those
counts include fixture creation and the mounted frame loop.

The measured mounted warm draw still allocates. The 10k- and 50k-line
viewports matched at 1,447 allocations upstream and 93 on Fast. GPUI
paint/layout and Kit view retention will be profiled in the cross-component
allocation work.

A four-minute upstream release diagnostic recorded 24 ten-second intervals.
All were below 8 ms p99, and RSS repeatedly crossed the 2% drift threshold.
The diagnostic stopped after reproducing the drift. The 30-minute soak and
production executor long-task check remain open.

The first Fast release short soak at 50k lines peaked 157,156 KiB above
empty. Massif attributed 14,348,160 bytes at the 512-frame heap peak to
the test-only text snapshot recorder. Fast's retained root can skip the
render that clears that recorder. The soak now clears those snapshots
after each replay frame, including mixed and other Kit-backed workloads.
A mounted regression verifies that a frame can record text and that the
replay boundary clears it.

With bounded test snapshots, upstream release Markdown measured 28.8 ms
to the first useful frame, 4.27–4.53 ms p99 and a 64,972 KiB peak RSS delta.
Fast measured 25.8 ms, 5.26–5.39 ms p99 and a 74,024 KiB peak delta.
The Fast release run still exceeded the 67,686 KiB RSS budget by
6,338 KiB and crossed the drift threshold. Its 64-frame development
RSS gate passed at 64,908 KiB above empty. The final mounted warm probe
matched at 1,444 allocations upstream and 93 on Fast at both sizes.

| Retained Markdown state | Bound | Eviction |
|---|---|---|
| Settled text-only top-level paragraph runs | One compact descriptor per parsed run. On 64-bit targets, a source-backed run with a whole-run emphasis mark occupies at most 48 bytes and needs no separate mark or source-map allocation. Transformed runs and overlapping or rich marks keep their exact payloads. | Replaced with the parsed document. An append shares the unchanged prefix. |
| Selection state | One shared selection state per paragraph. Only images and custom inline objects own selection state for the preceding run. | Replaced with the owning block. |
| Materialized cold paragraphs, paragraph decorations, table metrics and fence highlights | 256 top-level blocks per estimated-height view. Individual blocks can be large. | Evicts the cached block farthest from the block being laid out. Logical selection survives eviction. |
| Shaped text layouts | 4,096 entries per UI thread across all Kit text views. | Prunes older entries in batches and sweeps dropped owners. |
| Resource images | The host’s GPUI image cache policy, with the app asset cache as fallback. | Managed by GPUI or the host’s image provider. |
| Mermaid rasters | 16 entries and 64 MiB per [Mermaid engine](../mermaid/PRODUCTION.md). | Evicts the oldest engine cache entry. |

See the [mounted heap diagnosis](../../plans/PERF-MARKDOWN-MEMORY.md)
for retained costs by backend.
