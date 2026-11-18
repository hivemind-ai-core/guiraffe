# Chart delivery and validation

The implementation was tested at `4e78d431aba1e94c8ec338e6eccd6f652bd39daa`
on `feat-charts`, based on `release/0.1` commit `ac49cc5`. The branch is
ready for foreman review. Strict native hover isolation remains open.

## Implementation commits

| Commit | Change |
| --- | --- |
| `55af882` | Chart grammar and performance architecture specification |
| `f0a013d` | Typed borrowed sources, streaming snapshots, scales, and ticks |
| `6d1e31f` | Layer preparation, marks, stacks, facets, culling, and LTTB |
| `e08b1c6` | Canvas, guides, palette, interaction, accessibility, and data table |
| `27dfef0` | Axis label spacing, stable legend geometry, and hover invalidation |
| `4e78d43` | Gallery, streaming dashboard, snapshots, and benchmark registration |

## Gates

| Gate | Upstream | gpui-fast |
| --- | --- | --- |
| `cargo test -p guiraffe-charts` | Exit 0, 12 tests | Exit 0, 12 tests |
| `cargo test -p guiraffe-showcase` | Exit 0, 14 tests | Exit 0, 14 tests |
| `cargo clippy -p guiraffe-charts -p guiraffe-showcase -p guiraffe-bench --all-targets -- -D warnings` | Exit 0 | Exit 0 |

Fast commands used `--features gpui-kit/gpui-fast`. Workspace
`cargo fmt --all --check` and `git diff --check` exited 0. The charts
library source contains no direct `unwrap` or `expect` calls.
The optional instruction benchmark compiled via
`cargo check -p guiraffe-bench --bench instructions --features instruction-counts`
with exit 0 and a future-compatibility advisory for its dependency
`proc-macro-error2`. Callgrind execution remains unmeasured.

The snapshot gate compared all 67 catalog entries in both themes with
updates disabled on each backend. I inspected the 12 new images covering
Chart grammar, Faceted market charts, and Streaming chart dashboard.
A pixel audit of the 256 refreshed existing snapshots located every
above-tolerance difference inside the entry-count text in the header.
The worktree was cleared of `*.actual.png` and `*.diff.png` files.

## Performance

Release CLI measurements used Rust 1.98.0 on an AMD Ryzen 7 8745HS with the
powersave governor and CPU 15 affinity. Each suite used 40 samples with
20 iterations per sample. The tree was clean at the revision above.

| Workload | Median | Sample p95 |
| --- | ---: | ---: |
| `chart-line-1m`, full and zoomed 1,200 px views | 0.333 ms | 0.431 ms |
| `chart-scatter-100k`, one 1,200 px view | 1.197 ms | 1.298 ms |

These timings cover CPU viewport compilation from a prepared model. GPU
painting, end-to-end input latency, source preparation, and native frame
rate remain unmeasured. The line uses a precomputed LTTB hierarchy.
Dense scatter draws one representative per occupied screen cell and
uses the sorted source-backed index for nearest-point interaction.

## Remaining limits

- A changed native hover advanced the embedding host and the canvas from
  two renders to three on both backends. Scene compilation stayed at one
  and the unrelated retained sibling stayed unchanged. Moving within the
  same snapped tooltip target caused zero additional host renders.
  Both pinned GPUI implementations propagate child invalidation up their
  ancestor view path. This is the outstanding acceptance gap.
- Stream batches share source rows. The dashboard prepares a fresh model
  on the background executor before publishing each batch. Preparation
  work grows with accumulated data.
- The legend preserves the prepared domain and stack baselines when hiding
  series. Connected marks use one color per path. Large series are
  sampled for paint while tooltip and selection refer to original rows.
