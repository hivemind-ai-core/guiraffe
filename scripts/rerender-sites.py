#!/usr/bin/env python3
"""Attribute a re-rendered view's allocations on gpui-fast to call sites.

Runs bench/tests/rerender_sites.rs under valgrind's DHAT for each view kind, twice: with N and
with 2N re-renders. Per site, the difference divided by N is what one re-render allocates there;
mounting and startup cancel out. Prints each site (the innermost frame outside the allocator and
the standard collections), the component or test frame that owns it, and a category. The numbers
feed docs/performance.md.

    python3 scripts/rerender-sites.py [--binary PATH] [--iterations N] [--depth N]
"""

import argparse
import collections
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

# Frames that only say an allocation happened, not why.
PLUMBING = re.compile(
    r"(alloc::|core::|std::|hashbrown::|smallvec::|__rust|__rdl|<guiraffe_bench::allocator|"
    r"malloc|realloc|calloc|_vgr|RawVec|raw_vec|<alloc::|<core::|<std::|<hashbrown::|<smallvec::|"
    r"collections::|indexmap::|<indexmap::)"
)
# Frames that say whose work it is, other than GPUI's: a component, or the view's own render.
OWNER = re.compile(r"^<?(gpui_component::|gpui_base::|rerender_sites::RowView as )")

# What a site's allocation is for, by the first pattern the site matches.
CATEGORIES = [
    ("listener closures", r"on_mouse_event|on_click|on_key|on_action|LazyVec"),
    ("element ids", r"smol_str|ElementId|TransitionId|motion::spring|format"),
    ("retained records", r"begin_retained|finish_retained|rebuild_here|cached_gap"),
    ("gap context stacks", r"enter_gap"),
    ("patch bookkeeping", r"fast::patch::|claimed_keys|flush_effects"),
    ("accessibility", r"accesskit|a11y"),
    ("text", r"SharedString as gpui::element::Element|TextStyle|shape|line_layout"),
    ("element builders", r"Interactivity>::new|::render|ParentElement|request_layout|Observed"),
]


def build():
    result = subprocess.run(
        ["cargo", "test", "--release", "-p", "guiraffe-bench", "--features", "gpui-fast",
         "--test", "rerender_sites", "--no-run", "--message-format=json-render-diagnostics"],
        check=True, stdout=subprocess.PIPE, text=True,
    )
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("target", {}).get("name") == "rerender_sites" and message.get("executable"):
            return Path(message["executable"])
    raise RuntimeError("Cargo returned no rerender_sites test executable")


def function(frame):
    # "0x1234: name (in /path)" or "0x1234: name (file.rs:12:3)"
    text = frame.split(": ", 1)[-1]
    return re.sub(r" \((in |[^)]*\.rs:)[^)]*\)$", "", text)


def sites_of(binary, view, iterations, directory):
    output = directory / f"dhat-{view}-{iterations}.json"
    env = dict(os.environ, RERENDER_VIEW=view, RERENDER_ITERATIONS=str(iterations))
    subprocess.run(
        ["valgrind", "--tool=dhat", f"--dhat-out-file={output}", "--num-callers=40",
         str(binary), "--ignored", "--exact", "rerendered_view_allocations", "--nocapture"],
        env=env, check=True, capture_output=True, text=True,
    )
    data = json.loads(output.read_text())
    frames = data["ftbl"]
    counts = collections.Counter()
    for point in data["pps"]:
        stack = [function(frames[index]) for index in point["fs"]]
        counts[tuple(stack)] += point["tbk"]
    return counts


def measure(binary, view, iterations, directory):
    once = sites_of(binary, view, iterations, directory)
    twice = sites_of(binary, view, 2 * iterations, directory)
    sites = collections.Counter()
    owners = collections.Counter()
    categories = collections.Counter()
    total = 0
    for stack, count in twice.items():
        per = round((count - once.get(stack, 0)) / iterations)
        if per <= 0:
            continue
        site = next((name for name in stack if not PLUMBING.search(name)), stack[-1])
        owner = next((name for name in stack if OWNER.search(name)), "gpui")
        category = next(
            (name for name, pattern in CATEGORIES if re.search(pattern, site)), "other"
        )
        sites[(site, category)] += per
        owners[owner.split("::")[0].lstrip("<").replace("rerender_sites", "view render")] += per
        categories[category] += per
        total += per
    return total, sites, owners, categories


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--iterations", type=int, default=1000)
    parser.add_argument("--depth", type=int, default=25, help="sites to print per view")
    args = parser.parse_args()
    binary = args.binary.resolve() if args.binary else build()
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    target.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=target) as directory:
        for view in ("empty", "plain", "component", "store"):
            total, sites, owners, categories = measure(
                binary, view, args.iterations, Path(directory)
            )
            print(f"\n== {view}: {total} allocations per re-render attributed")
            for (site, category), count in sites.most_common(args.depth):
                print(f"{count:4}  [{category}] {site[:140]}")
            print("-- by category")
            for category, count in categories.most_common():
                print(f"{count:4}  {category}")
            print("-- by crate")
            for owner, count in owners.most_common():
                print(f"{count:4}  {owner}")


if __name__ == "__main__":
    main()
