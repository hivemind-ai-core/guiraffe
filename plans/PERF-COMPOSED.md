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

## Follow-up on mount and ordinary queued writes

Run the same affected rows with `COMPOSED_FOLLOWUP=1` on the two release
commands above. This mode measures mounts at all three sizes and queued
label and equal writes at 3,000 rows, for both inline and cached views. The
equal event uses `Store::dispatch` and the existing `Fx::update` handler.
Both runs use four warm-ups and 31 samples. The baseline uses Store code
from `dd02483` with this measurement mode. The updated runs use `af88b72`.

The liveness regression in `9f3203b` failed with 499,500 checks at 1,000
registrations and 49,995,000 at 10,000. With `9d9658e`, neither size
requires a liveness probe during registration. A selection owns its
slot, and the Store tracks weak entries in registration order. Dropping
the last owner triggers a sweep once dead entries reach half the
registry. Each sweep removes at least half the entries, so total
cleanup work is linear in the number of drops. The test also checks
exact statistics and bounded entries after dropping all but three
selections, then dropping the remainder.

The baseline already skips debug formatting when no interceptor is
installed. `af88b72` also eliminates the temporary allocation for the
new `Arc<State>`. The handler works on a tentative state and publishes a
new snapshot only on change. The warmed equal write still owns a boxed
event in the heterogeneous FIFO and a boxed GPUI defer callback per
scheduled flush. `Fx::update` exposes arbitrary mutable access, so the
engine must retain the original for equality and clone the state before
calling the mutator. In this fixture the clone allocates the row vector
once. Initial queue growth can add another allocation. The existing
allocation test warms the FIFO and checks that the equal event runs
without selection or render activity.

All timings are provisional. The one-minute host load at each run's
start and finish was as follows. The differing loads and baseline spreads
limit cross-backend and label-timing comparisons. Label allocation counts
and selection/render activity matched before and after. Each equal write
evaluated zero selectors and rendered zero views.

| Backend | Before load (start → finish) | After load (start → finish) |
| --- | ---: | ---: |
| upstream | 3.42 → 13.14 | 6.72 → 4.02 |
| gpui-fast | 13.14 → 7.22 | 4.02 → 9.77 |

| Backend | Variant | Rows | Operation | Before CPU median / p99 (ms) | After CPU median / p99 (ms) | Median allocations before → after |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| upstream | inline | 1,000 | mount | 110.381312 / 117.765812 | 89.231639 / 117.938438 | 228,624 → 228,625 |
| upstream | inline | 3,000 | mount | 441.498026 / 467.832747 | 270.657110 / 289.066305 | 683,654 → 683,655 |
| upstream | inline | 3,000 | label | 315.802910 / 347.431460 | 282.842635 / 313.199053 | 411,171 → 411,171 |
| upstream | inline | 3,000 | equal | 0.011080 / 0.019747 | 0.010179 / 0.016802 | 4 → 3 |
| upstream | inline | 10,000 | mount | 2846.826700 / 4705.746848 | 925.950614 / 993.993659 | 2,276,266 → 2,276,267 |
| upstream | cached | 1,000 | mount | 105.561336 / 110.331571 | 86.231587 / 97.113937 | 226,621 → 226,622 |
| upstream | cached | 3,000 | mount | 427.512598 / 437.149746 | 263.567604 / 307.064521 | 677,690 → 677,691 |
| upstream | cached | 3,000 | label | 22.587179 / 27.750425 | 18.771860 / 23.479339 | 3,302 → 3,302 |
| upstream | cached | 3,000 | equal | 0.010350 / 0.010640 | 0.009798 / 0.011021 | 4 → 3 |
| upstream | cached | 10,000 | mount | 2896.798294 / 18143.046321 | 929.379920 / 1022.636893 | 2,256,315 → 2,256,316 |
| gpui-fast | inline | 1,000 | mount | 165.354610 / 212.904643 | 83.156167 / 88.446145 | 263,991 → 263,992 |
| gpui-fast | inline | 3,000 | mount | 708.700741 / 819.998276 | 268.324397 / 326.903134 | 789,040 → 789,041 |
| gpui-fast | inline | 3,000 | label | 329.716746 / 446.261791 | 186.751791 / 195.147570 | 258,225 → 258,225 |
| gpui-fast | inline | 3,000 | equal | 0.014618 / 0.027622 | 0.010119 / 0.023644 | 4 → 3 |
| gpui-fast | inline | 10,000 | mount | 9543.386381 / 10507.689819 | 1052.717474 / 1187.336559 | 2,626,721 → 2,626,722 |
| gpui-fast | cached | 1,000 | mount | 263.249295 / 296.006914 | 86.756641 / 91.886312 | 282,062 → 282,063 |
| gpui-fast | cached | 3,000 | mount | 842.849092 / 992.403798 | 326.780459 / 349.541352 | 843,151 → 843,152 |
| gpui-fast | cached | 3,000 | label | 38.937235 / 46.840186 | 29.050950 / 31.138256 | 12,362 → 12,362 |
| gpui-fast | cached | 3,000 | equal | 0.014477 / 0.019146 | 0.011111 / 0.012283 | 4 → 3 |
| gpui-fast | cached | 10,000 | mount | 2845.803139 / 7554.170949 | 1476.501846 / 1929.616687 | 2,806,849 → 2,806,850 |

`scripts/gate.sh` passed both backend test suites, including the composed
allocation test, plus per-crate doctests, Clippy and formatting. No golden
images changed. The foreman owns the final quiet run.

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
