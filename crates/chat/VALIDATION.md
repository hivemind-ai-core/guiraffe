# Agent chat handoff — 2026-11-16

Branch `feat-chat`, based on `release/0.1` at `aa82bb4`. No merges performed.

The requested UI and provider hooks are implemented. One performance acceptance
criterion remains conditional on the native backend policy: default gpui-fast
scroll layers can repaint earlier visible messages. Strict zero earlier-message
renders is proved on its non-composited view-retention path, not guaranteed with
the default composited list. This limitation is explicit in the tests and README.

## Changes

- `2cd38c7`: provider-independent models, scroll policy, UTF-8 completion, bounded hand-off.
- `a8fff7e`: native variable-height conversation control, isolated message/composer
  entities, streamed Markdown/code copy, tool/reasoning disclosures, attachment
  chips, slash/mention completion, copy/retry/edit/cancel/stop, entry Presence,
  host events, schema-backed presentation props, example, tests and visual goldens.
- Companion gallery commit: Agent chat route with simulated background streaming,
  working attachment demo, sidebar goldens, and this report.

The channel uses Crossbeam's bounded queue with one non-clonable, non-Sync producer
and consumer. Cancellation remains shareable. It bounds pending chunks and work
per foreground tick; no UI mutex or history-wide document clone occurs per token.
The public provider can update reasoning and tool results separately by message id.

Only `crates/chat`, the explicitly authorized gallery, and workspace dependency /
lock entries changed. No `crates/test` or other component crates were edited.

## Final checks

The following passed on **both** upstream and gpui-fast, with no ignored tests:

```sh
cargo test -p guiraffe-chat -p guiraffe-gallery
cargo test -p guiraffe-chat -p guiraffe-gallery --features gpui-kit/gpui-fast
cargo clippy -p guiraffe-chat -p guiraffe-gallery --all-targets -- -D warnings
cargo clippy -p guiraffe-chat -p guiraffe-gallery --all-targets --features gpui-kit/gpui-fast -- -D warnings
cargo fmt -p guiraffe-chat -p guiraffe-gallery -- --check
git diff --check
```

Per backend:

| Crate | Passed |
|---|---:|
| guiraffe-chat | 30 unit/GPUI tests + 1 light/dark snapshot case + 1 compile-fail ownership doctest = **32** |
| guiraffe-gallery | 2 unit tests + state integration + all-page snapshot case = **4** |

Chat includes 14 pure/property/concurrency tests and 16 GPUI interaction tests.
Coverage includes native keyboard entry, Shift+Enter, completions, attachment
removal, real wheel detachment/jump, streaming height growth, 500-message
virtualization, code-copy clipboard contents, edit branching/cancellation,
replacement-stream cancellation, live reasoning, tool disclosure/status changes,
window isolation and both retained-rendering policies.

The final snapshot comparison runs used committed goldens without update mode.
Light/dark standalone chat and gallery-chat images were opened and inspected on
both backends. Standalone chat uses four explicitly redrawn native headless frames,
bundled assets, and guiraffe-test snapshot comparison/configuration. Gallery uses
its existing shared Snapshotter and checks all 8 pages in both schemes.
Existing gallery goldens changed only for the added sidebar row, apart from a
maximum 1/255 antialias difference outside the sidebar on gpui-fast Overview.

`cargo bench -p guiraffe-chat --bench handoff --profile dev` completed 100,000
96-byte chunks (9,600,000 bytes) in **39.236 ms** in the final run. This is one
standalone hand-off wall-clock measurement including allocations/thread work,
not a UI frame-time or production throughput guarantee. Toolchain: rustc 1.98.0
(88d9e12ae), Linux x86_64; gpui-kit 0.9.1, gpui-fast 0.1.15.

## Native limits / remaining integration work

1. gpui-fast's default scroll layers bypass nested retained view reuse while
   recording the changed list layer (`fast/retained.rs`, `fast/layers/paint.rs`).
   Tests prove unchanged conversation/composer renders and bounded visible-row
   repaint in that mode; disabling composited layers proves unchanged earlier
   message renders too. Applications can select `GPUI_SCROLL_LAYERS=0`; the widget
   does not mutate window-wide or process-wide backend policy. A native backend
   fix is needed for the strict zero-earlier-row criterion in default mode.
2. The shared `crates/test/src/snapshot.rs` owns an asset source but currently
   passes `Arc::new(())` to HeadlessAppContext. Existing gallery SVG icons therefore
   remain absent in its snapshots. Chat's local appearance driver supplies assets
   correctly and forces complete frames; the library/runtime does not depend on
   the workaround. The foreman was notified of the shared helper issue.
3. Provider transport and real file dialogs are intentionally host-owned through
   typed events. Examples simulate streaming and attachment selection locally.

Ready for foreman review/integration; no branch merges or external publishing done.
