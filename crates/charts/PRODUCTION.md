# Charts production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | Generated one line with 10M points and one scatter plot with 1M points across 100 series. CPU line scene compilation is fast once prepared. Dense scatter and cold preparation remain outside their budgets. |
| §2 Latency | ⚠️ Open | 10M line preparation median 1.98 s. Full scene plus three zoomed widths took 0.360 ms. Full 1M-point/100-series scatter scene compilation took 47.45 ms. Prepared geometry is currently compiled synchronously in the view. Cancellable off-thread generation, progress and tail budgets remain open. |
| §3 Memory | ⚠️ Open | Prepared line buffers have a 256 B/point structural gate for sample, LTTB and line-break buffers at 100k. Allocator gates cover existing smaller scenes and full million-point scatter recompute. Chart row strings, other retained storage, native paint and 30-minute memory validation remain open. |
| §4 Correctness and robustness | ⚠️ Open | Randomized stream chunk append/snapshot behavior matches a flat vector, in addition to existing scale, sampling and stack properties. A red/green test covers non-drawable dense bubble representatives. The complete rendering/interaction edge matrix, cancellation and fuzz verification remain open. |
| §5 Long-session and stress | ⚠️ Open | The shared headless soak runner has line/scatter interaction modes and samples frames and memory. Native inspect/drive and 10k points/s sustained streaming remain open, as does the full 30-minute run. |
| §6 Accessibility and UX | ⚠️ Open | [Native interactions](tests/interactions.rs) cover chart controls and retention. Verify focus and screen-reader state under data updates, and capture scale snapshots across both themes/backends. |
| §7 Process | ⚠️ Open | Audit, regression checks and exploratory release measurements recorded. Automated end-to-end acceptance, quiet reference baselines and independent review remain open. |

Latency acceptance requires p99 ≤8 ms, maximum ≤16 ms, UI tasks ≤50 ms and cold mount ≤200 ms. Nightly replay must sample frame percentiles and RSS/PSS every 10 seconds and enforce the 2% warmed drift limit. Review the [baseline policy](../../bench/baselines/README.md) before recording reference results.

Scale workloads and checks are registered in [suites.rs](../../bench/src/suites.rs),
[allocator.rs](../../bench/tests/allocator.rs) and the [headless soak runner](../../bench/examples/soak.rs).

Exploratory release runs used rustc 1.98.0 on the AMD Ryzen 7 8745HS with
Radeon 780M graphics and the powersave governor. Ten samples used a minimum
one invocation per batch. Machine load was 17.37/16.14/13.28 before and
16.32/15.95/13.31 afterward. CPU workloads above were measured at `a6e7ede`,
except timeline clip navigation at `5ae9382`. The host was shared and loaded,
so these measurements do not qualify as reference baselines.

The 10M-line headless short replay mounted in 3.25 s including data
preparation. Its second sample had 33.0 ms p99, 41.8 ms maximum and
1,905,300 KiB RSS, 336 KiB higher than the warmed sample. Short mode
exercises the runner and supplies diagnostics without asserting absolute timing.
