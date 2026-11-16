# Code surface hand-in

Source/test revision: `3f1ee1d44224d629eedc1dd932e33c6dbc6625ae` on `feat-code`, based on `release/0.1` at `2687e9e`. No release/main merge or push was performed. The final documentation commit only adds this report and evidence files.

## Implementation

- Shared `Arc<str>` source with a prebuilt UTF-8 line index, content hashes and minimap density summary. Visible-line shaping uses a bounded, content-checked cache; the fold/lens map is sparse.
- Controls Tree-sitter highlighting runs off-thread with immutable publication, coalescing and stale-result rejection. Light/dark styles are prepared together; parser failure keeps source available.
- Line/fold/custom gutters, highlight/diagnostic annotations, inline hints with reversible source offsets, lens rows, selection and source copy, wheel/page/minimap navigation.
- Independent hover/occurrence, Markdown bubble and trace layers; a common trace leaf connects sibling viewers in one window and follows scrolling.
- `PreparedAnalysis` builds the annotation index/display maps on a worker and rejects obsolete document snapshots before any UI write.
- `code_fences(existing_extensions, max_lines)` embeds the same viewer in Controls Markdown and leaves Mermaid dispatch untouched. Standalone, embedded and stress examples are included.

## Gates

At the source revision above, both commands pass on each backend:

```sh
cargo test --locked -p guiraffe-code -p guiraffe-bench
cargo clippy --locked -p guiraffe-code -p guiraffe-bench --all-targets -- -D warnings
# Repeat both with --features guiraffe-code/gpui-fast
cargo fmt -p guiraffe-code -p guiraffe-bench --check
git diff --check
```

Per backend: **26 code checks** (10 pure/property, 13 GPUI interaction, 2 snapshot cases, 1 doctest) and **32 benchmark checks** pass, with zero failures or ignored tests. Clippy compiles every example/test target. The recorded test runs use normal snapshot comparison, with update mode unset. Test/clippy transcripts are in `validation/`.

Interaction evidence includes real mouse selection/hover/definition callbacks, outside-release handling, Ctrl+A/C, folds, minimap clicks, inline-hint coordinate preservation, cross-view scrolling, actual in-flight stale completions, prepared-analysis no-write admission, and a million **distinct** source lines. The latter asserts viewport-bounded shaping and zero new shaping on return to a cached viewport. gpui-fast assertions require hover/local-trace/cross-trace updates to leave both code text and unrelated host renders unchanged. Upstream ancestor redraws remain its documented behavior.

Eight PNGs (standalone/embedded × light/dark × backend) were inspected through the shared Snapshotter path. The four backend pairs are byte-identical; hashes are recorded in `validation/snapshots.json`. Inspection caught and fixed missing card positioning, missing highlight backgrounds, font fallback, a theme reset and dark card foreground inheritance.

## Measurements

Two benchmark suites are registered. `code-scroll-1m` covers 32 indexed viewport jumps per invocation; `code-snapshot-handoff` covers 1,024 Arc publications. Both warmed model suites pass the allocator's zero-allocation gate. Their raw samples, machine/load/governor metadata and confidence intervals are retained in the JSON records. They are initial dev baselines, not a before/after speed-up or renderer measurement.

- `code-scroll-1m`: median 89.549 µs per 32-jump invocation.
- `code-snapshot-handoff`: median 9.267 µs per 1,024-clone invocation.

The headless draw example ran on both backends over 1M distinct plain-source lines, 20 warmup plus 100 measured draws, 41 visible rows. At this revision: upstream p50 1.531 ms / p95 2.501 ms; gpui-fast p50 1.249 ms / p95 1.442 ms. `validation/headless-draw.json` preserves the summaries and exact scope. These are dev-profile headless update/draw timings on a shared machine, not release-mode desktop presentation, input latency, GPU raster/readback, or a 120 fps guarantee. Native per-frame samples were not retained. The interactive desktop stress example was compiled, not manually driven in a desktop window.

## Integration and remaining bounds

- Never merged: foreman owns review, release batch validation and integration.
- The generic `guiraffe-markdown` extension-hook question remains with the foreman. The adapter/example are implemented within `crates/code`; no Markdown-crate edit or dependency cycle was introduced. Mermaid already has its own component; the code adapter leaves its fences available for that renderer.
- Cargo unifies features. Enabling code selects Controls' Tree-sitter path for other Kit widgets too; applications relying on Syntect-only language coverage must enable/register the grammars they need. This deserves integration review.
- Vertical virtualization is implemented. An exceptionally long single logical line is still shaped whole; horizontal window-shaping is a remaining performance gap.
- Bubbles use fixed-height margin packing and omit overflow; the minimap is a bounded density overview. Cross-window traces and editing are outside this hand-in.
- Scope was `crates/code`, workspace dependency registration and the explicitly requested bench suite/allocation-gate integration. Other crates/workers were left untouched.
