# Import and line-index fuzz targets

Run from the checkout root with nightly Rust and cargo-fuzz installed.

```sh
mkdir -p "$PWD/target/tmp"
export TMPDIR="$PWD/target/tmp"
export CARGO_TARGET_DIR="$PWD/target/fuzz-build"
cargo +nightly fuzz run clipboard_tsv -- -max_len=16384 -runs=10000
cargo +nightly fuzz run code_line_index -- -max_len=16384 -runs=10000
```

The clipboard target checks dimensions, bounds and round-trip preservation.
It limits the round trip to 16,384 cells so a ragged input can exercise the
importer's sparse representation without forcing the harness to expand it.
The line-index target compares every line and each valid column with offsets
from a split-line reference. Both targets pass malformed UTF-8 through lossy
decoding to exercise the `&str` API used by the application.

The targets compile the production model modules directly so the fuzzer can
instrument them without building a GUI backend.
