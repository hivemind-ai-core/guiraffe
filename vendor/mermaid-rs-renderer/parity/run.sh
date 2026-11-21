#!/usr/bin/env bash
# Compare with the unmodified crates.io release using the same fonts and themes.
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
work="$root/target/mermaid-parity"
mkdir -p "$work/tmp" "$work/src"
export GUIRAFFE_ROOT="$root"
export TMPDIR="$work/tmp" CARGO_TARGET_DIR="$work/build"
export CARGO_BUILD_JOBS=4
cp "$root/vendor/mermaid-rs-renderer/parity/main.rs" "$work/src/main.rs"
cat > "$work/Cargo.toml" <<MANIFEST
[package]
name = "mermaid-parity"
edition = "2024"
version = "0.0.0"
[workspace]
[dependencies]
reference = { package = "mermaid-rs-renderer", version = "=0.3.1", default-features = false }
patched = { package = "mermaid-rs-renderer", path = "$root/vendor/mermaid-rs-renderer", default-features = false }
MANIFEST
cd "$root"
rustc --edition 2024 --test -o "$work/grid-tests" - <<RUST
#[path = "$root/vendor/mermaid-rs-renderer/src/layout/spatial.rs"]
mod spatial;
RUST
"$work/grid-tests"
cargo run --release --manifest-path "$work/Cargo.toml" -- "$@"
