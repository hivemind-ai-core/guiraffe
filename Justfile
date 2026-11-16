# Development gates, matching CI.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --locked -- -D warnings

test:
    cargo test --workspace --locked

bench:
    cargo run --release --locked -p guiraffe-bench -- run

bench-compare A B:
    cargo run --release --locked -p guiraffe-bench -- compare {{quote(A)}} {{quote(B)}}
