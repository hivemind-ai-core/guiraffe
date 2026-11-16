# Markdown + Mermaid review #1: BLOCKED (not merged)
Must fix:
1. Streaming re-parses everything: the Append path does set_text plus a full Document::analyze per chunk, and old tasks aren't cancelled. Use push_str with the tail, analyze from settled_prefix, coalesce, and keep a single in-flight task. Wire up the unused settled_prefix, coalesce_delay and unfinished_mermaid_fence.
2. A streaming mermaid fence creates a new viewer and engine per token (the id is a content hash). Key by fence position, and hold back unfinished fences.
3. Prove incrementality with counters (parses, analyze calls) and measure first-paint and append timings on 1 MB against spec/15.
Same pass:
4. A process-wide mermaid cache budget.
5. Re-rasterize (debounced) on zoom and bounds changes.
6. Fix the cache comment, or re-insert on error.
7. A forced-panic test; document that unwinding is required.
8. A theme generation in the cache key.
9. A safe default link handler (never open URLs without a host callback), with a test.
10. GFM tests: tables, footnotes, task toggles, images policy, autolinks.
11. No mutation in render (mermaid host.rs:51); remove the dead code in streaming.rs.
