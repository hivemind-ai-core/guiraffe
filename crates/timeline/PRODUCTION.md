# Timeline production-readiness audit

Audited against [spec/23](../../spec/23-production-readiness.md) at source revision `62b8de3`.

| Criterion | Status | Evidence and remaining checks |
| --- | --- | --- |
| §1 Scale | ⚠️ Open | Generated 100k-key, 100k-clip and 100k-bar workloads. The dope-sheet native test executor also mounts a 100k-key document. Other family views still need scale rendering coverage. |
| §2 Latency | ⚠️ Open | Three pan/zoom/hit checks on 100k clips took 1.10 µs median. Three small box selections from 100k keys took 0.014 ms after range indexing and two 100k-bar strokes took 1.13 ms. The [100k-key replay](../../bench/examples/soak.rs) exercises the dense undecorated key culling and diamond batching paths. The mount and interaction budgets remain open. |
| §3 Memory | ⚠️ Open | Warm clip navigation/hit and standalone bar drawing allocate zero. Small box selection has an allocator gate. 100k standalone bars fit one f64 per value plus a shared vector header. Aggregate overhead, retained render state, warm GPUI allocation and leak limits remain open. |
| §4 Correctness and robustness | ⚠️ Open | Randomized clip insertion matches a reference scan. [Dope model property tests](src/dope_model.rs) check box selection and atomic retiming against set-based references. A stale-row regression covers host channel removal. A red/green test prevents non-finite clip ends on insertion. Native tests exercise existing gestures. Full model-oracle coverage, malformed/large-input matrices, and off-thread job validation remain open. |
| §5 Long-session and stress | ⚠️ Open | The shared headless soak runner drives dope-sheet scroll and drag with 100k keys, including Arabic, Japanese and emoji labels. The 30-minute run and native inspect/drive and family-wide replay remain open. |
| §6 Accessibility and UX | ⚠️ Open | [Keyboard](tests/keyboard.rs) and [snapshot](tests/snapshots.rs) coverage is at demo sizes. Verify announcements, focus under updates and scale snapshots in both themes/backends. |
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

At 36.98/24.61/17.36 load before the run, the short 100k-key dope-sheet
replay took 802 ms from fixture creation through first frame. Its second
10-second window reported a 1.12 s p99 over 20 scripted iterations. RSS
increased by 24 KiB from 100,952 KiB between the two samples. A scripted drag
iteration dispatches several events and can paint more than once.

Box-select median fell from 0.309 ms to 0.014 ms after `65d401a`.
The second run had 33.92/33.73/24.56 load and modified report Markdown only.
The runs were sequential on a shared host and were not a controlled A/B.

The showcase opts Gantt and Tracks into focus on item clicks and displays
keyboard instructions. Its native Gantt regression clicks a bar, verifies
a one-day move at the default knob setting, clicks the live week option
and verifies a seven-day move on the same view. The keyboard tests also
check that views without an explicit opt-in preserve pointer focus.

## Viewport rendering bounds

- The bar editor retains up to 16,384 pairs of `f64` extrema (256 KiB of
  entries) and one dirty index range. The first dense render and a column
  count change scan the model. A stroke recomputes only columns intersecting
  its changed range. Paint emits at most one quad per column.
- Tracks overview uses the sorted, disjoint clip intervals already in the model
  and retains no overview geometry. Each visible lane emits at most one mark
  per logical pixel column, capped at 16,384 columns, with adjacent runs of the
  same color combined. The selected clip keeps its individual element.
  When assistive technology is active, individual visible clip elements expose
  their names, roles and selection state.
- The dope sheet retains one cached header view per visible row (including
  partially visible rows). The header cache reuses the same controls across
  pan, zoom and key edits. The key canvas uses four-pixel spacing for dense
  undecorated rows without selection. Decorated, summary and selected rows
  retain their individual marks.
