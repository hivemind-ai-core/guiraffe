# GPUI Kit upstream proposals

Target repository: longbridge/gpui-kit. Base revision:
`39ab849a93b5f15507b969164d49891db8d5b470`.

## Add opt-in viewport measurement to scrollable TextView

Expose an estimated block height on the Markdown text state constructor.
Seed the list's vertical extent with this height, measure actual sizes in
the viewport and overscan, and rely on the list's scroll-anchor correction.
Keep exact intrinsic sizing for existing callers. Exercise first layout,
width changes, reveal to an unseen heading, and append while the reader is
away from the tail on both backends. A follow-up in GPUI and GPUI Fast
should add height hints to list splicing so Kit can keep measured heights
on append.

## Parse independently delimited Markdown sections

Use unindented ATX headings after a blank line as conservative boundaries,
shift mdast positions back into the original source, and pass each section
to the existing renderer and metadata visitor. Use the canonical parser for link definitions,
footnotes, and constructs whose scope is uncertain. Publish one complete
initial section for the estimated-height viewer while the background parser
produces the full document. Check for replacement or teardown between
sections. Exercise AST equality and UTF-8 source
positions. Extend coverage for document-wide constructs before widening
the fast path or claiming progressive publication for every document.

## Bound retained shaping independently of selection

Give the retained inline layout table a fixed entry cap. Leave current frame
layouts on their elements until paint so a cache eviction cannot drop
visible text. Preserve logical selection independently. Expose the existing
selection predicate to hosts that recycle text entities. Window-owned
selections currently refer to entity IDs. A future range snapshot API must
preserve source mapping, direction, copy format and cross-renderer selection
before hosts can recycle those selected entities.

## Share static paragraph runs across Markdown appends

The live viewer and parser both hold a snapshot during an append. Let
cloned paragraphs share their inline runs and copy them only on edit.
Keep the existing parsed-node API and selection pointers. Test copy-on-write
with text and selection projections. Store the root blocks in a persistent
vector so replacing the last parsed block shares settled leaves with the
previous snapshot. Verify sharing at multiple document sizes and preserve
source ranges and list continuation. The contiguous source string still
needs to be copied on append.

## Materialize Markdown text runs near the viewport

For opted-in, large Markdown TextViews, lower settled text-only paragraphs
to source ranges and exact inline metadata. Materialize the runs when a
paragraph enters the viewport or overscan and evict the furthest block
at the existing 256-block limit. Keep formatting and absolute source
positions from the canonical parse, and retain logical selection handles
across eviction. Source copy uses original spans, while plain copy reads
the stored source and normalization metadata. Exercise cross-window
copy and full traversal on both backends. Keep paragraphs with inline
objects and the streaming tail eager.

## Share immutable Tree-sitter queries across highlighters

Cache compiled queries by canonical language and query purpose, checking
the grammar and query source before reuse. Keep mutable parser state local
and cap the fence parser cache. Cover alias reuse, grammar re-registration,
injections, and high-cardinality fence names.
