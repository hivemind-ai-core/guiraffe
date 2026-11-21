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
