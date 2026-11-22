#!/usr/bin/env -S python3 -I
"""Broad, short leak check over the release soak example (Linux, needs Valgrind).

    scripts/leak-check.py --soak target/gate-up/release/examples/soak --out target/leak/up

Two questions, two tools:

* memcheck runs every scenario twice, short and four times longer, and compares what is lost at
  exit. A leak per interaction grows with the frame count; one-off global state doesn't. It also
  reports invalid reads, writes and frees.
* massif runs the mixed soak at full size and compares the live heap between the middle and the
  end of the run. This catches what memcheck can't: memory that is still referenced (caches,
  histories, subscriptions) and only released at teardown. A flat live heap with a growing RSS
  points at allocator fragmentation instead of a leak.

Exit status is 1 when lost memory grows with the frame count, memcheck sees an invalid access, the
live heap grows more than 2 %, or any run fails.
"""

import argparse
import concurrent.futures
import os
import re
import statistics
import subprocess
import sys
import time

SCENARIOS = (
    "mixed sheet grid code code-annotations flow timeline chart-line chart-scatter chart-stream "
    "gantt gantt-fan-out gantt-chain tracks curve-editor bar-editor flamegraph tree property-grid "
    "markdown mermaid chat"
).split()
KINDS = ("definitely lost", "indirectly lost", "possibly lost", "still reachable")
INVALID = re.compile(
    r"^==\d+== (Invalid read|Invalid write|Invalid free|Mismatched free|Jump to the invalid)"
)
GROWTH_SLACK = 1024


def run(command, env, log, timeout):
    started = time.time()
    with open(log, "w") as sink:
        try:
            code = subprocess.run(
                command, env=env, stdout=sink, stderr=sink, timeout=timeout
            ).returncode
        except subprocess.TimeoutExpired:
            code = "timeout"
    return code, time.time() - started


def memcheck(args, name, frames):
    stem = os.path.join(args.out, f"{name}-{frames}")
    env = dict(os.environ, SOAK_FRAMES=str(frames), SOAK_SCALE=str(args.scale))
    command = [
        "valgrind",
        "--leak-check=full",
        "--num-callers=40",
        "--undef-value-errors=no",
        f"--log-file={stem}.memcheck",
        args.soak,
        name,
    ]
    code, seconds = run(command, env, f"{stem}.log", args.timeout)
    lost = {}
    invalid = 0
    if os.path.exists(f"{stem}.memcheck"):
        with open(f"{stem}.memcheck", errors="replace") as log:
            for line in log:
                invalid += bool(INVALID.match(line))
                for kind in KINDS:
                    found = re.search(rf"{kind}: ([\d,]+) bytes", line)
                    if found:
                        lost[kind] = int(found.group(1).replace(",", ""))
    return name, frames, code, seconds, lost, invalid


def snapshots(path):
    """Massif snapshots as (time, heap bytes, {call path: bytes})."""
    result = []
    stack = []
    with open(path, errors="replace") as log:
        for line in log:
            if line.startswith("time="):
                result.append([int(line[5:]), 0, {}])
                stack = []
            elif line.startswith("mem_heap_B="):
                result[-1][1] = int(line[11:])
            else:
                node = re.match(r"( *)n(\d+): (\d+) (.*)", line)
                if not node or not result:
                    continue
                depth, children, size, label = node.groups()
                del stack[len(depth) :]
                label = re.sub(r"^0x[0-9A-Fa-f]+: | \(in [^)]*\)$", "", label.strip())
                stack.append(label)
                if children == "0":
                    key = tuple(stack[1:])
                    sites = result[-1][2]
                    sites[key] = sites.get(key, 0) + int(size)
    return result


def site(path):
    """Name a call path by its first frame outside the allocator and std containers."""
    plumbing = re.compile(
        r"^<?(alloc|core|std|hashbrown)(::|\[)|malloc|realloc|calloc|__rust|raw_vec|RawVec|below massif"
    )
    useful = [frame for frame in path if not plumbing.search(frame)]
    return " <- ".join(frame[:110] for frame in useful[:3]) or "(below threshold)"


def live_heap(args):
    out = os.path.join(args.out, "mixed.massif")
    env = dict(os.environ, SOAK_FRAMES=str(args.heap_frames))
    command = [
        "valgrind",
        "--tool=massif",
        "--time-unit=ms",
        "--detailed-freq=1",
        "--max-snapshots=120",
        "--threshold=0.2",
        "--depth=18",
        f"--massif-out-file={out}",
        args.soak,
        "mixed",
    ]
    code, seconds = run(command, env, os.path.join(args.out, "mixed-massif.log"), args.timeout)
    if code != 0:
        return code, seconds, None
    series = snapshots(out)
    end = series[-1][0]
    # Drop teardown, then compare a window after warm-up with one at the end.
    middle = [s for s in series if 0.30 * end <= s[0] < 0.50 * end]
    last = [s for s in series if 0.78 * end <= s[0] < 0.98 * end]
    if not middle or not last:
        return "too few snapshots", seconds, None
    before = statistics.median(s[1] for s in middle)
    after = statistics.median(s[1] for s in last)

    def mean(window):
        totals = {}
        for _, _, sites in window:
            for path, size in sites.items():
                key = site(path)
                totals[key] = totals.get(key, 0) + size / len(window)
        return totals

    old, new = mean(middle), mean(last)
    grown = sorted(((new[k] - old.get(k, 0), k) for k in new), reverse=True)
    return code, seconds, (before, after, [g for g in grown if g[0] > 8192][:20])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    parser.add_argument("--soak", required=True, help="release soak example binary")
    parser.add_argument("--out", required=True, help="directory for logs (not /tmp)")
    parser.add_argument("--frames", type=int, default=192, help="short memcheck run; long is 4x")
    parser.add_argument("--heap-frames", type=int, default=9600, help="massif mixed run")
    parser.add_argument("--scale", type=int, default=50, help="dataset divisor for memcheck")
    parser.add_argument("--jobs", type=int, default=10)
    parser.add_argument("--timeout", type=int, default=600, help="seconds per run")
    args = parser.parse_args()
    args.soak = os.path.abspath(args.soak)
    args.out = os.path.abspath(args.out)
    os.makedirs(args.out, exist_ok=True)

    failed = []
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        heap = pool.submit(live_heap, args)
        checks = [
            pool.submit(memcheck, args, name, frames)
            for name in SCENARIOS
            for frames in (args.frames, args.frames * 4)
        ]
        runs = {}
        for check in concurrent.futures.as_completed(checks):
            name, frames, *rest = check.result()
            runs[name, frames] = rest

        print("scenario            lost(short) lost(long) possibly(+) reachable(+) invalid  verdict")
        for name in SCENARIOS:
            short, long = runs[name, args.frames], runs[name, args.frames * 4]
            if short[0] != 0 or long[0] != 0 or len(short[2]) < 4 or len(long[2]) < 4:
                print(f"{name:19} run failed: short={short[0]} long={long[0]}")
                failed.append(name)
                continue
            lost = [r[2]["definitely lost"] + r[2]["indirectly lost"] for r in (short, long)]
            possibly = long[2]["possibly lost"] - short[2]["possibly lost"]
            reachable = long[2]["still reachable"] - short[2]["still reachable"]
            invalid = short[3] + long[3]
            verdict = "ok"
            if lost[1] > lost[0] + GROWTH_SLACK:
                verdict = "LEAK"
            elif invalid:
                verdict = "INVALID ACCESS"
            elif max(possibly, reachable) > 65536:
                verdict = "ok (retained globals grew)"
            if verdict in ("LEAK", "INVALID ACCESS"):
                failed.append(name)
            print(
                f"{name:19} {lost[0]:11} {lost[1]:10} {possibly:11} {reachable:12} {invalid:7}  {verdict}",
                flush=True,
            )

        code, seconds, result = heap.result()
        if result is None:
            print(f"live heap: run failed ({code})")
            failed.append("live heap")
        else:
            before, after, grown = result
            change = (after / before - 1) * 100
            print(
                f"\nlive heap, mixed, {args.heap_frames} frames ({seconds:.0f} s): "
                f"{before / 1048576:.2f} -> {after / 1048576:.2f} MiB ({change:+.1f} %)"
            )
            for size, key in grown:
                print(f"  +{size / 1024:8.1f} KiB  {key}")
            if change > 2:
                failed.append("live heap")

    print("\nFAIL: " + ", ".join(failed) if failed else "\nPASS")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
