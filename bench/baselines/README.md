# Reference baselines

Record and commit reference baselines only on a quiet, controlled machine under a
named profile. Keep exploratory loaded-host data outside this directory.

The initial `dev-laptop` experiment at clean commit
`cb1ade8c5ea268f43d8aea7b83f42fba54d23f64` measured 30 samples with 10,000
invocations per sample on an AMD Ryzen 7 8745HS (`powersave` governor), stable
rustc 1.98.0. The host was heavily loaded: 24.48 / 35.45 / 28.76 load averages.
Observed medians were 289.113 ns for core-variants and 29.555 ns for
windows-placement. These are historical exploratory notes, **not reference
baselines**; their JSON records were removed in review item 12. They also predate
the minimum-duration sampler introduced by review item 9.

Create fresh quiet-host records with:

```sh
cargo run --release --locked -p guiraffe-bench -- baseline --profile quiet-machine
cargo run --release --locked -p guiraffe-bench -- report
```
