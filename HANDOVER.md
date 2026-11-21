# PropertyGrid handover

The implementation is committed on `feat-property-grid`. The most recent code change is `dd42a94`, which preserves the selected theme when initializing divider persistence. Both workspace test stages passed with that source and the eight updated showcase goldens. The foreman stopped the gate during per-crate doctests and requested a handover. No build or profiling job from this dispatch remains active.

## Implemented

- Resizable, keyboard-operable divider with a layout-store persistence key and double-click fit to visible names.
- Shared virtualized density presets, sparse height overrides, alternating rows, grid lines, indentation guides, group styles, and sticky groups.
- Inline property inputs, growing text areas, typed component fields, mixed and read-only states, validation, reset, numeric scrub preview and range sliders, colour picking, enum and flag choices, and a path picker event.
- Editor keyboard navigation, clipboard and context menu, descriptions, advanced toggle, load-aware expansion and collapse, and cancellable filtering of expanded rows.
- Showcase controls and eight regenerated default-preset goldens across both backends and themes. Each changed image was visually inspected and rerun in comparison mode.
- New tests for sparse window scaling and zero geometry allocations at 10k and 10M rows, typed component parsing, numeric step and range bounds, divider drag and restoration, theme preservation, scrub preview and commit, and filter cancellation.

## Measured evidence

The early default-grid candidate was compared with the release baseline binary from the perf-baseline worktree. The host was under build load. These figures are indicative and predate the final editor changes.

| Probe | Baseline | Candidate |
| --- | ---: | ---: |
| Samply samples over 1,024 steps at 100,001 rows | 2,184 | 1,471 |
| Largest sampled self path, unsymbolized | 17.63% | 15.57% |
| Flexbox sampled self | 4.30% | 5.91% |
| Callgrind instructions, setup and 64 frames at 10,001 rows | 405,094,957 | 303,714,634 |
| Callgrind memcpy instructions | 90,887,281 | 58,064,349 |
| Callgrind allocator calls, setup and 64 frames | 149,183 | 113,174 |
| Short-soak mount at 100,001 rows | 96.3 ms | 14.8 ms |
| Short-soak p99 at 100,001 rows | 8.25–9.40 ms | 6.66–7.71 ms |
| Peak RSS at 1,000,001 rows over 256 frames | 61.5 MiB | 61.8 MiB |
| Loaded-host mount at 1,000,001 rows | 296 ms | 364 ms |
| Warm sparse geometry allocations at 10k and 10M | — | 0 |

Details and cache bounds are in `crates/tree/PRODUCTION.md`. Profile logs and the interrupted gate are in `target/` and `target/gate-logs/`. The allocation count for sparse geometry covers the pure viewport traversal. The default stress scenario leaves the extra inspector toolbar and styles off.

## Remaining work in order

1. Incorporate the current `release/0.2` into the feature branch after checking both heads. `release/0.2` was at `dcf9b51` when the handover was requested. The foreman owns release and main integration.
2. Complete the gate on the integrated revision. Both workspace test stages exited zero before the stop. Per-crate doctests ran partially, and the script had yet to run Clippy and formatting.
3. Add goldens covering active row-style presets and editor states. The current eight goldens cover the initial showcase layout. Verify that the match highlight, sticky push-off, picker and slider are visible in those states.
4. Broaden search to collapsed loaded descendants if required for spec/26 acceptance. Search currently matches names in the expanded projection and highlights matching rows. Auto-fit estimates the width of visible names. Expanding an extremely wide parent still calls the synchronous model splice within its batch.
5. Run revision-pinned release measurements with the toolbar and styles enabled as well as the default workload. Confirm quiet-window mount and p99 at both approximately 110k and one million rows, detect long main-thread tasks, and measure mounted warm repaint allocations. Zero allocations and load-independent traversal are established for the sparse geometry path. The full mounted renderer and million-row mount target remain open.

## Verification commands

Run from this worktree with its own target directories.

```sh
mkdir -p target/tmp
export TMPDIR="$PWD/target/tmp" CARGO_BUILD_JOBS=4
cargo test --release -p guiraffe-tree
cargo fmt --all --check
scripts/gate.sh

# Scoped showcase comparisons after the corresponding target has been built.
SHOWCASE_ONLY=property-grid CARGO_TARGET_DIR=target/gate-up cargo test -p guiraffe-showcase --test render every_entry_renders_in_both_themes
SHOWCASE_ONLY=tree-list CARGO_TARGET_DIR=target/gate-fast cargo test -p guiraffe-showcase --test render --features gpui-kit/gpui-fast every_entry_renders_in_both_themes

cargo build --release -p guiraffe-bench --example soak
SOAK_SCALE=9 target/release/examples/soak property-grid --short
target/release/examples/soak property-grid --short
SOAK_SCALE=10 SOAK_FRAMES=1024 samply record --save-only --unstable-presymbolicate -o target/property-grid-head.json.gz -- target/release/examples/soak property-grid
scripts/profile-summary.py target/property-grid-head.json.gz
```

Set `GUIRAFFE_UPDATE_SNAPSHOTS=1` only for scoped golden regeneration and inspect every changed image. Persistence should initialize `LayoutStore` directly when absent. Calling `guiraffe_layout::init` from the divider builder resets the application theme. Keep upstream and gpui-fast builds in separate worktree-local target directories.

## GPUI Kit patch handover

The patch worktree is `perf-kit-patch`. Merge commit `10763f0` incorporates
`release/0.2` at `58f268b`. The soak keeps the release's `Samples` pipeline,
mixed, grid, code, code-annotations and limit scenarios, and production-executor
Mermaid. Markdown records its first published rendered frame and prints a
separate background-ready timestamp. Chat and Markdown retain the mounted RSS
and warm-repaint scaling checks.

| Probe | Recorded baseline | Merged release or historical patch evidence |
|---|---:|---:|
| Markdown first useful frame | 705–755 ms, quiet release | 32.3 ms, loaded upstream release |
| Markdown background work through first settled frame | — | 808 ms, loaded upstream release |
| Markdown frame p99 | 11.0–11.6 ms, quiet release | 4.95–5.32 ms, loaded upstream release |
| Markdown RSS delta | 136 MiB upstream / 235 MiB Fast, quiet release | 64,624 KiB, loaded upstream release |
| Chat 100k history RSS delta | 105,368 KiB upstream, prior dev | 57,788 KiB upstream release / 56,344 KiB Fast dev |
| Fence scan instructions, 128 appends | 1.770B | 17.09M |
| Markdown inline run size | 424 B | 96 B |

Build and measurement logs are in `target/build-release.log` and
`target/takeover-validation/`. The merged release profile at 50k lines
attributed 73.72% inclusive to `markdown::to_mdast`, with independent
render and metadata traversals. Both scale-50 release smoke runs completed
10 frames. The upstream short soak uses two 10-second sample intervals. Cache bounds and test links are in
`vendor/README.md` and both component `PRODUCTION.md` files.

Remaining verification includes the scoped tests and Clippy on both backends
and the full gate. Markdown still needs
revision-pinned evidence for RSS on both backends, p99 and long tasks, and
zero-allocation warm repaint. The foreman approved sharing the render and
metadata parse after the merge handoff. Preserve section equivalence,
incremental metadata, cancellation, and selection when implementing it.

### List guard and shared initial parse

- `c18797f` retains the measured list tree when its records are unchanged.
  The plain-list regression compares equal viewport visits and allocation
  counts at 10k and 100k items. Both complete bench allocator suites passed
  29 tests at this stage.
- `44029cb` shares the initial AST traversal between Markdown rendering and
  metadata for eligible sources. Canonical GFM fallback covers math syntax,
  a withheld Mermaid tail, and superseded initial requests. The shared
  observer retains one metadata accumulator, no AST sections, and sends at
  most one result. It adds no cache.
- The new Markdown tests assert zero bytes reparsed by standalone metadata
  analysis for the shared initial case, canonical metadata after a streamed
  append, and correct metadata after replacing a queued initial request.

| Upstream probe | Merge `10763f0` | Shared parse and list guard `44029cb` |
| --- | ---: | ---: |
| 50k lines, profiled background readiness | 700 ms | 406 ms |
| 50k lines, parser inclusive sample share | 73.72% | 61.66% |
| 5k lines and 10 frames, Callgrind instructions | 1,203,756,098 | 831,752,373 |
| 50k lines, first useful frame in short soak | 32.3 ms | 30.2 ms |
| 50k lines, p99 over two short intervals | 4.95–5.32 ms | 4.66–5.13 ms |
| 50k lines, peak RSS above empty process | 64,624 KiB | 64,200 KiB |
| Mounted Markdown warm draw allocations at 10k / 50k | 1,463 / 1,464 | 1,447 / 1,447 after list guard |

The timing and RSS values are indicative shared-machine measurements. The
latest upstream Markdown short soak reported drift in its second interval.
Mounted warm draws still allocate, and the contiguous source is copied on
stream appends. The short run does not certify a 30-minute soak, all
interactive paths, a UI-thread long-task budget, or the 10M/10k target.

Both scoped backend runs passed the Markdown, Chat, DataGrid, and Tree
unit and integration tests after the shared parser change. The Fast mounted
development probe measured 64,576 KiB of Markdown RSS above empty and
57,424 KiB for Chat. Fast warm draws measured 93 allocations for Markdown
and 127 for Chat at both tested sizes. The standalone Kit text harness
reproduced five Fast failures previously recorded on the inherited release
baseline. The upstream Kit text harness passed 439 base and 78 component
tests, and the Fast component harness passed 78.

### Final release build and gate

`scripts/gate.sh` passed on code commit `44029cb`, including both workspace
test stages, per-crate doctests, Clippy and formatting. It reported no
changed goldens. The merge of `release/0.2` at `73b5782` added roadmap
documentation only.

| Release short soak with bounded snapshots | First useful frame | p99 | Peak RSS above empty |
| --- | ---: | ---: | ---: |
| Upstream Markdown, 50k lines | 28.8 ms | 4.27–4.53 ms | 64,972 KiB |
| Fast Markdown, 50k lines | 25.8 ms | 5.26–5.39 ms | 74,024 KiB |
| Upstream Chat, 100k messages | 50.2 ms | 3.02–4.51 ms | 59,856 KiB |
| Fast Chat, 100k messages | 43.9 ms | 3.60–3.93 ms | 65,756 KiB |

Massif on the pre-cleanup Fast release replay attributed 14,348,160 B at
the 512-frame heap peak to Kit's test text recorder. On Fast the root
can be retained and skip clearing this recorder. The replay clears text
snapshots at its frame boundary without forcing a root render. The Fast
Markdown peak delta fell from 157,156 to 74,024 KiB. The remaining 6,338 KiB
RSS overage and drift remain follow-ups. Both mixed replays covered all
12 phases, and 32 ten-frame scale-50 smoke probes passed across the two
backends. All four mounted example tests passed on each backend, including
the new snapshot lifetime check. In those tests the Markdown warm draw
matched at 1,444 allocations on upstream and 93 on Fast at both sizes.

The five standalone Fast Kit failures were also reproduced on the
published 0.9.1 crates with GPUI Fast 0.1.15. The stock and patched
`gpui-component` suites both passed all 78 text tests.
