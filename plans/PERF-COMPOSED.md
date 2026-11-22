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
p99 is the maximum with this sample size. Timing uses the calling thread's
CPU clock. Allocation counts capture allocations and reallocations on that
thread during each update, including draws GPUI test mode performs when
effects settle. Mount covers Store and selection creation, view tree creation,
and the first draw. Filter reset and measurement bookkeeping run outside
the measurement scope. An equal event leaves the window clean.

`cargo test -p guiraffe-bench` gates the exact identity of the changed cached
row and counts selector evaluations at 1,000 and 10,000 rows. This scaling
assertion tracks today's one selector evaluation per live row. It will need
to tighten when [roadmap section E](ROADMAP-0.3.md#e-fine-grained-reactivity-below-the-view-response-to-a-critique-of-gpui)
adds indexed subscriptions and dirty-path traversal.

## Measurement environment

Busy host, AMD Ryzen 7 8745HS, Linux 7.2.2, performance governor,
`rustc 1.98.0 (88d9e12ae 2026-08-18)`, optimized release with line tables.
The first upstream and gpui-fast runs started at one-minute loads of
20.54 and 20.34 respectively. The backend timings were collected on
the same host without per-thread CPU affinity.

## Release measurements

Benchmark commit `ab30919`. Both complete runs were busy. The one-minute
loads ended at 21.15 upstream and 19.25 on gpui-fast. Counts are medians,
and inline row renders are executions of the row builder in the page's
render method. The equal rows use the original queued event path.
A dash means the backend does not expose the retained-view counter.

| Backend | Variant | Rows | Operation | Median CPU ms | p99 CPU ms | Allocations | Page renders | Row renders | Row selectors | Retained reused |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| upstream | inline | 1000 | label | 131.388022 | 168.139607 | 137174 | 1 | 1000 | 1000 | – |
| upstream | inline | 1000 | key | 137.037528 | 180.821728 | 137264 | 1 | 1000 | 0 | – |
| upstream | inline | 1000 | hover | 133.714207 | 172.621601 | 137175 | 1 | 1000 | 0 | – |
| upstream | inline | 1000 | equal | 0.005460 | 0.011401 | 7 | 0 | 0 | 0 | – |
| upstream | inline | 1000 | filter | 72.376779 | 98.121765 | 68682 | 1 | 500 | 1000 | – |
| upstream | inline | 1000 | mount | 154.656386 | 195.663378 | 228618 | 1 | 1000 | 1000 | – |
| upstream | cached | 1000 | label | 12.306123 | 13.916965 | 1305 | 1 | 1 | 1000 | – |
| upstream | cached | 1000 | key | 11.474947 | 14.052688 | 1264 | 1 | 0 | 0 | – |
| upstream | cached | 1000 | hover | 13.326017 | 14.800574 | 1437 | 1 | 2 | 0 | – |
| upstream | cached | 1000 | equal | 0.005020 | 0.006762 | 7 | 0 | 0 | 0 | – |
| upstream | cached | 1000 | filter | 98.152593 | 111.934598 | 66051 | 1 | 499 | 1000 | – |
| upstream | cached | 1000 | mount | 205.820386 | 240.267846 | 226624 | 1 | 1000 | 1000 | – |
| upstream | inline | 3000 | label | 536.891450 | 576.013445 | 411174 | 1 | 3000 | 3000 | – |
| upstream | inline | 3000 | key | 465.276524 | 525.626891 | 411264 | 1 | 3000 | 0 | – |
| upstream | inline | 3000 | hover | 489.408482 | 551.526422 | 411175 | 1 | 3000 | 0 | – |
| upstream | inline | 3000 | equal | 0.016962 | 0.021901 | 7 | 0 | 0 | 0 | – |
| upstream | inline | 3000 | filter | 307.376691 | 327.642858 | 205682 | 1 | 1500 | 3000 | – |
| upstream | inline | 3000 | mount | 850.176943 | 954.259767 | 683643 | 1 | 3000 | 3000 | – |
| upstream | cached | 3000 | label | 43.941485 | 46.374239 | 3305 | 1 | 1 | 3000 | – |
| upstream | cached | 3000 | key | 41.337571 | 45.682383 | 3264 | 1 | 0 | 0 | – |
| upstream | cached | 3000 | hover | 47.427646 | 49.181230 | 3437 | 1 | 2 | 0 | – |
| upstream | cached | 3000 | equal | 0.017513 | 0.021560 | 7 | 0 | 0 | 0 | – |
| upstream | cached | 3000 | filter | 323.618739 | 409.033241 | 198051 | 1 | 1499 | 3000 | – |
| upstream | cached | 3000 | mount | 981.160511 | 1165.263606 | 677692 | 1 | 3000 | 3000 | – |
| upstream | inline | 10000 | label | 1951.604353 | 2022.984593 | 1370174 | 1 | 10000 | 10000 | – |
| upstream | inline | 10000 | key | 1917.446030 | 1996.327204 | 1370264 | 1 | 10000 | 0 | – |
| upstream | inline | 10000 | hover | 1896.649331 | 1947.980543 | 1370175 | 1 | 10000 | 0 | – |
| upstream | inline | 10000 | equal | 0.061215 | 0.623118 | 7 | 0 | 0 | 0 | – |
| upstream | inline | 10000 | filter | 1105.846285 | 1135.874889 | 685182 | 1 | 5000 | 10000 | – |
| upstream | inline | 10000 | mount | 8412.888215 | 11784.226318 | 2276205 | 1 | 10000 | 10000 | – |
| upstream | cached | 10000 | label | 124.005253 | 147.411661 | 10305 | 1 | 1 | 10000 | – |
| upstream | cached | 10000 | key | 116.415146 | 149.213910 | 10264 | 1 | 0 | 0 | – |
| upstream | cached | 10000 | hover | 137.037323 | 160.216136 | 10437 | 1 | 2 | 0 | – |
| upstream | cached | 10000 | equal | 0.043542 | 0.050645 | 7 | 0 | 0 | 0 | – |
| upstream | cached | 10000 | filter | 1099.781407 | 1188.938317 | 660051 | 1 | 4999 | 10000 | – |
| upstream | cached | 10000 | mount | 15125.181238 | 17086.620592 | 2256321 | 1 | 10000 | 10000 | – |
| gpui-fast | inline | 1000 | label | 107.484598 | 110.831469 | 86228 | 1 | 1000 | 1000 | 0 |
| gpui-fast | inline | 1000 | key | 82.302640 | 113.818553 | 86320 | 1 | 1000 | 0 | 0 |
| gpui-fast | inline | 1000 | hover | 79.536563 | 104.645559 | 86226 | 1 | 1000 | 0 | 0 |
| gpui-fast | inline | 1000 | equal | 0.005100 | 0.009007 | 7 | 0 | 0 | 0 | 0 |
| gpui-fast | inline | 1000 | filter | 55.782550 | 71.415432 | 43234 | 1 | 500 | 1000 | 0 |
| gpui-fast | inline | 1000 | mount | 135.863011 | 191.236181 | 263986 | 1 | 1000 | 1000 | 0 |
| gpui-fast | cached | 1000 | label | 10.343150 | 14.223352 | 4359 | 1 | 1 | 1000 | 999 |
| gpui-fast | cached | 1000 | key | 9.628409 | 13.901950 | 4348 | 1 | 0 | 0 | 1000 |
| gpui-fast | cached | 1000 | hover | 10.787254 | 13.003862 | 4460 | 1 | 2 | 0 | 998 |
| gpui-fast | cached | 1000 | equal | 0.005180 | 0.008386 | 7 | 0 | 0 | 0 | 0 |
| gpui-fast | cached | 1000 | filter | 70.829044 | 86.356612 | 53664 | 1 | 499 | 1000 | 1 |
| gpui-fast | cached | 1000 | mount | 210.474682 | 254.304232 | 282059 | 1 | 1000 | 1000 | 0 |
| gpui-fast | inline | 3000 | label | 346.588521 | 359.106968 | 258228 | 1 | 3000 | 3000 | 0 |
| gpui-fast | inline | 3000 | key | 337.414995 | 352.802192 | 258320 | 1 | 3000 | 0 | 0 |
| gpui-fast | inline | 3000 | hover | 343.091727 | 362.699670 | 258226 | 1 | 3000 | 0 | 0 |
| gpui-fast | inline | 3000 | equal | 0.021110 | 0.024897 | 7 | 0 | 0 | 0 | 0 |
| gpui-fast | inline | 3000 | filter | 200.735015 | 240.382811 | 129234 | 1 | 1500 | 3000 | 0 |
| gpui-fast | inline | 3000 | mount | 753.907014 | 811.165529 | 789026 | 1 | 3000 | 3000 | 0 |
| gpui-fast | cached | 3000 | label | 46.114720 | 50.030013 | 12365 | 1 | 1 | 3000 | 2999 |
| gpui-fast | cached | 3000 | key | 46.358680 | 48.288916 | 12354 | 1 | 0 | 0 | 3000 |
| gpui-fast | cached | 3000 | hover | 48.612551 | 51.985102 | 12466 | 1 | 2 | 0 | 2998 |
| gpui-fast | cached | 3000 | equal | 0.018094 | 0.022172 | 7 | 0 | 0 | 0 | 0 |
| gpui-fast | cached | 3000 | filter | 280.984640 | 293.205122 | 160671 | 1 | 1499 | 3000 | 1 |
| gpui-fast | cached | 3000 | mount | 973.174254 | 1187.006651 | 843155 | 1 | 3000 | 3000 | 0 |
| gpui-fast | inline | 10000 | label | 1203.955112 | 1242.520420 | 860228 | 1 | 10000 | 10000 | 0 |
| gpui-fast | inline | 10000 | key | 1206.905463 | 1283.593724 | 860320 | 1 | 10000 | 0 | 0 |
| gpui-fast | inline | 10000 | hover | 1222.536329 | 1242.213397 | 860226 | 1 | 10000 | 0 | 0 |
| gpui-fast | inline | 10000 | equal | 0.066325 | 0.102082 | 7 | 0 | 0 | 0 | 0 |
| gpui-fast | inline | 10000 | filter | 865.227069 | 893.305424 | 430234 | 1 | 5000 | 10000 | 0 |
| gpui-fast | inline | 10000 | mount | 10997.627879 | 13470.175584 | 2626633 | 1 | 10000 | 10000 | 0 |
| gpui-fast | cached | 10000 | label | 122.668771 | 158.500078 | 40371 | 1 | 1 | 10000 | 9999 |
| gpui-fast | cached | 10000 | key | 119.671053 | 154.261840 | 40360 | 1 | 0 | 0 | 10000 |
| gpui-fast | cached | 10000 | hover | 128.093413 | 151.722427 | 40472 | 1 | 2 | 0 | 9998 |
| gpui-fast | cached | 10000 | equal | 0.038281 | 0.042579 | 7 | 0 | 0 | 0 | 0 |
| gpui-fast | cached | 10000 | filter | 703.988688 | 964.666521 | 535180 | 1 | 4999 | 10000 | 1 |
| gpui-fast | cached | 10000 | mount | 12312.031885 | 14107.140139 | 2806856 | 1 | 10000 | 10000 | 0 |

The 10,000-row mounts took 8.4–15.1 seconds at the median. Source inspection
shows a quadratic registration term in `Store::register`, which prunes the
existing selection slots on every registration.

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
| Upstream | 10,000 | 43.542 / 50.645 | 1.854 / 2.093 | 7 → 0 |
| gpui-fast | 1,000 | 5.180 / 8.386 | 1.252 / 1.543 | 7 → 0 |
| gpui-fast | 3,000 | 18.094 / 22.172 | 1.263 / 1.513 | 7 → 0 |
| gpui-fast | 10,000 | 38.281 / 42.579 | 1.653 / 2.074 | 7 → 0 |

Both variants and all three sizes measured zero allocations, selector
evaluations and renders on the synchronous equal write. The inline
median ranged from 1.042 to 1.623 µs. The gate also
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
reuse plus retained-record splices, including non-row views. The vendored API
exposes no independent paint replay counter. Use
`callgrind_annotate --inclusive=yes --tree=both target/composed.callgrind`
to inspect calls to the replay functions.

### Cached label edit at 3,000 rows

Callgrind 3.25.1 collected 14 scoped edits per backend on benchmark commit
`ab30919`, with the initial mount outside the collection scope. Callgrind
reports executed guest instruction counts. The percentages below include callees
unless marked exclusive. Nested percentages overlap.

| Backend | Total instructions | Instructions per edit | Window draw | Root prepaint incl. layout | Store row-selection refresh |
| --- | ---: | ---: | ---: | ---: | ---: |
| Upstream | 1,202,684,381 | 85,906,027 | 96.69% | 43.89% | 1.81% |
| gpui-fast | 1,184,221,856 | 84,587,275 | 95.75% | 42.90% | 2.37% |

Upstream spent 20.91% in `Window::reuse_prepaint`, including dispatch
subtree reuse, another 6.95% in `Window::reuse_paint`, and 10.64% in
Taffy layout. The largest individual exclusive entry was libc memory
copy at 8.92%. Dispatch-tree reuse, push and drop appear separately
among the top exclusive entries.

On gpui-fast, `reuse_retained_prepaint` accounted for 19.44% and
`reuse_retained_paint` for 5.63%. Dispatch record copying accounted
for 6.32% inclusive, and libc memory copy for 14.29% exclusive.
Callgrind recorded **41,986 calls to each retained replay function**,
exactly 2,999 per edit. This confirms that the frame reaches each clean
row's boundary and replays its existing data. Clean rows skip their
render bodies and element-tree rebuild, while the parent still enumerates
row views and the frame processes their records. On both backends the
render counter saw one row rerender and the Store visited all 3,000
row selectors. The fast backend reported 999 reused views at 1,000 rows
and 9,999 at 10,000, matching the linear replay pattern.

## Comparison with the critique

The critique reports 8.4 ms upstream and 3.4 ms on gpui-fast for a cached
label change at 3,000 live rows. Its apply-and-settle figures use different
backend revisions, an i9-13900K and an unpublished harness. The table
here uses calling-thread CPU time for Store dispatch and one headless
draw with GUIraffe Controls. At 3,000 cached rows the measured median was
43.94 ms upstream and 46.11 ms on gpui-fast, about 5.2× and 13.6× the
critique's figures respectively. Our gpui-fast run executed about 1.5%
fewer instructions but used 4.9% more thread CPU time than upstream
on this host. The shared shape is one changed row with work that
still scales with the width of the live tree. These timings combine
different hardware, controls, Store work and backend revisions, so
the ratios describe these measurements only. [A Critique of GPUI](https://blog.zortax.de/posts/a-critique-of-gpui)

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
