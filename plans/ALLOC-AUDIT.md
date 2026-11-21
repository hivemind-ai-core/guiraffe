# Warm-repaint allocation audit (perf-alloc-audit)

Scope: the crates nobody else owns — `ui` (DataUi + `ui!` views), `windows`,
`motion`, `commands`, `devtools`, `state`, `router`, `icons`. Components,
inputs, layout and overlays belong to Kilo (`bench/src/warm_repaint.rs`); charts,
timeline, flow, flamegraph, tree, datagrid, code, markdown, chat and mermaid
belong to the Codex work packages.

## Harness

`guiraffe_test::alloc` (`crates/test/src/alloc.rs`) is a thread-local counting
allocator: install it as `#[global_allocator]` in a test binary, warm the view,
then count one more repaint with `count_allocations`. It mirrors
`bench/src/allocator.rs` (which stays the instruction-count gate owner) so crate
tests can assert without depending on the bench crate.

Each crate's gate lives in its own `tests/` directory and asserts, in this order:

1. a warm repaint stays inside a **recorded budget** (measured below), and
2. the repaint **does not grow** frame over frame (median of frames 4–6 ≤ median of frames 1–3,
   so a single gpui-fast outlier or a one-off retained-render buffer does not fail the gate).

A literal "zero allocations" target is not reachable for a GPUI repaint: the
framework builds its scene every frame. Measured floor on the upstream backend:
a plain 4-node `div` tree, no GUIraffe code, costs **63 allocations / 52 360 B**
per forced draw. Of those, a bare context update is 1 and `run_until_parked`
alone is 0, so ~62 are GPUI frame scaffolding charged to every window.

## Measured budgets (upstream backend, debug profile, stable series)

| crate | view under test | allocations per warm repaint | budget |
|---|---|---|---|
| router | nav view over one `Router::select` | 7 (fast: 20–21) | 60 |
| state | memoised `Store` selection view | 13 | 60 |
| motion | settled `Animated` + `Presence` | 23 | 80 |
| devtools | open, paused panel, 1 tracked view | 757–758 (fast: 488–508; closed host: 31 / 101) | 900 |
| ui (`ui!`) | macro-generated 4-child view | 60 (first frame 81) | 120 |
| ui (DataUi) | 5-node bound document + shell | 189 | 200 |
| commands | 18-row menu in 3 categories | 858 | 1 000 |
| windows | no views (registry, bus, layout store) | — | n/a |
| icons | asset source only | — | n/a |

`windows` and `icons` expose no rendered view: `windows` is a keyed window
registry + typed event bus + layout persistence, and `icons` is an
`AssetSource`. Neither runs per-frame code, so neither gets a repaint gate; the
audit records that explicitly rather than inventing a synthetic one.

## Attribution within DataUi

Bisected by document shape (same shell, same warm draws):

| document | allocations | delta |
|---|---|---|
| empty stack in the shell | 14 | host floor |
| + 1 `text` | 39 | 25 |
| + 3 `text` | 75 | 18 per extra text |
| + 6 `text` | 128 | 17.7 per text |
| + 1 nested `stack` | 23 | 9 |
| + 1 `button` | 91 | 77 |

So the host charges ~9 allocations per node (an `AnyElement` per child plus the
children `Vec`), the `text` widget ~18 on top (it is `guiraffe-components`, Kilo's
crate), and a `button` ~77 (a `Button` entity plus its handlers).

## Fixes landed

- `crates/commands/src/service.rs`, `menu.rs`: `MenuRow::Item` now carries a
  pre-built `element_id`. The menu was calling `SharedString::from(id.to_string())`
  for every visible row on every frame; the row id string is now formatted once,
  when the row projection is rebuilt after a catalog/context change.

## Recorded, not fixed (need an owner decision)

- `Menu::render` still rebuilds one `Button` element per visible row per frame,
  with per-row `ButtonCustomVariant`, theme lookups and a click closure that
  captures a cloned `Commands` and `CommandId`. That is the bulk of the 858.
  A virtualised row pool (entity per row, like `DataUi`'s `BoundNode`) would cut
  it, but it is a structural change to the menu's ownership model, not a local
  fix, and it overlaps the Spotlight redesign in `plans/ROADMAP-0.2.md`.
- Devtools rebuilds its telemetry lines with `format!` every sample; the panel
  is opt-in and paused-aware, and the measured cost (757) is dominated by the
  row/panel element tree rather than the formatting. Budgeted, not optimised.
- DataUi's per-node `AnyElement` + children `Vec` erasure is inherent to the
  `Widget::render(children: Vec<AnyElement>)` signature. Changing it to a sink
  or a smallvec would remove ~9 allocations per node for every document; it is
  a public trait change, so it needs the foreman's call.

## Verification

- `cargo test -p guiraffe-ui -p guiraffe-motion -p guiraffe-state -p guiraffe-router -p guiraffe-commands -p guiraffe-devtools --tests --features guiraffe-devtools/devtools`
  (upstream): all pass.
- Same set with `--features gpui-kit/gpui-fast`: see the gate log.
- Full workspace gate (`scripts/gate.sh`): tests on both backends, per-crate
  doctests, `clippy -D warnings`, `fmt --check`.

## Gate shape note

All six gates use the same shape: run six counted repaints after warm-up, assert
the **peak** is inside a recorded budget and that the **median** of frames 4–6
does not exceed the median of frames 1–3. Medians, not equality, because
gpui-fast legitimately varies by a frame (router: 21 then 20 — a retained-render
buffer appearing once). Devtools peaks at 758 upstream and 490 on gpui-fast, so
its budget covers both backends.

Tests assert **behaviour** (bounded, non-growing repaint cost), not incidental
numbers: the budgets are the recorded measurements above, and each one carries a
pointer to this file.
