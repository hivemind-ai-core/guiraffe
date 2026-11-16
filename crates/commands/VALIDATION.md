# Commands handoff — 2026-11-16

Branch: `feat-commands`, based on release commit `aa82bb4`.

Implementation commits:

- `2d4bf28` — pure keybinding, context predicate, scoped context stack, and deterministic registry core.
- `e31de06` — native shortcut/chord routing, window-local services, guarded handlers, virtualized menu, Spotlight adapter, interaction tests, runnable example, and exhaustive native-predicate parity corrections.
- A following documentation/snapshot commit contains this report, README, integration fixture, and four reviewed light/dark goldens.

Only `crates/commands/**` and the root `Cargo.toml` workspace dependency were changed. No merge or push was performed. Cargo generated an entry for the new package during testing; `Cargo.lock` was restored to respect the allowed-file boundary. Integration must regenerate that entry before running with `--locked`.

## Passing gates

On both upstream GPUI and `--features gpui-kit/gpui-fast`:

- `cargo test -p guiraffe-commands`: **29 tests passed**, zero failures or ignored tests: 16 pure unit tests, 11 GPUI interaction/parity/render tests, one integration test comparing two scheme snapshots, and one runnable doctest.
- `cargo clippy -p guiraffe-commands --all-targets -- -D warnings`: passed. All-targets compilation includes the runnable example.
- `cargo fmt -p guiraffe-commands -- --check`: passed.
- `git diff --check`: passed.

Commands used direct stable-toolchain binaries (Rust 1.98.0) in PATH because the environment's rustup proxy incorrectly resolves its invoked name. Builds reused the existing `feat-chat/target` directory. These are native GPUI test/headless-render checks; an interactive desktop launch was not run.

## Coverage

Pure tests cover invalid IDs and bindings, all 120 modifier permutations, key aliases and function-key normalization, chord prefixes, bounded/invalid predicates, Unicode contexts, context guard restoration during unwinding, ordering, duplicate/failed-write atomicity, change detection, and deterministic conflict/fallback behavior.

GPUI tests cover single shortcuts and delayed chord completion, actual focused key contexts, disabled/inactive guards, menu clicks, rebind/removal routes, conflict fallback, reentrant mutation from handlers, separate window services, idempotent initialization, stale Spotlight activation guards, and zero notifications/invalidation on no-op writes. Predicate parity includes all 512 combinations of three flags over three context frames across twelve expressions, plus named value/negation examples. Both original and canonical predicates are evaluated by GPUI for comparison. The render test updates an unrelated leaf and checks consumer counts, rather than hiding whole-window invalidation with forced refreshes.

The independent snapshot fixture uses bundled assets and four forced redraw frames before capture. All four backend/scheme goldens were visually reviewed for text, category ordering, hints, disabled appearance, spacing, and clipping. Forced redraws are confined to snapshot preparation.

## Integration limits

- Existing Spotlight descriptor types remain unchanged. Open palettes display a descriptor snapshot; callers must observe the registry and call `Spotlight::set_commands` to refresh it. Execution always checks current registry membership, enabled state, explicit host contexts, and the captured originating focus contexts.
- Native keymap routes are global, owned, and deduplicated. Unregister/rebind leaves an inert old route; resolution and propagation use each service's current catalog. The service never clears unrelated application bindings.
- Menus use explicit host contexts to preserve logical state when focus enters the menu. Shortcut routing additionally incorporates the actual focused window's GPUI key contexts. Applications must update explicit contexts as their active document/editor changes.
- The menu is an in-app adapter. Native menu bars, toolbar/context-menu composition, gallery registration, IR consumers, and a full keymap editor remain owning-crate follow-ups; the catalog and editing APIs needed by those consumers are exposed.
