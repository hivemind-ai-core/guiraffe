# Chat performance

Chat keeps cold history in indexed message records and creates a GPUI
message entity when the row is laid out or starts streaming. History keeps
64 ordinary resident entities and evicts the one farthest from the row being
laid out. The record preserves raw message data and disclosure state.
Reasoning Markdown parses on demand when open.

Streaming rows, rows with an open menu, and rows participating in a
selection retain their entities until the interaction ends. The selection
exception can grow with the rows participating in a drag across history.
Each offscreen pin observes its row and existing selectable text views.
Ending a stream, closing its menu, or clearing the selection releases the
pin when the row has no remaining active interaction. Re-entering the row
transfers it to the ordinary cache, and history truncation drops the pins
for removed rows. `ended_pins_evict_without_entering_another_row` checks
release after an eight-cache traversal. Kit independently keeps
up to 4,096 shaped layouts per UI thread, eight fence parsers per thread
with at most 64 KiB of cached source per parser, and 128 compiled Tree-sitter
query entries process-wide.

The [interaction tests](src/gpui_tests.rs) exercise the 64-entity bound at
every offset through 400 rows, then revisit evicted rows and check that tool
and reasoning disclosures survived. The
[Linux RSS test](../../bench/examples/soak.rs) compares fresh processes
with an empty window and with 100,000 messages. On the shared development
machine, its upstream backend delta fell from 105,368 KiB to 57,356 KiB
against a 77,033 KiB budget. GPUI Fast measured a 56,344 KiB delta. The same-harness dev scroll probe mounted in
119 ms and measured 3.2–3.9 ms p99, with a 58,556 KiB process peak delta
over the empty window. Peak RSS during a selection spanning history and
warm repaint allocations remain open.

Reply creation admits materialized views to the same cache even when no frame
renders between streams. A chat has a single active producer. The test
`superseded_offscreen_replies_respect_the_row_cache` exercises 1,000 consecutive
reply starts and checks ordinary residency and ended stream pins. The ignored
`selection_retention_across_one_hundred_thousand_rows` probe tests a held drag,
a completed drag, and keyboard Select All on a focused message while the list
scrolls through 100,000 rows. On upstream, all three peaked at 64 mounted views
and zero pins. Selection cleared at the first scroll to row index 8 and
remained clear at the end. The current Kit participant projection clears a
selection when its rendered endpoint leaves the window.
