# Release 0.1 → main: checklist

## Merge gates (all on release/0.1, both backends: default + `--features gpui-kit/gpui-fast`)
- [ ] In-flight branches merged or explicitly deferred: fix-controls-12 (focus/a11y/§12.2), fix-data-ui-3, feat-code, kilo (layout follow-up, flow connectors, Indicator→Kit Badge, Kit semantic colors), feat-devtools fixes
- [ ] `cargo test --workspace` green, both backends; no ignored tests without a KNOWN_ISSUES entry
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`
- [ ] Coverage test (every component in showcase) + render goldens green; all goldens reviewed since last palette change
- [ ] Benchmarks run against baselines (spec/13); no regressions > threshold
- [ ] Hardening pass (below) done for the listed crates
- [ ] Live smoke: showcase, gallery, flow editor, chat, markdown, mermaid launched under Xvfb/Wayland; screenshots of each reviewed; manual checklist (focus, Tab, overlays dismiss, toasts, Spotlight, streaming chat, zoom)
- [ ] `guiraffe new` app builds + launches from a clean checkout; README getting-started verified
- [ ] KNOWN_ISSUES.md current; CHANGELOG for 0.1; version bump; tag v0.1.0

## Hardening: property / stateful testing
Pilot **Hegel** (hegel.dev; `hegeltest` crate, MIT, beta, Hypothesis-lineage engine) as a dev-dependency on the stateful models where shrinking quality matters most; keep proptest where it already works. Targets:
- flow-core editor: random op sequences (add/move/connect/delete/undo/redo) vs a naive model; invariants (no dangling edges, undo∘redo = id)
- timeline: Track (move/trim/split) and Curve (set/move_key/eval) invariants; Gantt schedule vs brute-force longest path
- ui (DataUi): keyed repeat splices vs naive list model (identity preservation)
- state Store: undo lens, cascades, debounce; router history
- markdown/mermaid streaming: arbitrary chunking of a document gives the same result as one-shot
- code viewer: line index / viewport math at extreme sizes
Decision rule: adopt Hegel if it finds at least as much as proptest with better shrinking and stable CI; if beta churn hurts, stay on proptest.

## After 0.1
Open release/0.2 from main.
