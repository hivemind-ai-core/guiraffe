#!/usr/bin/env python3
"""Gate mounted component timings using a release-built guiraffe-bench soak executable."""
import argparse
import os
import subprocess
import sys
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("soak", type=Path)
args = parser.parse_args()
sizes = {
    "flamegraph": (32_768, 65_536, 131_072, 262_144, 524_288, 10_000_000),
    "tracks": (32, 64, 128, 256, 1024),
    "timeline": (16, 64, 128, 256, 1024),
    "mermaid": (16, 32, 64, 128, 500),
}
failed = False
for component, counts in sizes.items():
    for count in counts:
        env = dict(os.environ, SOAK_LIMIT_PROBE=component, SOAK_ITEMS=str(count))
        for key in ("SOAK_FRAMES", "SOAK_SCALE", "SOAK_FLOW_WARM_REPLAY"):
            env.pop(key, None)
        result = subprocess.run(
            [args.soak.resolve(), component], env=env, timeout=60,
            stdout=subprocess.PIPE, text=True, check=True,
        )
        record, = (line for line in result.stdout.splitlines() if line.startswith("LIMIT "))
        mount, p99, _rss = map(int, record.split()[1:])
        passed = 0 < mount <= 200_000 and 0 < p99 <= 8_000
        print(f"{component} {count}: mount={mount}us p99={p99}us "
              f"{'PASS' if passed else 'FAIL'}", flush=True)
        failed |= not passed
# The original dope-sheet regression includes tab activation and concurrent
# chart ingestion. Exercise complete mixed cycles at the mounted fixture size.
mixed_env = dict(os.environ, SOAK_FRAMES="384")
for key in ("SOAK_LIMIT_PROBE", "SOAK_ITEMS", "SOAK_SCALE", "SOAK_FLOW_WARM_REPLAY"):
    mixed_env.pop(key, None)
mixed = subprocess.run(
    [args.soak.resolve(), "mixed"], env=mixed_env, timeout=120,
    stdout=subprocess.PIPE, text=True, check=True,
)
record, = (line for line in mixed.stdout.splitlines() if line.startswith("component=timeline "))
metrics = dict(field.split("=", 1) for field in record.split())
passed = int(metrics["frames"]) > 0 and int(metrics["max_us"]) <= 8_000
print(f"mixed {record} {'PASS' if passed else 'FAIL'}", flush=True)
sys.exit(int(failed or not passed))
