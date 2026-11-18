# Timeline production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | Generated 100k-key, 100k-clip and 100k-bar workloads. The dope-sheet native test executor also mounts a 100k-key document. Other family views still need scale rendering coverage. |
| §2 Latency | ⚠️ Open | Three pan/zoom/hit checks on 100k clips took 1.10 µs median. Three small box selections from 100k keys took 0.309 ms and two 100k-bar strokes took 1.13 ms. Headless dope-sheet paint fell outside both the mount and interaction budgets; dense visible keys still build individual paths. |
| §3 Memory | ⚠️ Open | Warm clip navigation/hit and standalone bar drawing allocate zero. Small box selection has an allocator gate. 100k standalone bars fit one f64 per value plus a shared vector header. Aggregate overhead, retained render state, warm GPUI allocation and leak limits remain open. |
| §4 Correctness and robustness | ⚠️ Open | Randomized clip insertion matches a linear overlap oracle. A red/green test prevents non-finite clip ends on insertion. Native tests exercise existing gestures. Full model-oracle coverage, malformed/large-input matrices, and off-thread job validation remain open. |
| §5 Long-session and stress | ⚠️ Open | The shared headless soak runner drives dope-sheet scroll and drag with 100k keys, including Arabic, Japanese and emoji labels. The 30-minute run and native inspect/drive and family-wide replay remain open. |
| §6 Accessibility and UX | ⚠️ Open | [Keyboard](tests/keyboard.rs) and [snapshot](tests/snapshots.rs) coverage is at demo sizes. Verify announcements, focus under updates and scale snapshots in both themes/backends. |
| §7 Process | ⚠️ Open | Initial audit recorded. New release measurements, automated acceptance and independent review remain open. |

Latency acceptance requires p99 ≤8 ms, maximum ≤16 ms, UI tasks ≤50 ms and cold mount ≤200 ms. Nightly replay must sample frame percentiles and RSS/PSS every 10 seconds and enforce the 2% warmed drift limit. Review the [baseline policy](../../bench/baselines/README.md) before recording reference results.

Scale workloads and checks are registered in [suites.rs](../../bench/src/suites.rs),
[allocator.rs](../../bench/tests/allocator.rs) and the [headless soak runner](../../bench/examples/soak.rs).

Exploratory release runs used rustc 1.98.0 on the AMD Ryzen 7 8745HS with
Radeon 780M graphics and the powersave governor. Ten samples used a minimum
one invocation per batch. Machine load was 17.37/16.14/13.28 before and
16.32/15.95/13.31 afterward. CPU workloads above were measured at `a6e7ede`,
except timeline clip navigation at `5ae9382`. The host was shared and loaded,
so these measurements do not qualify as reference baselines.
