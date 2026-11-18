# Release 0.1 → main: checklist

## Merge gates (all on release/0.1, both backends: default + `--features gpui-kit/gpui-fast`)
- [x] In-flight branches merged: controls §12 (focus/a11y), data-ui-3, code viewer + demos, datagrid, timeline overhaul, kilo (layout, flow connectors, Indicator→Kit Badge, Kit semantic + alert colours), showcase audit, CLI dev/add/doctor, comparison harness, bug-fix batches. **Deferred to 0.2:** feat-devtools (review fixes outstanding), feat-commands (blocked on review), omp's feat-mermaid-2 and feat-components-2 (unreviewed side work), feat-charts (in progress).
- [x] `cargo test --workspace` green on both backends from a **clean build** (2026-11-18, f1c6a8f+fix): only failure was a rustdoc E0460 link error in a workspace-wide doctest (two feature-builds of flow-core) → example marked `ignore`, covered by bench/tests/allocator.rs
- [x] `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` (clean build)
- [x] Coverage test + committed render goldens verified against a clean build on both backends (no regeneration); changed goldens reviewed when regenerated
- [x] Benchmarks vs e88911f (30 samples): core-variants 240→90 ns (−62%, memoized after a 5.6 µs regression was caught and fixed); flow-build-5k +1.6% [−5.6%, +12.2%] pass; flow-delete-child-5k −2.1% [−6.6%, +3.3%] pass. Earlier +39%/+115% were load noise. Bench tool fixed to skip suites missing on one side.
- [-] Hardening pass (Hegel pilot) **deferred to 0.2**; existing proptest/property tests cover window math, sorting, TSV, scales, reconciliation.
- [~] Live smoke (2026-11-17, Xvfb): showcase, gallery, flow editor, agent chat, markdown viewer all launch and stay up; screenshots of gallery/chat/flow/markdown correct. Xvfb here has no DRI3 (software Vulkan): windows intermittently present black and animation/timer-driven demos run slowly (markdown streaming), so a **manual pass on a real display is still required**: showcase, gallery, flow editor, chat, markdown, mermaid launched under Xvfb/Wayland; screenshots of each reviewed; manual checklist (focus, Tab, overlays dismiss, toasts, Spotlight, streaming chat, zoom)
- [x] `guiraffe new` app builds + launches from a clean clone (fixed: template lacked `[workspace]` and the guiraffe-icons dependency). Release binary 60 MB (42 MB stripped); 106 MB RSS under software Vulkan (not representative)

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
