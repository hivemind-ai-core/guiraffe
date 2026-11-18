# Charts production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | 10M line points, 1M scatter points, 100 series. Existing [benchmarks](../../bench/src/suites.rs) cover 1M line and 100k scatter. |
| §2 Latency | ⚠️ Open | Prepared scenes and cached hover exist. [Historical measurements](VALIDATION.md) cover CPU compilation only. Verify native frame tails and cold mount. Streaming prepares accumulated data again and requires cancellation and sustained-throughput evidence. |
| §3 Memory | ⚠️ Open | Existing [allocator checks](../../bench/tests/allocator.rs) limit scene allocation calls. Measure retained bytes per encoded row, index and hierarchy, and verify warm native paint/navigation allocations. |
| §4 Correctness and robustness | ⚠️ Open | Scale, LTTB and stacking properties exist. Add a naive interaction/stream reference and native DPI/theme/input-burst scenarios. Audit malformed inputs and background cancellation on supersede/drop. |
| §5 Long-session and stress | ⚠️ Open | Replay pan, zoom, hover and selection with sustained ingestion at 10k points/s for 30 minutes. |
| §6 Accessibility and UX | ⚠️ Open | [Native interactions](tests/interactions.rs) cover chart controls and retention. Verify focus and screen-reader state under data updates, and capture scale snapshots across both themes/backends. |
| §7 Process | ⚠️ Open | Initial audit recorded. New release measurements, automated acceptance and independent review remain open. |

Latency acceptance requires p99 ≤8 ms, maximum ≤16 ms, UI tasks ≤50 ms and cold mount ≤200 ms. Nightly replay must sample frame percentiles and RSS/PSS every 10 seconds and enforce the 2% warmed drift limit. Review the [baseline policy](../../bench/baselines/README.md) before recording reference results.
