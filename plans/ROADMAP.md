# Roadmap / workflow
Branches: `main` (foreman only) ← `release/0.1` (integration) ← `feat/*` worktrees under `worktrees/`.
Each agent: work in its own worktree branch off `release/0.1`, `cargo test -p <crate>` + `cargo clippy -D warnings` green, commit, report. Foreman reviews, merges into release, runs whole-workspace tests, merges to main.

## Wave 1 (parallel)
| work | owner |
|---|---|
| core theme tokens + Controls bridge (blocks visual polish of everything) | foreman (claude) |
| flow-core + flow (port graph-core/graph-ui) | codex |
| mermaid + markdown (port mer) | omp |
| layout crate | kilo |
## Wave 2
windows (foreman), overlays (codex), components (kilo/omp), gallery, agent chat.
## Wave 3: review/refactor pass, benchmarks, docs.
