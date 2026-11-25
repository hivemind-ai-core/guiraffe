#!/usr/bin/env python3
"""Gate release-built guiraffe-bench soak executables against the mounted timing budgets."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

SIZES = {
    "flamegraph": (32_768, 65_536, 131_072, 262_144, 524_288, 10_000_000),
    "tracks": (32, 64, 128, 256, 1_000),
    # 64 tracks with a clip painter drawing 16 rectangles per clip (P0.3.1 #2: 60 fps).
    "tracks-painter": (50, 100, 200),
    "timeline": (16, 64, 128, 256, 1_000),
    "mermaid": (16, 32, 64, 128, 500),
}

# Frame p99 budgets; 8 ms unless listed. The painter scenario's acceptance is 60 fps.
FRAME_BUDGET_US = {"tracks-painter": 16_000}


def replay(binary, component, settings, timeout):
    env = {k: v for k, v in os.environ.items() if not k.startswith("SOAK_")}
    env.update(settings)
    result = subprocess.run(
        [binary, component], env=env, timeout=timeout, capture_output=True, text=True,
    )
    if result.returncode:
        raise ValueError(f"soak exit {result.returncode}\n{result.stdout}{result.stderr}")
    return result.stdout.splitlines()


def check(binary, component, count):
    settings = dict(SOAK_LIMIT_PROBE=component, SOAK_ITEMS=str(count), SOAK_FRAME_CPU="1")
    lines = replay(binary, component, settings, timeout=60)
    record, = (line for line in lines if line.startswith("LIMIT "))
    mount, p99, _rss = map(int, record.split()[1:])
    cpu = next((line for line in lines if line.startswith("frame_thread_cpu_p99_us=")), "")
    budget = FRAME_BUDGET_US.get(component, 8_000)
    return 0 < mount <= 200_000 and 0 < p99 <= budget, f"mount={mount}us p99={p99}us {cpu}".strip()


def check_mixed(binary):
    # Include tab activation and chart ingestion from the original mixed soak.
    lines = replay(binary, "mixed", {"SOAK_FRAMES": "384", "SOAK_FRAME_CPU": "1"}, timeout=120)
    record, = (line for line in lines if line.startswith("component=timeline "))
    metrics = dict(field.split("=", 1) for field in record.split())
    cpu = next((line for line in lines if line.startswith("cpu_component=timeline ")), "")
    return int(metrics["frames"]) == 32 and int(metrics["max_us"]) <= 8_000, f"{record} {cpu}".strip()


def report(label, operation, *args):
    try:
        passed, details = operation(*args)
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        passed, details = False, str(error)
    print(f"{label}: {details} {'PASS' if passed else 'FAIL'}", flush=True)
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("soak", type=Path, nargs="+", help="one executable per backend")
    args = parser.parse_args()
    passed = True
    for binary in args.soak:
        binary = binary.resolve()
        print(binary, flush=True)
        for component, counts in SIZES.items():
            for count in counts:
                passed &= report(f"{component} {count}", check, binary, component, count)
        passed &= report("mixed", check_mixed, binary)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
