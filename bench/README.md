# GUIraffe benchmarks

Run from the repository root with the pinned workspace toolchain/dependencies:

```sh
just bench
cargo run --release --locked -p guiraffe-bench -- baseline --profile quiet-machine --notes 'Quiet controlled host'
just bench-compare OLD_REF NEW_REF
cargo run --release --locked -p guiraffe-bench -- report
cargo bench --locked -p guiraffe-bench --bench micro
cargo run --release --locked -p guiraffe-bench -- run --instructions
```

`run` prints a JSON array and optionally writes it with `--output PATH`. `baseline`
writes one versioned record to `bench/baselines/PROFILE/SUITE.json`. Release builds
are required for performance claims. The recorded `build_profile` distinguishes
debug measurements. Each workload invocation comprises the operations documented
in `src/suites.rs`; measurements are **per invocation**, not per individual variant.
The clock covers only workload execution, excluding host metadata, serialization,
process startup and bootstrap analysis. Default: 20 samples, 10,000 iterations
as the minimum batch size per sample, one batch of warmup, and 10,000 seeded
bootstrap resamples. Samples repeat batches until measured workload duration reaches
at least 1 ms. Timings use f64 seconds converted to nanoseconds and divide by the
actual invocation count, avoiding integer nanosecond truncation for tiny workloads.

`compare OLD_REF NEW_REF` resolves commits before creating detached temporary
worktrees, builds both with `--release --locked`, and copies both executables out
of the dedicated `target/bench-compare` build cache. Cargo JSON messages identify the
executable, including nondefault target triples and artifact paths. After copying
the old binary and recording its metadata, the harness removes its source checkout
before creating/building the new revision, invalidating old local-source paths in
shared Cargo fingerprints. Both refs must
already contain this harness protocol
and the selected suites. Each A/B pair is sampled in separate subprocesses with
warmup, alternating AB then BA to reduce order bias. `taskset` uses the first CPU
in the calling process's allowed affinity mask; `--cpu N` overrides it. Missing
`taskset` is explicitly reported; an invalid CPU is an error. Temporary worktrees
use the recognizable `guiraffe-bench-` prefix inside
`target/bench-compare/worktrees`. Ctrl-C/SIGTERM sets an interruption flag; the CLI
kills and reaps an active child's process group before unwinding checkout guards.
Cancellation is checked between measured batches, subprocesses and A/B rounds.
Startup takes a registry lock, reaps stale marked benchmark directories only when
their owner lock is free, and runs `git worktree prune`; live comparisons are never
reaped. An uncatchable kill is recovered at the next comparison startup.
Refs are probed for the benchmark CLI before any worktrees/builds. A dirty invoking
source tree produces a warning when either ref resolves to HEAD: only committed
snapshots are compared. The comparison records' dirty flag describes that source
root, not the clean detached checkout.

The table shows median, relative change and paired bootstrap CI; full records,
MAD and individual confidence intervals are saved to `bench/reports/compare.json`.
The default wall-clock threshold is +10%; failure requires the entire relative
95% CI to exceed it. An interval wholly below -10% reports `improvement`; overlap
or a boundary equality remains `pass` (uncertain). Exit codes: 0 pass/improvement,
1 statistically established wall-clock regression, 2 tool/configuration/interruption
error. Instruction-mode Cargo failures are tool errors, not classified wall-clock
regressions. `--threshold FRACTION` overrides that budget. A profile is
a human-maintained machine/environment identity, not a CPU autodetection guess.
`--new-profile NAME` explicitly identifies a different environment; comparison
refuses different profile/build names without `--force`. A forced comparison
still requires identical suites, metrics and iteration counts. A loaded host can
produce wide or biased timing distributions: these results are exploratory,
not evidence of a speedup or compliance with frame budgets.

Reports accept a record file or recursively read a directory of record files
(single records and arrays). Retain timestamped `run --output` files to produce
trends: baseline intentionally replaces the latest measurement. Plots and tables
are grouped by machine profile, suite and build profile. HTML escapes metadata,
records load and notes, and contains no remote assets. Nightly CI retains history
through a cache and uploads HTML/JSON/criterion artifacts; its dedicated
`self-hosted, linux, guiraffe-bench` runner must have the Linux GPUI dependencies
listed in `ci.yml` and be kept quiet by the operator. A runner with those labels
must be provisioned before the scheduled job can run.

## Instruction counts

Install Valgrind and the exact matching runner:

```sh
cargo install iai-callgrind-runner --version 0.16.1 --locked
cargo bench --locked -p guiraffe-bench --features instruction-counts --bench instructions -- --save-baseline=reference --save-summary=json
cargo bench --locked -p guiraffe-bench --features instruction-counts --bench instructions -- --baseline=reference --save-summary=json
```

The library config fails for instruction (`Ir`) regressions above 2%; summaries
live in `target/iai`. The CLI checks `valgrind --version` and prints an explicit
skip when unavailable. A present Valgrind with missing runner or failed benchmark
is an error, never a successful skip. PR CI builds its base revision first with a
named baseline, then gates the new revision; a base predating the harness has no
previous instruction baseline. Deterministic counts do not use a statistical CI.
The wall-clock `compare` command does not compare instruction summaries; use the
named Callgrind baseline commands above for that metric.

## Adding suites and allocation assertions

Add the crate dependency to `bench/Cargo.toml`, implement workloads and register
stable names in `src/suites.rs`. The CLI, criterion and iai-callgrind all consume
that single registration table (the instruction wrappers are macro-generated).
Flow-core now registers four 5k-node/10k-edge workloads: scene build/drop,
twenty spatial queries with cached-route lookup, child move/undo, and child
select/delete/undo. The query suite reuses scratch buffers and has a warmed
zero-allocation assertion; scene construction and edits intentionally allocate.
Fixture initialization is paid by warmup. Use `--iterations 1` for an inexpensive
comparison of these larger workloads; each sample still runs for at least 1 ms.
Core has variant resolution; window placement lives in guiraffe-windows.

`CountingAllocator` is an opt-in `#[global_allocator]` wrapper around `System`;
`count_allocations` counts successful alloc/realloc/alloc_zeroed operations and
cumulative requested bytes for the calling thread. It ignores other threads,
rejects nesting and resets after panic unwinding. Install it in a test executable
before asserting allocations per frame. Returning an allocated object keeps it
alive beyond the counting scope. For cross-thread gates,
`count_process_allocations` counts every thread through
synchronized accounting; run it in an isolated test executable without unrelated
background work, and coordinate worker completion within the callback. Process
scopes reject nesting/overlap and recover on panic; they may coexist with thread
scopes. Realloc bytes mean the entire new requested size, not the growth delta.
The utility cannot detect whether the caller
installed it: omitting the global allocator yields no counts. Its small unsafe
module forwards allocator preconditions to System, checks successful pointers,
and uses allocation-free const TLS instrumentation. Tests cover the actual global
allocator, zeroed allocations, reallocation, isolation, unwinding, and zero
allocations for the initial pure workloads.

This initial infrastructure does not yet measure GPUI layout/prepaint/paint,
real-renderer scenarios, startup/RSS/binary-size, or Electron. Those require the
forthcoming layout, guiraffe-test and e2e APIs. No GUI interactions are introduced
by this headless crate. CI config supplies fmt/clippy/tests, cargo-deny, both
GPUI backend builds/tests, instruction gates, and nightly wall time; coverage,
audit, fuzzing, mutation and flake jobs remain roadmap work.

## Data UI workloads

`data-ui-instantiate`, `data-ui-patch`, `data-ui-splice`, and `data-ui-structural-splice` are registered in the shared CLI/criterion/Callgrind table. They use real `DataUi` entities and a headless native fixture with bundled assets and a 10,000-item keyed list. Setup/initial rendering occurs during warmup; criterion and Callgrind explicitly prepare and settle the fixture outside their measured callbacks, draining setup notifications before one-shot instruction measurements.

Instantiation creates/drops a host from an already compiled four-node template with no-op logic; lazy first rendering and list seeding are excluded. Patch changes a mounted text node's color. Splice updates one keyed item in the middle of the seeded list, alternating prebuilt shared records. Structural splice alternates that row's key to exercise creation/disposal and live order-statistics maintenance. The text template explicitly opts into a bounded single-line row, and native row views are allocated only when rendered. Patch/splice include slot processing, reconciliation, and targeted notifications, excluding the subsequent draw. These are operation costs, not frame times. The fixture initializes native GPUI once per thread, so `gpui-kit/test-support` is enabled for this benchmark harness.

`tests/allocator.rs` warms these workloads then counts requested allocation traffic, with separate bounds for each operation. The original zero-allocation assertions remain unchanged for core variants and window placement. Run one workload with `cargo run --release -p guiraffe-bench -- run --suite data-ui-splice --samples 20 --iterations 1000`; use a quiet machine and a recorded profile for performance comparisons.
