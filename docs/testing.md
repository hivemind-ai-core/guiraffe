# Testing

## The gate

```sh
scripts/gate.sh            # tests on both backends, doctests, clippy, formatting
scripts/gate.sh --regen    # regenerate screenshot goldens first, then look at each changed image
```

It needs about 100 GB of disk for both backends and takes 10 to 25 minutes on a 16-thread machine.

| Kind | What it protects |
|---|---|
| Model and property tests | Edits, undo, sorting, layout and indexing against simple reference models |
| Interaction tests | Pointer, keyboard, focus and accessibility roles in a headless window |
| Screenshot goldens | Appearance in light and dark on both backends |
| Render counters | That a change re-renders only what it should |
| Allocation gates | Warm paths, judged by allocation counts that machine load can't distort |
| Scaling tests | That per-frame work doesn't follow dataset size |
| Catalogue drift | That builder options are available to DataUi or documented as Rust-only |
| Fuzz targets | Clipboard and line-index models (see [fuzz](../fuzz/README.md)) |

## Leaks

```sh
CARGO_TARGET_DIR=target/gate-up cargo build --release -p guiraffe-bench --example soak
scripts/leak-check.py --soak target/gate-up/release/examples/soak --out target/leak
```

This needs Valgrind and takes about five minutes. It runs every soak scenario short and four times
longer and fails if lost memory grows. It also fails on invalid memory accesses, or if the mixed
scenario's live heap grows more than 2 %.

## What isn't tested

macOS, Windows, native screen readers, and performance on low-end hardware.
