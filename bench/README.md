# GUIraffe benchmarks

Run from the repository root with the pinned workspace toolchain/dependencies:

```sh
just bench
cargo run --release --locked -p guiraffe-bench -- baseline --profile dev-laptop --notes 'Host heavily loaded'
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
per sample, one equally long warmup, and 10,000 seeded bootstrap resamples.

`compare OLD_REF NEW_REF` resolves commits before creating detached temporary
worktrees, builds both with `--release --locked`, and copies both executables out
of the shared build cache. Both refs must already contain this harness protocol
and the selected suites. Each A/B pair is sampled in separate subprocesses with
warmup, alternating AB then BA to reduce order bias. `taskset` uses the first CPU
in the calling process's allowed affinity mask; `--cpu N` overrides it. Missing
`taskset` is explicitly reported; an invalid CPU is an error. Temporary worktrees
are removed on normal exit and Rust error unwinding; an uncatchable kill can leave
one registered, in which case inspect `git worktree list` before removing it.

The table shows median, relative change and paired bootstrap CI; full records,
MAD and individual confidence intervals are saved to `bench/reports/compare.json`.
The default wall-clock threshold is +10%; failure requires the entire relative
95% CI to exceed it. `--threshold FRACTION` overrides that budget. A profile is
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
Flow-core and layout are stubs
on the dispatch base and can be added when their APIs land. Core has variant
resolution; window placement lives in guiraffe-windows, with no placement API
in guiraffe-core on this revision.

`CountingAllocator` is an opt-in `#[global_allocator]` wrapper around `System`;
`count_allocations` counts successful alloc/realloc/alloc_zeroed operations and
cumulative requested bytes for the calling thread. It ignores other threads,
rejects nesting and resets after panic unwinding. Install it in a test executable
before asserting allocations per frame. Returning an allocated object keeps it
alive beyond the counting scope. The utility cannot detect whether the caller
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
