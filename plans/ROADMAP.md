# Roadmap

Workflow:
- Each piece of work happens on a `feat/*` worktree, merges into `release/0.1` (integration), and from there into `main`.
- Only the foreman merges.
- A merge needs the gates passing (fmt, clippy -D warnings, tests) **plus an independent review**. Findings go in `reviews/<crate>-review-N.md`.
- Batch-test `release/*`, merge it to `main`, then cut the next release branch.

Status legend: ✅ merged · 🔧 in progress · 🔍 in review · ⏳ queued

## Phase 1: foundations (current)
| Item | Owner | Status |
|---|---|---|
| core: tokens, palettes, variants, Controls bridge | foreman | ✅ |
| windows: registry, guards, bus, persistence | foreman | ✅ |
| gpui-kit 0.9.1 upgrade | foreman | ✅ |
| DESIGN.md, API conventions, specs | foreman | ✅ |
| flow-core + flow | codex | 🔧 review #1 fixes |
| layout | kilo | 🔍 |
| mermaid + markdown | omp | 🔧 |
| overlays | codex | 🔧 |

## Phase 2: quality and performance infrastructure (before more widgets)
These land early so everything after them is measured and tested from day one.
1. `guiraffe-test`: helpers, test ids, RenderCounter, allocation counter. *(foreman)*
2. Spike: visual snapshots on Linux (Xvfb/weston + lavapipe, capture, determinism). *(agent)*
3. `guiraffe-bench` + baselines + `compare` (criterion, iai-callgrind, frame benches). Record baselines for flow/layout/windows. *(agent)*
4. CI: gates, both GPUI backends (upstream + gpui-fast), coverage, deny/audit, nightly suites. *(agent)*
5. `guiraffe-motion`: easing, springs, Transition, Presence, AnimatedValue. *(foreman)*
6. `guiraffe-macros`: `#[derive(Props)]` (builder, schema, IR registration). *(foreman)*

## Phase 3: app framework (React parity)
`guiraffe-state` (selectors) → `guiraffe-router` (typed, nested, per-window) → `guiraffe-commands`
→ `guiraffe-form` → `guiraffe-query` → `guiraffe-machine`. Flow and overlays then migrate onto the machine and commands.

## Phase 4: widgets
`guiraffe-components` (spec/03, split among agents by family: inputs / display / feedback),
Agent Chat UI (Mocket-inspired: virtualized streaming list, tool-call cards, composer),
windows tear-off and parent/modal windows, overlays per-window integration.

## Phase 5: data-driven UI and Mocket
`guiraffe-ui-ir` + JSON Schema → `guiraffe-ui` runtime with hot reload → `guiraffe-codegen`
→ `guiraffe-mocket` plugin → `spec/11-mocket-target.md` handed to Mocket for a native target.

## Phase 6: tooling and polish
`guiraffe` CLI (new/preview/gen/snapshot/bench/bundle), `guiraffe-devtools` (inspector, render highlighting,
state time-travel, machine viewer, theme editor), gallery, docs cookbook, Electron comparison app, 0.1 release.

Running throughout: a review/refactor pass after every phase, nightly flake/fuzz/mutation runs,
and benchmark trend reviews.
