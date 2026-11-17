# Data UI follow-ups — 2026-11-16

Historical first-pass evidence. Review fixes and current validation are recorded in `VALIDATION-3.md`; the fixed-height and item-write limits below describe the earlier measured revision.

Worktree `feat-data-ui-2`, based on release `fd7ae98`. Implementation commits:

- `d5b311a`: keyed repeats, conditional nodes, persistent splice patches, native two-way Controls, compact payloads, typed bindings, compiled/cached style recipes, example and tests.
- `9e2e4d4`: registered native instantiate/patch/splice benchmarks, allocation gates, criterion/Callgrind setup, and benchmark documentation.

Only `crates/ui/**`, the explicitly requested `bench/**` integration, and their three lockfile dependency entries changed. No merge or push was performed.

## Validation

Both upstream GPUI and `--features gpui-kit/gpui-fast` pass:

- `cargo test --locked -p guiraffe-ui -p guiraffe-bench`: **62 tests per backend**, zero failures/ignored tests. UI has 31 (8 pure unit tests, 9 existing document/interaction tests, 13 follow-up tests, one doctest); bench has 31 (15 unit, seven allocation, one process-allocation, two CLI, five comparison, one doctest).
- `cargo clippy -p guiraffe-ui -p guiraffe-bench --all-targets --features guiraffe-bench/instruction-counts -- -D warnings`, adding `gpui-kit/gpui-fast` for the alternate backend: passed. This compiles the example, criterion benches, and optional Callgrind wrappers; instruction counts themselves were not executed.
- `cargo check --locked -p guiraffe-mocket -p guiraffe-cli`: passed, checking downstream consumers without editing their files.
- `cargo fmt -p guiraffe-ui -p guiraffe-bench -- --check` and `git diff --check`: passed.

The environment uses direct stable Rust 1.98.0 binaries in PATH because its rustup proxy resolves the wrong invoked name. Builds reused `feat-chat/target`. These are native GPUI TestAppContext/headless checks; the example was compiled, not interactively launched.

Pure coverage includes all small list lengths/ranges/insertion lengths against a Vec oracle, checked/unknown slots, no-op/version behavior, immutable snapshots, string/numeric key separation, duplicate/missing key non-aliasing, keyed update/reordering, typed inert IDs and diagnostics, item expressions, implied events, and style token recipes/caches. Existing depth, background compilation, hot reload, stale generation and legacy Logic tests remain green.

Interaction tests use actual GPUI text input, checkbox, switch, and select popup. They verify payload types, optimistic slot values, normalization, veto repairing native text state, disabled behavior, conditions, and keyed identities. On gpui-fast, replacing one keyed item leaves other row render counts unchanged; insertion preserves the unmoved row count, and reordering preserves identities. Tests branch on `gpui_kit::GPUI_FAST`. Event dispatch has no host entity write or host-refresh fallback; invalidations use `cx.notify(entity_id)` for affected leaves.

## Allocation and timing artifacts

The three suites exercise real DataUi entities. A shared headless fixture contains a 10,000-item keyed list, with setup/initial drawing excluded from warm measurement. Instantiation measures host construction from a compiled four-node template with no-op logic, excluding lazy child rendering/list initialization. Patch changes a text color; splice replaces one middle item. Updates include reconciliation and notifications, excluding the following frame.

Measured calling-thread allocation traffic is identical on both backends:

| Operation | Allocations/reallocations | Requested bytes |
|---|---:|---:|
| Host instantiate | 11 | 868 |
| Scalar patch | 3 | 344 |
| Middle-item splice | 5 | 1,160 |
| Splice with retained 1,000-item snapshot | 5 | 6,040 |
| Splice with retained 100,000-item snapshot | 6 | 7,608 |

The snapshot test verifies the old value remains intact. Budgets are explicit per operation; the original core/window zero-allocation assertions remain enforced. Counts describe cumulative requested allocation traffic, not peak/live memory.

Raw artifacts in `bench/measurements/`:

- `data-ui-2-allocations.json`
- `data-ui-2-debug-upstream.json`
- `data-ui-2-debug-fast.json`

Timing records identify clean source revision `9e2e4d488013afbdc82c2b3b2184860b830ace89`, toolchain, CPU, governor, load, profile, backend notes, ten samples, MAD and bootstrap confidence intervals. Debug smoke medians (microseconds per operation) were 2.037 / 1.970 / 5.810 upstream and 1.629 / 1.826 / 5.730 fast for instantiate / patch / splice. These runs used an uncontrolled shared development host and are **not release baselines, A/B comparisons, frame times, or speedup claims**.

## Deliberate limits

- Repeats currently use an eight-row viewport with equal control-height rows. Variable-height template measurement remains a follow-up.
- Structural insertion/reordering preserves keyed state but can re-render rows whose absolute bounds change. Replacement isolation is proved separately. Native focus/hover can also schedule broader frames; the event-path guarantee is that the host entity is never written to dispatch or refresh.
- Two-way inputs target global slots. Item-relative expressions are reads; list-field editing should emit splice patches from application logic. Select options/selection use shared strings.
- Literal size/color tokens compile once; global style slots cache parsing on value changes. Item-relative size/color expressions are unsupported.
- The typed helper is a builder API with collected diagnostics, rather than a procedural derive. Existing Value-based `Logic::on_event` implementations remain compatible through the new `on_payload` hook.
- The new benchmark harness uses GPUI test-support for real headless contexts. Release/quiet-host timing and live desktop visual validation were not performed.
