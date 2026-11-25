"""Replay the historical size sweep with a built guiraffe-bench soak example."""

import argparse
import csv
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("soak", type=Path)
parser.add_argument("backend", choices=("upstream", "fast"))
parser.add_argument("--max-ratio", type=float, help="fail if p99 exceeds this multiple of 0.2")
args = parser.parse_args()
if args.max_ratio is not None and not 0 < args.max_ratio < float("inf"):
    parser.error("--max-ratio must be positive and finite")
failed = False
with Path(__file__).with_name("baselines").joinpath("mounted-0.2.csv").open() as source:
    for row in csv.DictReader(source):
        scenario = row["scenario"]
        env = {k: v for k, v in os.environ.items() if not k.startswith("SOAK_")}
        env.update(SOAK_ITEMS=row["items"], SOAK_LIMIT_PROBE=scenario, SOAK_BASELINE_SWEEP="1")
        run = subprocess.run([args.soak.resolve(), scenario], env=env, text=True,
                             stdout=subprocess.PIPE, check=True, timeout=600)
        record, = [s for s in run.stdout.splitlines() if s.startswith("LIMIT ")]
        _, mount, p99, peak = record.split()
        measured = int(p99) / 1000
        reference = row[f"{args.backend}_p99_ms"]
        ratio = measured / float(reference) if reference else None
        print(f"{scenario} n={row['total_items']} mount_ms={int(mount)/1000:.3f} "
              f"p99_ms={measured:.3f} peak_KiB={int(peak)} "
              f"ratio={format(ratio, '.2f') if ratio is not None else 'unmeasured'}", flush=True)
        if args.max_ratio is not None and ratio is not None and ratio > args.max_ratio:
            failed = True
raise SystemExit(int(failed))
