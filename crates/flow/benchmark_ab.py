#!/usr/bin/env python3
"""Alternate prebuilt benchmark binaries on one CPU; retain raw output and spread."""
import argparse
import datetime
import json
import pathlib
import re
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
for mode in ("core", "edit", "ui"):
    for variant in ("old", "new"):
        parser.add_argument(f"--{variant}-{mode}", required=True)
parser.add_argument("--old-delete")
parser.add_argument("--new-delete")
parser.add_argument("--cpu", type=int, required=True)
parser.add_argument("--runs", type=int, default=10)
parser.add_argument("--old-revision", required=True)
parser.add_argument("--new-revision", required=True)
parser.add_argument("--output", type=pathlib.Path, required=True)
args = parser.parse_args()
if bool(args.old_delete)!=bool(args.new_delete):
    parser.error("provide both child-delete binaries")
modes=("core","edit","ui")+(("delete",) if args.old_delete else ())
rows = []
for run in range(args.runs):
    for mode in modes:
        for variant in (("old", "new") if run % 2 == 0 else ("new", "old")):
            started = time.monotonic()
            result = subprocess.run(
                ["taskset", "-c", str(args.cpu), getattr(args, f"{variant}_{mode}")],
                check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            output = result.stdout
            metrics = {}
            if mode == "core":
                match = re.search(r"build=([\d.]+)(ms|us|µs|ns|s)", output)
                if not match:
                    raise ValueError(output)
                metrics["build_us"] = float(match[1]) * {"ms": 1000, "us": 1, "µs": 1, "ns": .001, "s": 1e6}[match[2]]
            # Edit output includes a second, labelled reconstruction reference. Capture
            # the actual Editor distribution before that reference for old/new comparison.
            measured = output.split("full reconstruction reference")[0]
            for key, value in re.findall(r"(p50|p95|p99)=(\d+)us", measured):
                metrics[key + "_us"] = int(value)
            rows.append(dict(run=run, mode=mode, variant=variant, metrics=metrics,
                             wall_seconds=time.monotonic()-started, stdout=output))
            print(f"run={run+1}/{args.runs} {mode} {variant}: {metrics}", flush=True)
summary = {}
for mode in modes:
    summary[mode] = {}
    for variant in ("old", "new"):
        samples = [r["metrics"] for r in rows if r["mode"] == mode and r["variant"] == variant]
        summary[mode][variant] = {
            key: dict(median=statistics.median(s[key] for s in samples),
                      minimum=min(s[key] for s in samples), maximum=max(s[key] for s in samples))
            for key in samples[0]
        }
args.output.write_text(json.dumps(dict(
    utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), cpu=args.cpu, runs=args.runs,
    old_revision=args.old_revision, new_revision=args.new_revision,
    protocol="Alternating AB/BA by run, same CPU and gpui-kit 0.9.1; shared host, no GPU/presentation",
    summary=summary, rows=rows,
), indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
