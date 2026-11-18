#!/bin/bash
# Full gate (AGENTS.md). Run from any worktree: scripts/gate.sh [--regen]
#   --regen  regenerate snapshot goldens on both backends first (then LOOK at every changed PNG).
# Separate target dirs per backend, inside this worktree: alternating feature sets in one target
# dir leaves stale artifacts that link old crate versions (false E0425/E0460/E0463 failures).
# Doctests run per crate: workspace-wide rustdoc hits E0460 when a crate is built twice with
# different features in one invocation. Logs go to target/gate-logs (not /tmp).
set -u
cd "$(git rev-parse --show-toplevel)" || exit 2
LOG=target/gate-logs; mkdir -p "$LOG"
UP="env CARGO_TARGET_DIR=target/gate-up"; FAST="env CARGO_TARGET_DIR=target/gate-fast"
T="--workspace --no-fail-fast --lib --bins --tests"
CRATES=$(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c "import json,sys;print(' '.join(p['name'] for p in json.load(sys.stdin)['packages'] if any('lib' in t['kind'] or 'proc-macro' in t['kind'] for t in p['targets'])))")
fail=0
step(){ local name=$1; shift; "$@" > "$LOG/$name.log" 2>&1; local rc=$?; echo "$name exit=$rc"
  [ $rc -ne 0 ] && { fail=1; grep -E "FAILED|panicked|^error" -A3 "$LOG/$name.log" | head -10; }; return 0; }
if [ "${1:-}" = "--regen" ]; then
  GUIRAFFE_UPDATE_SNAPSHOTS=1 $UP cargo test -q $T > "$LOG/regen-upstream.log" 2>&1; echo "regen-upstream exit=$?"
  GUIRAFFE_UPDATE_SNAPSHOTS=1 $FAST cargo test -q $T --features gpui-kit/gpui-fast > "$LOG/regen-fast.log" 2>&1; echo "regen-fast exit=$?"
  find crates bench \( -name "*.actual.png" -o -name "*.diff.png" \) -delete
fi
step test-upstream $UP cargo test -q $T
step test-fast $FAST cargo test -q $T --features gpui-kit/gpui-fast
for c in $CRATES; do $UP cargo test -q -p "$c" --doc > "$LOG/doc-$c.log" 2>&1 || { echo "doctest FAIL $c"; fail=1; }; done
echo "doctests done"
step clippy $UP cargo clippy -q --workspace --all-targets -- -D warnings
step fmt cargo fmt --all --check
echo "changed goldens:"; git status --short -- '*.png' | head -50
[ $fail -eq 0 ] && echo GATE-PASS || echo GATE-FAIL
exit $fail
