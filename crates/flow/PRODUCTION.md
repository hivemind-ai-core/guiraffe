# Flow production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | 50k nodes and 100k edges. Existing [stress](examples/stress.rs) covers 5k/10k. |
| §2 Latency | ⚠️ Open | Viewport culling and path caching exist. [Historical CPU timings](PERFORMANCE.md) cover smaller scenes. Verify input-to-frame, cold mount, fit-all, and background layout cancellation. |
| §3 Memory | ⚠️ Open | Reusable core query buffers have an [allocation gate](../../bench/tests/allocator.rs). Native element construction allocates. Measure retained bytes per node, route/index overhead, viewport residency and recompute allocations. |
| §4 Correctness and robustness | ⚠️ Open | Core edit and spatial regression tests exist. Add randomized edit/query comparison, malformed geometry and native edge-input coverage. Audit serialization inputs, panic sites and background publication. |
| §5 Long-session and stress | ⚠️ Open | Add native pan, zoom, select and edit replay with memory and frame samples. |
| §6 Accessibility and UX | ⚠️ Open | Keyboard canvas commands exist. Audit handle navigation and announcement. Capture scale viewport positions on both themes and backends. |
| §7 Process | ⚠️ Open | Initial audit recorded. New release measurements, automated acceptance and independent review remain open. |

Latency acceptance requires p99 ≤8 ms, maximum ≤16 ms, UI tasks ≤50 ms and cold mount ≤200 ms. Nightly replay must sample frame percentiles and RSS/PSS every 10 seconds and enforce the 2% warmed drift limit. Review the [baseline policy](../../bench/baselines/README.md) before recording reference results.
