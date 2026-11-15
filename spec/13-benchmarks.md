# Benchmarks and baselines

Layout:
- `bench/` holds the harness crate `guiraffe-bench` and its suites.
- `bench/baselines/<profile>/<suite>.json` holds recorded baselines.
- `bench/reports/` holds the generated HTML reports (gitignored).

| Suite | Tool | Gate |
|---|---|---|
| micro (pure: flow-core queries/edits, placement, fuzzy, easing, markdown parse, router match) | criterion | nightly |
| instructions (same benches) | iai-callgrind | **every PR, ±2 %** |
| render counts (re-renders per scripted interaction for each Control and example app) | guiraffe-test RenderCounter | **every PR, exact or lower** |
| headless frame (layout/prepaint/paint ms; elements; allocations per frame) | GPUI test/bench context + counting allocator | every PR (allocations exact), wall-clock nightly |
| scenario (real renderer via e2e probe: pan 5k graph, scroll 100k list, stream 1 MB markdown, open/close 50 modals, 10 windows) | guiraffe-test probe | nightly, ±10 % |
| startup, memory, binary size | hyperfine, /proc RSS, dhat, cargo-bloat | nightly |
| Electron comparison | same reference app | per release |

Method:
- Run A/B interleaved, with CPU pinning (taskset) and a warm-up, at least 10 samples.
- Report median, MAD and a bootstrap 95 % CI. A regression counts only when the CI excludes the threshold.
- Every baseline records commit, rustc, CPU model, governor and load average.
  `compare` refuses to compare across profiles unless forced.
- Commands: `guiraffe bench run|baseline|compare <old> <new>|report`.
- Rule: optimisation PRs must include `compare` output. Every new hot path lands with a benchmark first.
