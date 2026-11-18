# Timeline production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | 100k bars, clips and keys. Existing [validation](VALIDATION-3.md) covers interaction fixtures at demo sizes. |
| §2 Latency | ⚠️ Open | Row/time culling and shared viewport logic exist. Measure each editor's native interaction, first useful frame and edit recompute, including large curve and hierarchy changes. |
| §3 Memory | ⚠️ Open | Document channel/key/clip/bar retention, visible geometry storage and allocation traffic during paint and edits. |
| §4 Correctness and robustness | ⚠️ Open | [Interaction tests](tests/interactions.rs) and pure unit tests cover editing and some degenerate bar inputs. Add randomized edit references, nonfinite/extreme coordinates, multilingual labels, DPI/theme changes and input bursts. Audit cancellation and panic behavior. |
| §5 Long-session and stress | ⚠️ Open | Add native editing, scrub, zoom and pan replay across editor types. |
| §6 Accessibility and UX | ⚠️ Open | [Keyboard](tests/keyboard.rs) and [snapshot](tests/snapshots.rs) coverage is at demo sizes. Verify announcements, focus under updates and scale snapshots in both themes/backends. |
| §7 Process | ⚠️ Open | Initial audit recorded. New release measurements, automated acceptance and independent review remain open. |

Latency acceptance requires p99 ≤8 ms, maximum ≤16 ms, UI tasks ≤50 ms and cold mount ≤200 ms. Nightly replay must sample frame percentiles and RSS/PSS every 10 seconds and enforce the 2% warmed drift limit. Review the [baseline policy](../../bench/baselines/README.md) before recording reference results.
