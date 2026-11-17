# Data UI review fixes — 2026-11-16

Worktree `feat-data-ui-2`; no merge or push. This report supersedes the behavioral limits and measurements in `VALIDATION-2.md`. Earlier artifacts remain historical evidence for their recorded revisions.

Commits:

- `f284bfc` — scoped repeat events, item-relative write-back, guarded transactions, live position handles, native control observers, natural/proven-uniform layouts, duplicate recovery, real render instrumentation, and regression tests/example.
- `fc4dfe4` — structural-splice workload and settled criterion/Callgrind setup.
- `7ecd423` — exercise benchmark preparation before/after native allocation checks.

## Review disposition

1. **Interactive rows:** all row button/input events deliver `Payload::Scoped`, with live key/index, stable identity, source slot, repeat and parent context. `bind.value = @item.field` gets optimistic guarded field write-back. Logic can normalize via `out.item(payload.item_change().with_value(...))` or veto. Cached callbacks resolve current order-statistics handles, not captured indices. Unqualified names remain intentional global/shared slots; examples/tests use explicit item-relative targets for independent row state.
2. **Layout:** multiline and multi-control templates use natural height. Only a resolved factory with a bounded-height leaf contract may virtualize. Text opts in with `single-line: true`, and overrides of a builtin name default to unknown height. Short/empty lists have no eight-row minimum. Variable-height virtualization is still deferred; such templates render naturally.
3. **Duplicate keys:** a surviving duplicate is promoted when its previous key owner disappears. Equal-position field edits retain unkeyed/duplicate input state; duplicate identities never alias.
4. **Instrumentation:** `host_renders()` increments only in the actual host Render implementation; the tautological fallback-refresh counter and assertion were removed. Tests assert nonzero measured baselines, fast-backend host retention on a layout-neutral event/replacement, and no-op retention on both backends, alongside actual slot values and native input state.
5. **Veto ordering:** veto discards every response patch, including normalization, and restores the previous target. Optimism is observable to logic synchronously; native synchronization is deferred/coalesced until the response has completed, before the next ordinary frame. This rule is documented and tested.
6. **Unknown input events:** rejected before optimistic data mutation, matching the legacy dispatch path.

The remaining nits are addressed: item metrics are test-support-only and use the template's key path and an optional parent scope; native controls synchronize through model observers, with App subscriptions and pure view notifications instead of parent-render control updates; the unused Window placeholder was removed.

The structural benchmark exposed an additional re-entrancy failure in direct headless App access: creating a native row entity during a borrowed reconciliation could flush a draw into the half-mutated model. Row views now allocate lazily during rendering, and offscreen rows retain model identity without an eager native entity. Reconciliation only mutates model data and queues observers/notifications. The new workload and preparation gate exercise that path.

## Passing gates

Both upstream GPUI and `gpui-kit/gpui-fast`:

- `cargo test --locked -p guiraffe-ui -p guiraffe-bench`: **81 passed per backend**, zero failures/ignored. UI: 50 (13 unit, nine legacy document/interaction, 13 follow-up, 14 scoped-row/layout regressions, one doctest); bench: 31.
- All-targets Clippy with `-D warnings`, including optional `guiraffe-bench/instruction-counts` wrappers: passed. Instruction counts themselves were not run.
- Production `cargo clippy --locked -p guiraffe-ui --lib -- -D warnings`: passed without test-support; downstream Mocket/CLI compile checks passed without warnings.
- Scoped `cargo fmt ... -- --check` and `git diff --check`: passed.

Native interaction tests cover per-row checkbox/delete, text typing and normalization/veto across reorder, a deleted item's delayed patch, rebasing an item patch after an earlier delete in the same response, custom value/change widget handlers, and unknown-event rejection. Layout tests check natural multiline height, short-list bounds, and factory proof selection. Pure position tests compare 1,500 generated edit sequences against a Vec oracle, exercise reordering/removal, and detach external handles when their tree drops. Existing hot-reload/generation tests remain green.

Tests use `gpui_kit::GPUI_FAST` for backend assertions. Builds used direct stable Rust 1.98.0 binaries and the shared `feat-chat/target` cache. The example was compiled; no live desktop visual run was performed.

## Fresh allocation and timing evidence

Measured source: clean `7ecd4237f2b794f1a6117746f7be6d249d50d458`. Both backends have identical warmed allocation traffic:

| Operation | Allocation/reallocation calls | Requested bytes |
|---|---:|---:|
| Host construction | 12 | 972 |
| Scalar patch | 4 | 632 |
| Same-key middle splice | 7 | 1,540 |
| Structural middle-key replacement | 12 | 2,044 |
| Replacement with retained 100k-item snapshot | 6 | 7,608 |

Counts are cumulative requested allocation traffic, not live/peak memory. Scoped payloads and item patches use shared indirection so they do not inflate the common scalar message variants; a size regression test enforces that property. Native allocation budgets remain unchanged, and prepare/settle is exercised outside the measured allocation scope.

Fresh artifacts under `bench/measurements/`:

- `data-ui-review-allocations.json`
- `data-ui-review-debug-upstream.json`
- `data-ui-review-debug-fast.json`

Debug smoke medians in microseconds for host / scalar patch / same-key splice / structural splice were 1.436 / 1.716 / 5.295 / 16.184 upstream and 1.453 / 1.697 / 5.273 / 16.139 fast. Raw records retain ten samples, MAD/confidence intervals, toolchain, CPU, governor, load and backend notes. This was an uncontrolled shared host: these are **not release baselines, paired comparisons, frame times, or speedup claims**. Setup/settling and subsequent draw are excluded; instantiation measures host construction from a compiled template, excluding lazy child rendering/list seeding.

Known boundaries remain explicit: variable-height lists use natural layout, moving bounds can cause re-renders despite preserved identity, and item-relative size/color expressions are not implemented. The blocking row interaction/write-back issue and all enumerated review fixes above are resolved.
