#!/usr/bin/env python3
"""Gate gpui-fast's release edit instruction counts: virtualized visible and offscreen edits
stay flat from 1k to 10k rows; fully mounted edits splice the page and stay under a ceiling.

--ab compares the keyed Store write (variant A, store) with the same rows held in a plain
entity and edited directly with cx.notify on the row view (variant B, plain). It runs both
backends, virtualized and fully mounted, at 1k/3k/10k rows, reports CPU, instructions and
allocations per edit and the store's share of each, and gates the store's instruction
overhead over plain."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

CALLGRIND_SAMPLES = 3
CPU_SAMPLES = 31
ROWS = tuple(int(n) for n in os.environ.get("LABEL_AB_ROWS", "1000,3000,10000").split(","))
# Store instructions per edit may exceed plain by AB_TOLERANCE of plain plus AB_FIXED_IR.
# The keyed write routes the edit to the row's selection, compares the old value and
# notifies that view: a fixed cost (about 3,300 Ir and two allocations, measured on
# 2026-11-25) that does not grow with rows. Offscreen, plain does almost nothing, so a
# ratio alone would fail; on visible and mounted edits the store measured 0.0 to 1.2%.
AB_TOLERANCE = float(os.environ.get("LABEL_AB_TOLERANCE", "0.02"))
AB_FIXED_IR = int(os.environ.get("LABEL_AB_FIXED_IR", "4000"))
# Upstream re-renders every ancestor, so a fully mounted 10k edit costs about 150G Ir and
# Callgrind would take most of a night. These cases report CPU and allocations only.
SKIP_IR = set(os.environ.get("LABEL_AB_SKIP_IR", "upstream:mounted:10000").split(","))


def build(fast=True, target_dir=None):
    features = ["--features", "gpui-fast"] if fast else []
    env = dict(os.environ)
    if target_dir:
        env["CARGO_TARGET_DIR"] = str(target_dir)
    result = subprocess.run(
        ["cargo", "test", "--locked", "--release", "-p", "guiraffe-bench", *features,
         "--test", "composed", "--no-run", "--message-format=json-render-diagnostics"],
        check=True, stdout=subprocess.PIPE, text=True, env=env,
    )
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("target", {}).get("name") == "composed" and message.get("executable"):
            return Path(message["executable"])
    raise RuntimeError("Cargo returned no composed test executable")


def sweep_env(edit, rows, mounted, variant, samples):
    env = dict(os.environ, COMPOSED_PHASE_ROWS=str(rows), COMPOSED_PHASE_SAMPLES=str(samples),
               COMPOSED_PHASE_EDIT=edit, COMPOSED_VARIANT=variant)
    env.pop("COMPOSED_ALL_VISIBLE", None)
    if mounted:
        env["COMPOSED_ALL_VISIBLE"] = "1"
    return env


def measure(binary, directory, edit, rows, mounted=False, variant="store"):
    output = directory / f"{variant}-{edit}-{rows}-{mounted}.cg"
    env = sweep_env(edit, rows, mounted, variant, CALLGRIND_SAMPLES)
    sweep = "composed_label_phase_sweep" if mounted else "virtualized_label_phase_sweep"
    run = subprocess.run(
        ["valgrind", "--tool=callgrind", "--collect-atstart=no",
         "--toggle-collect=composed::measured_label_frame", f"--callgrind-out-file={output}",
         str(binary), "--ignored", "--exact", sweep, "--nocapture"],
        env=env, capture_output=True, text=True,
    )
    if run.returncode or run.stdout.count(f"rows={rows} mounted=") != 1:
        raise RuntimeError(run.stdout + run.stderr)
    for line in output.read_text().splitlines():
        if line.startswith("summary:"):
            instructions = int(line.split()[1])
            if instructions > 0:
                return instructions
    raise RuntimeError(f"Missing Callgrind instructions for {edit} at {rows} rows")


def native(binary, edit, rows, mounted, variant):
    """Median CPU nanoseconds and allocations per edit, outside Valgrind."""
    sweep = "composed_label_phase_sweep" if mounted else "virtualized_label_phase_sweep"
    run = subprocess.run(
        [str(binary), "--ignored", "--exact", sweep, "--nocapture"],
        env=sweep_env(edit, rows, mounted, variant, CPU_SAMPLES), capture_output=True, text=True,
    )
    lines = [l for l in run.stdout.splitlines() if l.startswith(f"variant={variant} rows={rows} ")]
    if run.returncode or len(lines) != 1:
        raise RuntimeError(run.stdout + run.stderr)
    fields = dict(part.split("=", 1) for part in lines[0].split() if "=" in part)
    return int(fields["total"]), int(fields["allocs"])


def ab(args):
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
    cases = [("virtualized", "visible", False), ("virtualized", "offscreen", False),
             ("mounted", "visible", True)]
    results, failed = [], []
    for backend, fast in (("upstream", False), ("gpui-fast", True)):
        binary = build(fast, target / f"label-ab-{backend}")
        with tempfile.TemporaryDirectory(dir=target) as directory:
            for layout, edit, mounted in cases:
                for rows in ROWS:
                    row = {"backend": backend, "layout": layout, "edit": edit, "rows": rows}
                    counted = f"{backend}:{layout}:{rows}" not in SKIP_IR
                    for variant in ("store", "plain"):
                        ir = (measure(binary, Path(directory), edit, rows, mounted, variant)
                              if counted else 0)
                        ns, allocs = native(binary, edit, rows, mounted, variant)
                        row[variant] = {"ir": ir // CALLGRIND_SAMPLES, "cpu_ns": ns, "allocs": allocs}
                    store, plain = row["store"], row["plain"]
                    row["store_share"] = {
                        key: (store[key] - plain[key]) / store[key] if store[key] else 0.0
                        for key in ("ir", "cpu_ns", "allocs")
                    }
                    overhead = store["ir"] - plain["ir"]
                    row["ir_overhead"] = overhead if counted else None
                    results.append(row)
                    share = row["store_share"]
                    if not counted:
                        print(f"{backend:9} {layout:11} {edit:9} {rows:>6} rows | instructions "
                              "skipped (LABEL_AB_SKIP_IR)", flush=True)
                    print(f"{backend:9} {layout:11} {edit:9} {rows:>6} rows | "
                          f"store {store['ir']:>11,} Ir {store['cpu_ns']:>10,} ns {store['allocs']:>6} allocs | "
                          f"plain {plain['ir']:>11,} Ir {plain['cpu_ns']:>10,} ns {plain['allocs']:>6} allocs | "
                          f"store share Ir {share['ir']:6.1%} CPU {share['cpu_ns']:6.1%} "
                          f"allocs {share['allocs']:6.1%}", flush=True)
                    if counted and overhead > plain["ir"] * AB_TOLERANCE + AB_FIXED_IR:
                        failed.append(f"{backend} {layout} {edit} {rows}: +{overhead:,} Ir")
    report = args.output or target / "label-ab.json"
    report.write_text(json.dumps(results, indent=2))
    print(f"report: {report}")
    if failed:
        raise SystemExit(f"Store instructions per edit exceed plain by more than "
                         f"{AB_TOLERANCE:.0%} + {AB_FIXED_IR:,} Ir: " + "; ".join(failed))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="Previously built release gpui-fast composed test")
    parser.add_argument("--ab", action="store_true", help="Run the store vs plain A/B instead")
    parser.add_argument("--output", type=Path, help="A/B JSON report (default target/label-ab.json)")
    args = parser.parse_args()
    if args.ab:
        ab(args)
        return
    binary = args.binary.resolve() if args.binary else build()
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    target.mkdir(parents=True, exist_ok=True)
    failed = False
    with tempfile.TemporaryDirectory(dir=target) as directory:
        for edit in ("visible", "offscreen"):
            counts = [measure(binary, Path(directory), edit, n) for n in (1000, 3000, 10000)]
            print(f"{edit}: Ir for three changed frames at 1k/3k/10k = {counts}", flush=True)
            failed |= max(counts[1:]) * 100 > counts[0] * 102
        # Seven measured frames (four warm-up, three samples) per run. Before the
        # page was spliced a fully mounted frame cost 28.7M at 1k and 85.0M at 3k.
        counts = [measure(binary, Path(directory), "visible", n, mounted=True) // 7
                  for n in (1000, 3000)]
        print(f"fully mounted: Ir per changed frame at 1k/3k = {counts}", flush=True)
        failed |= counts[0] > 14_000_000 or counts[1] > 42_000_000
    if failed:
        raise SystemExit("Changed-frame instructions exceed the 2% virtualized scaling or the fully mounted ceiling")


if __name__ == "__main__":
    main()
