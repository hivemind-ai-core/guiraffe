# Data UI follow-up review — 2026-11-17

Branch `fix-data-ui-3`, based on release `7df0b56`; no merge or push. Code commits: `bda134a` (indexed duplicate recovery and stronger regressions), `8071cbd` (configurable repeat viewport and key guard coverage). This supplements `VALIDATION-3.md`; older timing/allocation records describe their own revisions.

## Changes

- The duplicate survivor test now deletes a#1 from `[a#1, a#2]`, then replaces the remaining row with `[b, a']`. It checks that a#2 retains its state and live position through this length-changing splice, so positional reuse cannot mask missing promotion. Disabling the recovery pass produces the expected failing test (exit 101); restored recovery passes.
- Duplicate candidates are indexed by key and stable position handle. Reconciliation examines candidates only for released keys with existing duplicates, rather than rescanning all rows. Ambiguous candidates promote in creation order. A deterministic 1,000-splice test checks ownership, candidate liveness, position ranks and retained state after mixed duplicate/keyless edits. Ordinary unique-key splices do not allocate a released-key set.
- Uniform repeats now use content height until they exceed the window height. `DataUiHandle::set_repeat_max_height(Some(height), cx)` supplies a smaller panel limit; `None` restores the window default. The limit is finite and nonnegative, and zero is supported. Unchanged and invalid limits are inert. Pure sizing handles enormous counts without f32 overflow. Native layout coverage checks 40 rows in a 600px window, limits 0/160/240, no-op/error behavior, reset, window resize, and a fitting ten-row list at its full content height.
- A live row identity paired with another row's key is rejected by the runtime guard. Disabling only the key comparison produces the expected failing native interaction test (exit 101); restoring it passes.

## Validation

At code revision `8071cbd`, both upstream GPUI and `gpui-fast`:

- `cargo test --locked -p guiraffe-ui` (plus `--features gpui-fast`): **56 passed per backend**, zero failures/ignored: 17 unit, nine document/interaction, 13 existing follow-up, 16 scoped-row/layout, one doctest.
- `cargo clippy --locked -p guiraffe-ui --all-targets -- -D warnings` (plus `--features gpui-fast`): passed, including example compilation.
- Production-library Clippy without test-support: passed.
- Existing `guiraffe-bench` allocator regression `data_ui_native_workloads_have_bounded_allocation_traffic`: one test passed per backend. No benchmark source/budgets changed. Warmed calls/requested bytes were identical on both backends: construction 12/980, scalar patch 4/632, same-key splice 7/1540, structural splice 12/2044. The per-host sizing policy adds eight construction bytes compared with the previous report; patch/splice traffic is unchanged.
- `cargo fmt -p guiraffe-ui -- --check` and `git diff --check`: passed.

The direct stable Rust 1.98.0 toolchain and shared `feat-chat/target` cache were used. No desktop visual session or new timing benchmark was run. The bench result is the scoped allocation regression, not the entire benchmark/flow suite.

## Boundaries

The default height is the window viewport, not automatic measurement of an arbitrary parent panel; embedded panels supply their content-height limit. The limit is per host and applies only to factory-proven uniform repeats. Variable-height repeats keep natural layout and remain unvirtualized. No other crates were edited. The separate feat-code review was forwarded to its owner and is outside this branch.
