# Benchmark infrastructure handoff

Initial implementation: `c11b27a` (harness, CLI, suites and tests) and `cb1ade8`
(CI and Justfile). Review fixes: `2029a06` (items 9/11), `e9371e8` (item 10),
`e36da76` (items 1, 3–5, 8 and CLI improvement output), and
`ee71300` (item 1: cancellation while descendant output pipes drain).
No merges were performed. No other crate's source or manifest was changed.

## Local validation

On stable rustc 1.98.0, x86_64-unknown-linux-gnu:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy -p guiraffe-bench --all-targets --features instruction-counts --locked -- -D warnings`: passed, including generated iai-callgrind targets. Cargo separately reports a future-incompatibility notice in third-party `proc-macro-error2 2.0.1`.
- `cargo test -p guiraffe-bench --locked`: **29 passed**, comprising 15 library tests (12 statistics, 3 harness validation), 6 allocator integration tests, 7 CLI/comparison integration tests and 1 doctest; zero failed or ignored.
- `cargo build --release --locked -p guiraffe-bench`: passed.
- `cargo check -p guiraffe-bench --all-targets --features gpui-fast --locked`: passed (typecheck, not a full local backend test run).
- `run --instructions`: exited successfully with the explicit message `SKIP instruction counts: Valgrind is not installed or cannot run`; actual instruction measurements were not run on this machine.
- CI YAML and TOML parsed locally; `just --list` recognized all four requested recipes.

The counting allocator tests install the real global allocator and check allocations,
zeroed allocations, reallocation, thread isolation, nested-scope rejection and panic
recovery. Both initial workloads assert zero allocations. No GPUI interactions are
introduced by this crate.

## Historical loaded-host experiment (not a reference)

The original `bench/baselines/dev-laptop/*.json` records were removed for review
item 12. Historical exploratory notes remain in `bench/baselines/README.md`; no
loaded-host reference JSON is tracked. Fresh reference records require a quiet,
controlled machine. The original experiment was measured from clean commit
`cb1ade8c5ea268f43d8aea7b83f42fba54d23f64`, release build, 30 samples,
10,000 invocations per sample, 10,000 warmup invocations and 10,000 seeded
bootstrap resamples. The CPU was AMD Ryzen 7 8745HS with governor `powersave`.

**The host was heavily loaded:** load averages were **24.48 / 35.45 / 28.76**,
with concurrent Orca agents, Rust builds and browser processes. The original JSON records
and generated HTML explicitly retained this context. These are exploratory baselines,
not quiet-runner performance evidence or a claim about GPUI frame budgets.

| Workload | Median ns/invocation | Unscaled MAD ns | Bootstrap 95% CI ns |
|---|---:|---:|---:|
| core-variants | 289.113 | 6.956 | [285.213, 296.079] |
| windows-placement | 29.555 | 1.266 | [28.879, 31.039] |

One core invocation resolves seven variants in each of two schemes. One placement
invocation restores against two displays and computes a cascade. A nanosecond value
here is not a per-frame measurement. `bench/reports/trends.html` was generated and
parsed locally. Generate a new report after recording fresh quiet-host data
(reports are gitignored). These historical samples predate adaptive minimum-duration
measurement and are not comparable to newly recorded values without a fresh run.

## Historical two-revision comparison smoke (before review fixes)

```sh
target/release/guiraffe-bench compare c11b27a cb1ade8 \
  --samples 10 --iterations 1000 --threshold 1 \
  --output bench/reports/smoke.json
```

Both commits contain identical workload source; this validates the workflow rather
than an optimization. Both detached revisions were built with release/locked flags,
samples were pinned with taskset to allowed CPU 0, and AB/BA rounds were interleaved.
The smoke deliberately used a 100% threshold; the default production threshold is
10%, and unit tests independently cover significant, uncertain and boundary results.

| Workload | Old median ns | New median ns | Relative 95% CI | Result |
|---|---:|---:|---:|---|
| core-variants | 381.98 | 386.41 | [-1.98%, 3.09%] | pass |
| windows-placement | 43.30 | 43.19 | [-1.72%, 1.94%] | pass |

Saved JSON was checked for both exact commit identities, ten paired samples per
suite and CPU pinning. Git worktree registration was checked after completion:
comparison's temporary checkouts were removed. The difference from unpinned baseline
numbers also illustrates why these loaded-host runs are not performance claims.

## Review fixes and new evidence

- **1:** Ctrl-C and SIGTERM set a flag, checked between measured batches and child
  commands. Active subprocesses own a process group; interruption kills that group
  before reaping its leader. RAII covers checkout creation, build failure and normal
  return. Startup recovers only marked `guiraffe-bench-` directories with a free
  owner lock, serializes registration with a registry lock, and runs Git's worktree
  prune. Live owners are preserved. Tests interrupt an actual sleeping build child,
  assert its exit/reaping and checkout removal (including a leader that exits while
  a descendant holds stdout), recover a stale registered checkout,
  and prove a locked owner's checkout survives concurrent startup.
- **3:** CLI exit statuses are 0 for pass/improvement, 1 for an established wall-clock
  regression and 2 for tool/configuration/interruption errors. Tests run actual CLI
  comparisons with deterministic fake Cargo artifacts and sample binaries (100→100,
  100→150, 100→50) and assert status, confidence classification and saved JSON.
- **4:** Comparison builds use `target/bench-compare`. Cargo JSON compiler-artifact
  messages supply executable paths; the fixture deliberately emits an executable in
  an unconventional custom-target directory, proving no release-path assumption.
- **5:** Dirty state is collected from the invoking repository root, retained in
  comparison metadata, and warns when a ref resolves to its HEAD. A test dirties
  source timing data and verifies that committed data is measured and a warning is
  printed. Worktrees remain clean committed snapshots; no WORKTREE pseudo-ref added.
- **8:** Both refs are probed for benchmark manifests and CLI source before creating
  checkouts or building. Missing support yields a clear ref-specific tool error;
  the integration test confirms no build or temporary registration is started.
- **9:** Timings use `as_secs_f64() * 1e9`, repeated batches until at least 1 ms of
  measured workload duration, and the actual invocation count as denominator.
  Tests exercise single-iteration tiny workloads, positivity, minimum duration and
  cancellation. `Record.iterations` is now the minimum batch size; older harness
  refs retain their older sampler, so re-record both sides with the current protocol
  before making performance claims about tiny workloads.
- **10:** Realloc byte accounting explicitly means cumulative full requested sizes,
  not growth or retained memory. Process-wide counting is opt-in, synchronized,
  includes worker threads, rejects overlapping scopes and resets during unwinding.
  An isolated single-test executable proves worker allocation observation, nested
  rejection, recovery and the existing exact per-thread byte semantics.
- **11:** Confidence intervals wholly below the negative threshold report
  `improvement` in the table and saved JSON. Unit tests cover boundary/overlap cases;
  an end-to-end CLI fixture verifies the improvement output and successful exit.
- **12:** Loaded-host JSON references removed; exploratory numbers remain as notes.

The new comparison integration tests use real Git repositories/worktrees and the
real CLI orchestration/process paths, but fake Cargo/artifact executables to make
regression and improvement decisions deterministic. They do not claim a fresh
performance comparison or new Valgrind measurements. The historical smoke above
used real Cargo before the review, and remains historical evidence only.

## Explicit deferred review follow-ups

- **2 — allocation/render-count gates:** add real per-frame allocation and scripted
  interaction/render-count assertions to every-PR CI when guiraffe-test and the GUI
  scenarios land. Current pure-suite zero-allocation tests and allocator utilities
  do not establish those GUI gates.
- **6 — prove the instruction gate fails red:** on a host with Valgrind and the exact
  runner, deliberately increase a fixture's instruction count above 2%, run the
  named baseline comparison in CI and retain evidence that the gate exits nonzero.
  Local compilation and the missing-Valgrind skip are not that proof.
- **7 — durable nightly history:** persist timestamped nightly JSON beyond the
  Actions cache's eviction/retention limits (for example versioned artifact/object
  storage), restore previous history and verify trend generation across runs.
  The existing Actions cache is a convenience, not durable historical storage.

## Remaining acceptance boundaries

- `cargo-deny` is not installed locally, so its CI action/policy has not been executed here; no advisories have been silently exempted.
- GitHub Actions jobs have been authored and syntax checked, not run remotely. The nightly job requires provisioning a dedicated quiet runner labeled `self-hosted`, `linux`, `guiraffe-bench`; history is retained through the workflow cache.
- Valgrind measurements require Valgrind and exactly `iai-callgrind-runner 0.16.1`. PR CI creates a named base-revision baseline and enforces a 2% instruction regression limit; when the base predates this harness there is no old instruction baseline.
- `compare` compares wall-clock records. Instruction comparisons use iai-callgrind's documented named-baseline commands in README and PR CI rather than the wall-clock comparison JSON format.
- Core has no placement API on the dispatch base; placement is benchmarked through guiraffe-windows. Flow-core and layout are stubs on this base. Their future workloads can be registered once in `src/suites.rs`, which also generates instruction wrappers.
- GPUI layout/prepaint/paint, renderer scenarios, startup/RSS/binary-size, Electron comparison and broader coverage/audit/fuzz/mutation/flake jobs remain later infrastructure work. This handoff makes no claim that those roadmap items are implemented.
