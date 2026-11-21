#!/usr/bin/env python3
"""Summarise a samply profile (recorded with --save-only --unstable-presymbolicate).

    scripts/profile-summary.py PROFILE.json.gz [--top 40] [--thread NAME] [--filter SUBSTR]

Prints the hottest functions by self time and by inclusive (total) time, as a share of the
thread's samples. Names are demangled-ish (Rust hashes stripped). Use --filter to keep only
stacks that pass through a function containing SUBSTR (e.g. a phase such as `paint`).
"""
import argparse
import bisect
import gzip
import json
import re
from collections import Counter
from pathlib import Path


def load_symbols(profile_path: Path):
    syms_path = Path(str(profile_path).removesuffix(".gz")).with_suffix(".syms.json")
    if not syms_path.exists():
        syms_path = Path(str(profile_path).replace(".json.gz", ".json.syms.json"))
    data = json.loads(syms_path.read_text())
    strings = data["string_table"]
    tables = {}
    for lib in data["data"]:
        entries = sorted((e["rva"], e["size"], strings[e["symbol"]]) for e in lib["symbol_table"])
        tables[lib["debug_name"]] = ([e[0] for e in entries], entries)
    return tables


HASH = re.compile(r"::h[0-9a-f]{16}$")


def clean(name: str) -> str:
    name = HASH.sub("", name)
    return name.replace("_$LT$", "<").replace("$GT$", ">").replace("$u20$", " ").replace("..", "::")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("profile", type=Path)
    ap.add_argument("--top", type=int, default=40)
    ap.add_argument("--thread", default=None)
    ap.add_argument("--filter", default=None)
    args = ap.parse_args()

    profile = json.load(gzip.open(args.profile))
    symbols = load_symbols(args.profile)
    libs = [lib["debugName"] for lib in profile["libs"]]
    threads = profile["threads"]
    thread = (
        next(t for t in threads if t["name"] == args.thread)
        if args.thread
        else max(threads, key=lambda t: t["samples"]["length"])
    )
    frames, funcs, stacks = thread["frameTable"], thread["funcTable"], thread["stackTable"]
    resources, strings = thread["resourceTable"], thread["stringArray"]

    cache = {}

    def frame_name(f: int) -> str:
        if f in cache:
            return cache[f]
        func = frames["func"][f]
        address = frames["address"][f]
        resource = funcs["resource"][func]
        name = strings[funcs["name"][func]]
        if resource is not None and resource >= 0 and address is not None and address >= 0:
            lib = libs[resources["lib"][resource]]
            table = symbols.get(lib)
            if table:
                starts, entries = table
                i = bisect.bisect_right(starts, address) - 1
                if i >= 0 and address < entries[i][0] + max(entries[i][1], 1):
                    name = entries[i][2]
                else:
                    name = f"{lib}+{address:#x}"
        cache[f] = clean(name)
        return cache[f]

    def stack_names(s):
        out = []
        while s is not None:
            out.append(frame_name(stacks["frame"][s]))
            s = stacks["prefix"][s]
        return out  # leaf first

    self_time, total_time = Counter(), Counter()
    samples = thread["samples"]
    weights = samples.get("weight") or [1] * samples["length"]
    counted = 0
    for stack, weight in zip(samples["stack"], weights):
        if stack is None:
            continue
        names = stack_names(stack)
        if args.filter and not any(args.filter in n for n in names):
            continue
        w = weight or 1
        counted += w
        self_time[names[0]] += w
        for n in set(names):
            total_time[n] += w

    print(f"thread={thread['name']} samples={counted}" + (f" filter={args.filter!r}" if args.filter else ""))
    for title, counter in (("SELF", self_time), ("INCLUSIVE", total_time)):
        print(f"\n== {title} (top {args.top})")
        for name, w in counter.most_common(args.top):
            print(f"{100 * w / max(counted, 1):6.2f}%  {name[:160]}")


if __name__ == "__main__":
    main()
