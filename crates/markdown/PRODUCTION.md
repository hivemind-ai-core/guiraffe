# Markdown performance

The [streaming tests](src/streaming.rs) compare incremental metadata with a one-shot GFM analysis across every character split of a mixed document. After the initial analysis, the background worker receives only the unsettled source tail. One metadata analysis can be in flight and newer updates coalesce into one follow-up.

The viewer retains one source buffer and one published metadata snapshot per document. The pinned Kit also holds parsed text and retains up to one shaping result per paragraph, a data and width result per table, and a highlight result per fenced code block. Total retained layout data can grow with the document. Kit measures all blocks when it first lays out the document and has no public viewport-only measurement switch in the pinned version. Bounded layout memory and scale-independent first layout remain open.
