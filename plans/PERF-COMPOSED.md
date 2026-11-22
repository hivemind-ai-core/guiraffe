# Composed-app reactivity benchmark (0.3 E.1)

The benchmark mounts 1,000, 3,000 or 10,000 live rows in a fixed 420 × 800
headless viewport. Every row is laid out, and none is virtualized. The rows
have fixed outer dimensions and contain text, a Controls checkbox, badge and
button. The page has a focused Controls text input. In the inline variant the
page reads one `Selected<Row>` per row and builds the rows itself. In the cached
variant each row ID owns a view with its own selection and fixed-size cached
boundary. Both variants use the same row builder and Store. Filtering shows
even IDs and preserves the cached view entities for subsequent updates.

## Running the benchmark

```sh
mkdir -p target/tmp
export TMPDIR="$PWD/target/tmp" CARGO_BUILD_JOBS=4
cargo test --release -p guiraffe-bench --test composed -- --ignored --nocapture
cargo test --release -p guiraffe-bench --test composed --features gpui-fast -- --ignored --nocapture
```

The package feature forwards to `gpui-kit/gpui-fast` and enables collection of
its backend-specific layout counters. The benchmark uses the real platform
text system and bundled assets, with headless rendering and no GPU submission.
Four warm-up samples precede 31 measurements per operation. The nearest-rank
p99 is the maximum with this sample size. Timing uses the
calling thread's CPU clock. Allocation counts capture allocations and
reallocations on that thread during each update, including any dirty-window draws performed by GPUI
test mode when effects settle. The mount includes building the Store, selections and view tree and
the first draw. Filter reset and measurement bookkeeping are outside the
measured operation. The harness lets the backend decide whether to draw. An equal event
leaves the window clean.

`cargo test -p guiraffe-bench` gates the exact identity of the changed cached
row and counts selector evaluations at 1,000 and 10,000 rows. This scaling
assertion tracks today's one selector evaluation per live row. It will need
to tighten when [roadmap section E](ROADMAP-0.3.md#e-fine-grained-reactivity-below-the-view-response-to-a-critique-of-gpui)
adds indexed subscriptions and dirty-path traversal.

## Queued equal-write baseline

On the busy Ryzen 7 8745HS host (one-minute load above 20), a queued equal
label write at 1,000 cached rows took 5.020 µs median (6.762 µs p99) on
upstream and 5.180 µs median (8.386 µs p99) on gpui-fast. Both allocated
seven times per median write and evaluated zero row selectors. The
render and notification gate observed no activity on the page or
the edited row. The queued path still boxed the event, deferred its
flush, formatted the event description and constructed a tentative
copy of the state before recognizing equality. The page-side guard
reached zero allocations by avoiding dispatch altogether.

## Synchronous equal writes

`Store::dispatch_now` runs the typed event directly when the Store is idle.
The label handler uses `Fx::set_field` to compare the field before taking
a mutable copy of the state. Both paths still process the event through
the engine, and an installed interceptor still receives its event
record. When dispatch is already batched, the event joins the existing
queue in FIFO order.

| Backend | Cached rows | Before median / p99 (µs) | After median / p99 (µs) | Allocations before → after |
| --- | ---: | ---: | ---: | ---: |
| Upstream | 1,000 | 5.020 / 6.762 | 1.192 / 1.423 | 7 → 0 |
| Upstream | 3,000 | 17.513 / 21.560 | 1.123 / 1.332 | 7 → 0 |
| gpui-fast | 1,000 | 5.180 / 8.386 | 1.252 / 1.543 | 7 → 0 |
| gpui-fast | 3,000 | 18.094 / 22.172 | 1.263 / 1.513 | 7 → 0 |

Both variants and all three sizes measured zero allocations, selector
evaluations and renders on the synchronous equal write. The gate also
checks that the Store processed the event. These measurements cover
an idle Store without an interceptor. Re-run the equal-write portion
with `COMPOSED_EQUAL_ONLY=1`. The after measurements ran on the same
busy host at a one-minute load of roughly 15–20.

## Profiling

Build each release executable with
`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` and run the following with the
path printed by `cargo test --release -p guiraffe-bench --test composed --no-run`.
Set `COMPOSED_PROFILE=1` to select only the cached 3,000-row label workload.

```sh
COMPOSED_PROFILE=1 valgrind --tool=callgrind --collect-atstart=no \
  --toggle-collect='composed::label_edit' --callgrind-out-file=target/composed.callgrind \
  "$bench_binary" --ignored --exact composed_measurements --nocapture
callgrind_annotate --inclusive=yes target/composed.callgrind
```

The fast backend's `LayoutStats::views_reused` counts retained prepaint
reuse plus retained-record splices. It is a count of reused views, including
non-row views. The vendored API exposes no independent paint replay count.
The counter indicates that clean view content gets reused. It cannot
distinguish a visit to each clean view from a bulk reuse of their records.

## Comparison with the critique

[A Critique of GPUI](https://blog.zortax.de/posts/a-critique-of-gpui)
reports 8.4 ms upstream and 3.4 ms on gpui-fast for a cached label
change at 3,000 live rows. Its apply-and-settle figures use different
backend revisions, an i9-13900K and an unpublished harness. The table
here uses calling-thread CPU time for Store dispatch and one headless
draw with GUIraffe Controls. The useful comparison is the size of the
cached-update cost and how it changes with row count.

## Roadmap recommendations

- **E.2, reactive sinks** — Start with a keyed Store write routed directly
  to its bound text sink. Preserve equality checks and history, and keep a
  stable subscription ID per sink. The current full-state comparison,
  copy-on-write snapshot, selection refresh and prune need an indexed change
  path before the label edit can be independent of row count. A focused text
  prototype looks like one to two engineer-weeks, with several more weeks for
  style, visibility, keyed ranges and lifecycle coverage.
- **E.3, dirty paths** — Route paint-only invalidation to retained dirty
  descendants and replay contiguous clean frame records in bulk. Cover hit
  testing, focus, dispatch, text metrics and layout-invalidating edits.
  Allow an affected layout to visit siblings when geometry really changes.
  This looks like several engineer-weeks in the vendored backend.
- **E.4, cheap boundaries** — Add stable keyed RenderOnce lifetimes that
  reuse E.2's dependency versions and E.3's skip contract. Keep identity
  across reorder and release subscriptions on removal. Estimate one to two
  engineer-weeks for keyed rows after those prerequisites, plus migration
  of real controls. Filtering half the rows and mounting all rows inherently
  touch a number of elements proportional to the requested change.
