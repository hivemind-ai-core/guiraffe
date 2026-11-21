# CodeViewer: rich annotations at scale

Goal: an editor-grade decoration layer — thousands of diagnostics, a full git blame, coverage and
review comments on a 10M-line file — without leaving the spec/23 budgets (mount ≤200 ms, p99 ≤8 ms,
memory O(viewport) + compact per-annotation storage). Owner: codex. Review/gate: foreman.

## 1. Stress test first
Add `soak code-annotations`: 1M lines with 100k inline marks (ranges), 20k squiggles, 5k bubbles
(block widgets), 50k gutter marks, full blame (one record per line, ~2k commits), 1k inlay hints in
view, search highlighting of 50k matches. Interactions: scroll (line + page), jump, hover a mark,
toggle layers. Profile, write a diagnosis, then optimise. Lock with scaling tests (per-frame work
equal at 10k vs 1M annotations) and allocator tests.

## 2. Data model (performance-critical)
- One `Decorations` store per document: layers (diagnostics, search, blame, coverage, review, user…)
  each an **interval index** keyed by (line, col) ranges — query cost O(log n + visible); adds/removes
  in batches; stable ids; z-order and merge rules per layer. Compact storage (u32 offsets, interned
  styles/authors/commits) — documented bytes per annotation.
- Edits shift annotations (anchor bias left/right); layers can be marked "stale" and refreshed async.
- Paint only visible annotations; aggregate when dense (e.g. >N squiggles per line → one combined
  mark; minimap/scrollbar marks aggregated per pixel row).

## 3. Decoration kinds
- **Underlines**: straight, wavy (squiggle), dotted, dashed, double; severity colors (error/warning/
  info/hint) from theme tokens; "unnecessary" (faded) and "deprecated" (strikethrough).
- **Highlights**: range background (search, selection-like, read/write refs), outlined box,
  **circled/boxed text** (rounded outline around a range), bracket-pair match.
- **Gutter**: icons (breakpoint, bookmark, error), change bars (added/modified/deleted from a diff),
  coverage bars (hit/miss/partial), fold markers, multiple gutter lanes with fixed widths.
- **Blame**: gutter column (author, relative age, short hash) with **age heatmap**, grouping of
  consecutive lines from the same commit, hover card (full message, date, author, link callback),
  current-line inline blame ("You, 3 days ago • message") at line end.
- **Inline widgets**: inlay hints (type/param names, not part of the text), ghost text (AI
  completion preview), end-of-line diagnostics text, code lens rows above a line (clickable).
- **Block widgets / bubbles**: review comments or diagnostics rendered between lines (arbitrary
  GPUI content, virtualised, height measured once and cached).
- **Overview**: minimap or scrollbar marks per layer (aggregated per pixel).
- Hover/click events on every decoration with typed payloads; keyboard navigation (next/previous
  diagnostic, next change); accessible descriptions (diagnostics announced on the current line).

## 4. Deliverables
API + rustdoc, showcase entries (diagnostics, blame, review comments, coverage), goldens both themes
and backends, stress probe + tests above, PRODUCTION.md rows (per-annotation bytes, caches + bounds).
