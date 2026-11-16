# Roadmap

Workflow:
- Each piece of work happens on a `feat/*` worktree, merges into `release/0.1` (integration), and from there into `main`.
- Only the foreman merges.
- A merge needs the gates passing (fmt, clippy -D warnings, tests) **plus an independent review**. Findings go in `reviews/<crate>-review-N.md`.
- Batch-test `release/*`, merge it to `main`, then cut the next release branch.

Status legend: ✅ merged · 🔧 in progress · 🔍 in review · ⏳ queued

## Phase 1: foundations
| Item | Owner | Status |
|---|---|---|
| core: tokens, palettes, variants, Controls bridge | foreman | ✅ |
| windows: registry, guards, bus, persistence | foreman | ✅ |
| gpui-kit 0.9.1 upgrade | foreman | ✅ |
| DESIGN.md, API conventions, specs | foreman | ✅ |
| flow-core + flow | codex | ✅ merged with follow-ups (reviews/flow-review-2.md) |
| overlays | codex | ✅ merged; review fixes 🔧 (reviews/overlays-review-1.md) |
| layout | kilo | 🔧 review #2 fixes (blocked merge) |
| mermaid, then markdown | omp | 🔧 slow; scope narrowed |

## Phase 2: quality and performance infrastructure
| Item | Owner | Status |
|---|---|---|
| guiraffe-bench: stats, interleaved compare, baselines, CI, allocator | codex | ✅ |
| visual snapshot spike: offscreen wgpu + Xvfb e2e probe | codex | ✅ (spikes/visual-snapshots.md) |
| guiraffe-motion: easing, springs, presence, Animated | foreman | ✅ |
| backend decision: gpui-fast default for apps (measured) | foreman | ✅ |
| guiraffe-test helpers: TestId, RenderCounter, snapshot API on the offscreen path | foreman | ⏳ |
| guiraffe-macros `#[derive(Props)]` | foreman | ⏳ |

## Phase 3: app framework (React parity)
✅ `guiraffe-state` (re-frame-inspired; reviewed and fixed) → 🔍 `guiraffe-router` (typed, guards; built on state) → `guiraffe-commands`
→ `guiraffe-form` → `guiraffe-query` → `guiraffe-machine`. Flow and overlays then migrate onto the machine and commands.

## Phase 4: widgets
`guiraffe-components` (spec/03, split among agents by family: inputs / display / feedback),
Agent Chat UI (Mocket-inspired: virtualized streaming list, tool-call cards, composer),
windows tear-off and parent/modal windows, overlays per-window integration.

Required display component: ⏳ syntax-highlighting code viewer (`CodeBlock`), read-only for now;
selection/copy, theme-aware highlighting, plain-text fallback and large-file responsiveness (spec/03).
Both code and Mermaid viewers must work standalone and embedded in Markdown using the same
viewer implementations (spec/06); Markdown owns fence dispatch and embedding layout.

## Phase 5: data-driven UI and Mocket
Status:
- ✅ `guiraffe-ui`: IR, schema-compiled templates, DataUi host with per-node invalidation, hot reload.
- ✅ `guiraffe-mocket`: importer, CLI and plugin; Mocket's sample screens render.
- ⏳ each/when, event payloads and two-way inputs, codegen, chart widgets.

`guiraffe-ui-ir` + JSON Schema → `guiraffe-ui` runtime with hot reload → `guiraffe-codegen`
→ `guiraffe-mocket` plugin → `spec/11-mocket-target.md` handed to Mocket for a native target.

## Phase 6: tooling and polish
`guiraffe` CLI (new/preview/gen/snapshot/bench/bundle), `guiraffe-devtools` (inspector, render highlighting,
state time-travel, machine viewer, theme editor), gallery, docs cookbook, Electron comparison app, 0.1 release.

Running throughout: a review/refactor pass after every phase, nightly flake/fuzz/mutation runs,
and benchmark trend reviews.
