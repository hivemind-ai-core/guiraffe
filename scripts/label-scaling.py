#!/usr/bin/env python3
"""Gate gpui-fast's release edit instruction counts: virtualized visible and offscreen edits,
and edits of a list with every row mounted, stay flat from 1k to 10k rows."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def build():
    result = subprocess.run(
        ["cargo", "test", "--locked", "--release", "-p", "guiraffe-bench",
         "--features", "gpui-fast", "--test", "composed", "--no-run",
         "--message-format=json-render-diagnostics"],
        check=True, stdout=subprocess.PIPE, text=True,
    )
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("target", {}).get("name") == "composed" and message.get("executable"):
            return Path(message["executable"])
    raise RuntimeError("Cargo returned no composed test executable")


def measure(binary, directory, edit, rows, mounted=False):
    output = directory / f"{edit}-{rows}-{mounted}.cg"
    env = dict(os.environ, COMPOSED_PHASE_ROWS=str(rows), COMPOSED_PHASE_SAMPLES="3",
               COMPOSED_PHASE_EDIT=edit)
    env.pop("COMPOSED_ALL_VISIBLE", None)
    if mounted:
        env["COMPOSED_ALL_VISIBLE"] = "1"
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="Previously built release gpui-fast composed test")
    args = parser.parse_args()
    binary = args.binary.resolve() if args.binary else build()
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    target.mkdir(parents=True, exist_ok=True)
    failed = False
    with tempfile.TemporaryDirectory(dir=target) as directory:
        for edit in ("visible", "offscreen"):
            counts = [measure(binary, Path(directory), edit, n) for n in (1000, 3000, 10000)]
            print(f"{edit}: Ir for three changed frames at 1k/3k/10k = {counts}", flush=True)
            failed |= max(counts[1:]) * 100 > counts[0] * 102
        # Every row mounted and visible: gpui-fast patches the edited row into
        # last frame, so the frame is as flat as a virtualized one. Seven
        # measured frames (four warm-up, three samples) per run; a patched
        # frame cost 139k Ir at 1k and 3k when this was gated, against 28.7M
        # and 85.0M for the splice it replaced.
        counts = [measure(binary, Path(directory), "visible", n, mounted=True) // 7
                  for n in (1000, 3000, 10000)]
        print(f"fully mounted: Ir per changed frame at 1k/3k/10k = {counts}", flush=True)
        failed |= max(counts[1:]) * 100 > counts[0] * 102 or counts[0] > 200_000
    if failed:
        raise SystemExit("Changed-frame instructions exceed the 2% scaling budget or 200k Ir per fully mounted frame")


if __name__ == "__main__":
    main()
