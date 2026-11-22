# Contributing

## Before you start

- Read the [architecture](docs/architecture.md) and the
  [API conventions](docs/contributing/api-conventions.md).
- For anything visual, read the [UI and UX guidance](docs/contributing/ui-ux/README.md).
- Check the [roadmap](ROADMAP.md) before proposing a new component.

## A change is ready when

- `scripts/gate.sh` passes. It tests both backends.
- Behavior has a test. Performance work has a test that machine load can't distort, such as an
  allocation, instruction or scaling count.
- Every changed screenshot has been looked at.
- Library code has no `unwrap`, `expect` or panics on bad input.
- A change to vendored code is listed in [vendor/README.md](vendor/README.md).
- A change to the DataUi catalogue records its fingerprint in the changelog. The CLI test prints
  it. Bump the catalogue major version for breaking changes.

## Reporting a performance problem

Include the backend, release or debug, dataset size, machine, load, and the command that
reproduces it.
