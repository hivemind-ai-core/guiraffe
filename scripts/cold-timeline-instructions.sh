#!/usr/bin/env bash
# Profile just the first timeline tab activation in the mounted production replay.
set -euo pipefail
if (( $# == 0 )); then
    echo "usage: $0 path/to/release/examples/soak [...]" >&2
    exit 2
fi
root=$(git rev-parse --show-toplevel)
mkdir -p "$root/target"
scratch=$(mktemp -d "$root/target/cold-timeline.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
budget=28000000
failed=0
for binary in "$@"; do
    rm -f -- "$scratch/counts"
    env -u SOAK_ITEMS -u SOAK_LIMIT_PROBE -u SOAK_FRAME_CPU \
        SOAK_FRAMES=33 SOAK_SCALE=1 SOAK_FIXED_STREAM_TICKS=1 \
        valgrind --tool=callgrind --collect-atstart=no \
        '--toggle-collect=*profile_cold_timeline*' \
        --callgrind-out-file="$scratch/counts" "$binary" mixed >"$scratch/run.log"
    # One measured timeline frame, including the tab switch, input, and settling.
    grep -q '^component=timeline frames=1 ' "$scratch/run.log"
    instructions=$(awk '$1 == "summary:" {print $2}' "$scratch/counts")
    if [[ ! $instructions =~ ^[1-9][0-9]*$ ]]; then
        echo "$binary: missing cold timeline instruction count" >&2
        exit 1
    fi
    echo "$binary: cold timeline instructions=$instructions budget=$budget"
    if (( instructions > budget )); then failed=1; fi
done
exit "$failed"
