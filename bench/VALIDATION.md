# Benchmark infrastructure handoff

Implementation: `c11b27a` (harness, CLI, suites and tests) and `cb1ade8` (CI and Justfile).
No merges were performed. No other crate's source or manifest was changed.

## Local validation

On stable rustc 1.98.0, x86_64-unknown-linux-gnu:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy -p guiraffe-bench --all-targets --features instruction-counts --locked -- -D warnings`: passed, including generated iai-callgrind targets. Cargo separately reports a future-incompatibility notice in third-party `proc-macro-error2 2.0.1`.
- `cargo test -p guiraffe-bench --locked`: **21 passed**, comprising 13 library tests (11 statistics, 2 harness validation), 5 allocator integration tests, 2 CLI integration tests and 1 doctest; zero failed or ignored.
- `cargo build --release --locked -p guiraffe-bench`: passed.
- `cargo check -p guiraffe-bench --all-targets --features gpui-fast --locked`: passed (typecheck, not a full local backend test run).
- `run --instructions`: exited successfully with the explicit message `SKIP instruction counts: Valgrind is not installed or cannot run`; actual instruction measurements were not run on this machine.
- CI YAML and TOML parsed locally; `just --list` recognized all four requested recipes.

The counting allocator tests install the real global allocator and check allocations,
zeroed allocations, reallocation, thread isolation, nested-scope rejection and panic
recovery. Both initial workloads assert zero allocations. No GPUI interactions are
introduced by this crate.

## Recorded baseline

`bench/baselines/dev-laptop/*.json` was measured from clean commit
`cb1ade8c5ea268f43d8aea7b83f42fba54d23f64`, release build, 30 samples,
10,000 invocations per sample, 10,000 warmup invocations and 10,000 seeded
bootstrap resamples. The CPU was AMD Ryzen 7 8745HS with governor `powersave`.

**The host was heavily loaded:** load averages were **24.48 / 35.45 / 28.76**,
with concurrent Orca agents, Rust builds and browser processes. The JSON records
and generated HTML explicitly retain this context. These are exploratory baselines,
not quiet-runner performance evidence or a claim about GPUI frame budgets.

| Workload | Median ns/invocation | Unscaled MAD ns | Bootstrap 95% CI ns |
|---|---:|---:|---:|
| core-variants | 289.113 | 6.956 | [285.213, 296.079] |
| windows-placement | 29.555 | 1.266 | [28.879, 31.039] |

One core invocation resolves seven variants in each of two schemes. One placement
invocation restores against two displays and computes a cascade. A nanosecond value
here is not a per-frame measurement. `bench/reports/trends.html` was generated and
parsed locally; regenerate it using the CLI after checkout (reports are gitignored).

## Two-revision comparison smoke

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

## Remaining acceptance boundaries

- `cargo-deny` is not installed locally, so its CI action/policy has not been executed here; no advisories have been silently exempted.
- GitHub Actions jobs have been authored and syntax checked, not run remotely. The nightly job requires provisioning a dedicated quiet runner labeled `self-hosted`, `linux`, `guiraffe-bench`; history is retained through the workflow cache.
- Valgrind measurements require Valgrind and exactly `iai-callgrind-runner 0.16.1`. PR CI creates a named base-revision baseline and enforces a 2% instruction regression limit; when the base predates this harness there is no old instruction baseline.
- `compare` compares wall-clock records. Instruction comparisons use iai-callgrind's documented named-baseline commands in README and PR CI rather than the wall-clock comparison JSON format.
- Core has no placement API on the dispatch base; placement is benchmarked through guiraffe-windows. Flow-core and layout are stubs on this base. Their future workloads can be registered once in `src/suites.rs`, which also generates instruction wrappers.
- GPUI layout/prepaint/paint, renderer scenarios, startup/RSS/binary-size, Electron comparison and broader coverage/audit/fuzz/mutation/flake jobs remain later infrastructure work. This handoff makes no claim that those roadmap items are implemented.
