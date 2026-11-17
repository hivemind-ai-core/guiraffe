# Code viewer review fixes — 2026-11-17

Validated source: `f42f619795bfad26102d854961deb1ef6ceb1816` on `feat-code`, rebased onto release/0.1
`7df0b5619b8b28fa2e59cd7b926a5922eb6d7e78`. The original hand-in remains at
`backup/feat-code-review-ea47512`. No release/main merge or push was performed.
The next documentation commit adds only this report and transcripts.

## Review changes

- Pending-source annotations, folds and traces are held against the requested source
  revision and installed with that source. A newer source drops the older revision's
  presentation. Prepared analysis can be queued for a document sharing the requested
  source handle. A language refresh preserves a pending replacement source.
- The original behavior is reproduced by a GPUI test on the pre-fix checkout: the
  requested source publishes, but its annotations become `[]`. See
  [before-fix transcript](validation/review-race-before.txt). The same annotation/fold/
  trace test and prepared-analysis/stale-result tests pass with the fix.
- Viewport offsets use f64. The million-distinct-line interaction test now checks a
  quarter-pixel scroll and its actual source anchor near line 900,000. Horizontal
  scrolling is bounded by the widest shaped line seen for the current document.
- Ctrl+C leaves the clipboard unchanged without a nonempty valid source selection.
- Shaping is capped at 16 KiB of UTF-8 text per logical line, at a character boundary.
  A visible notice marks limited previews. The long-line test checks the shaping bound,
  bounded horizontal scrolling and full-original-source selection/copy. This is a
  bounded preview, not horizontal window-shaping of every byte in a huge logical line.
- Shape-cache collision replacement no longer evicts an unrelated entry from a full
  cache. Changing a symbol provider refreshes an existing hover. New source publication
  cancels the previous drag/hover state. Geometry-dependent overlays receive a post-layout
  notification for layout revisions, not only changes to outer bounds.
- The handoff benchmark was renamed `code-snapshot-arc-clone`: it performs 1,024 Arc
  clone/drop pairs and does not exercise worker delivery, channels or UI publication.
  The cancellation fixture now atomically publishes its PID marker, fixing an observed
  empty-file parse race while keeping its process cleanup assertions intact.
- Added the CodeViewer showcase entry with syntax colors, a highlight, inline hint,
  lens, Markdown analysis card, fold marker and trace. Coverage passes with 38 entries.

## Gates

Both backends pass the following commands, with `ARGV0` unset and a shared target cache:

```sh
cargo test -p guiraffe-code -p guiraffe-bench
cargo test -p guiraffe-showcase
cargo clippy -p guiraffe-code -p guiraffe-bench -p guiraffe-showcase --all-targets -- -D warnings
# Repeat with --features gpui-kit/gpui-fast before the clippy -- separator.
cargo fmt -p guiraffe-code -p guiraffe-bench -p guiraffe-showcase -- --check
git diff --check
```

Per backend: **32 code checks** (14 unit/property, 15 GPUI interaction, 2 visual
cases, 1 doctest), **34 benchmark checks**, and **4 showcase tests** pass, with zero
failures or ignored tests. The showcase render test checks 38 entries in light/dark:
76 scenes per backend, 152 across both. Final runs use normal reference comparison
with update mode unset. All-target clippy includes every example and test target.

Transcripts: [upstream tests](validation/upstream-test.txt),
[fast tests](validation/fast-test.txt), [upstream clippy](validation/upstream-clippy.txt),
[fast clippy](validation/fast-clippy.txt), [upstream showcase](validation/upstream-showcase.txt),
[fast showcase](validation/fast-showcase.txt).

The CodeViewer light/dark references and Markdown code-color changes were inspected.
Adding the entry changes the shell count from 37 to 38, so all showcase references
were refreshed. Image differences against the previous references showed 72 of 74
existing upstream images changed only in the header; the two Markdown-viewer images
also changed in syntax colors. [Difference audit](validation/showcase-differences.json).
The standalone/embedded code-viewer reference hashes are in
[validation/snapshots.json](validation/snapshots.json).

## Measurement and integration limits

The previous report's timing numbers and performance summaries have been removed from
current acceptance claims. Superseded raw records are under [validation/archive](validation/archive/README.md),
with their original source revision and sample-count caveats. No current worker handoff,
frame-time, allocation-free renderer, release FPS or speed-up claim is made from them.
The warmed allocator tests cover the registered model/Arc workloads, not GUI rendering.

Controls' Tree-sitter feature remains unified across Kit consumers. Code ships Rust,
JSON and JavaScript grammars; hosts needing more languages must register them. The
full showcase was tested with that feature combination, but this does not establish
Syntect's broader language coverage. The code-fence adapter remains host-installed;
it leaves Mermaid dispatch available and avoids a Markdown/code dependency cycle.

Remaining nonblocking polish: continuous drag-selection autoscroll, gutter drag
selection, and fully synchronous placement of geometry-dependent Markdown cards.
Cards may settle on a following frame. The minimap remains a density overview;
analysis cards have bounded fixed-height margin packing. Editing, cross-window traces
and full horizontal virtualization of extreme logical lines remain outside this hand-in.
