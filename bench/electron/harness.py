"""Measurement driver for bench/electron/run.sh (stdlib only, Linux /proc).

Launches each app with `--bench <scenario>`, samples the whole process tree from /proc while
it runs, and writes raw JSON plus a Markdown summary. See run.sh for the validity checks.
"""

import argparse
import datetime as dt
import json
import os
import statistics
import subprocess
import sys
import tarfile
import threading
import time
from pathlib import Path

CLK_TCK = os.sysconf("SC_CLK_TCK")
SCENARIOS = ["startup", "idle", "grid-fling", "sort", "filter", "theme", "chat", "resize", "load"]


# ------------------------------------------------------------------ /proc helpers

def proc_stat(pid):
    """(ppid, utime+stime ticks) or None."""
    try:
        s = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return None
    f = s.rsplit(")", 1)[1].split()
    return int(f[1]), int(f[11]) + int(f[12])


def tree(root):
    """`root` and all its descendants."""
    children = {}
    for d in os.listdir("/proc"):
        if d.isdigit():
            st = proc_stat(int(d))
            if st:
                children.setdefault(st[0], []).append(int(d))
    out, todo = [], [root]
    while todo:
        p = todo.pop()
        out.append(p)
        todo.extend(children.get(p, []))
    return out


def kind(pid):
    try:
        cmd = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
    except OSError:
        return "gone"
    for a in cmd:
        if a.startswith(b"--type="):
            return a[7:].decode()
    return "main"


def memory(root):
    """Sum of RSS/PSS (KiB) over the tree, plus per-process-kind PSS."""
    rss = pss = 0
    by_kind = {}
    for p in tree(root):
        try:
            lines = Path(f"/proc/{p}/smaps_rollup").read_text().splitlines()
        except OSError:
            continue
        v = {l.split(":")[0]: int(l.split()[1]) for l in lines if l.endswith("kB")}
        rss += v.get("Rss", 0)
        pss += v.get("Pss", 0)
        k = kind(p)
        by_kind[k] = by_kind.get(k, 0) + v.get("Pss", 0)
    return {"rss_kib": rss, "pss_kib": pss, "pss_by_kind_kib": by_kind, "processes": len(tree(root))}


def cpu_ticks(root):
    return sum((proc_stat(p) or (0, 0))[1] for p in tree(root))


# ------------------------------------------------------------------ running

def command(app, repo):
    if app == "guiraffe":
        return [str(repo / "bench/electron/guiraffe-app/target/release/guiraffe-compare")]
    e = repo / "bench/electron/electron-app"
    packaged = e / "dist/linux-unpacked/compare-electron-app"
    if packaged.exists():
        return [str(packaged)]
    return [str(e / "node_modules/.bin/electron"), str(e)]


def drop_caches():
    """Cold start needs an empty page cache; only possible with passwordless sudo."""
    r = subprocess.run(["sudo", "-n", "sh", "-c", "sync; echo 3 > /proc/sys/vm/drop_caches"],
                       capture_output=True)
    return r.returncode == 0


def run_once(app, scenario, repo, idle_secs, idle_sample_at, cpu_window, timeout):
    env = dict(os.environ, BENCH_IDLE_SECS=str(idle_secs))
    launch_ms = time.time() * 1000
    p = subprocess.Popen(command(app, repo) + ["--bench", scenario], stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, env=env, text=True)
    samples, extra = [], {}
    stop = threading.Event()

    def sampler():
        t0 = time.monotonic()
        idle_done = False
        cpu_start = None
        while not stop.is_set():
            t = time.monotonic() - t0
            if scenario == "load":
                samples.append({"t": round(t, 2), **memory(p.pid)})
            if scenario == "idle" and not idle_done:
                if cpu_start is None and t >= idle_sample_at:
                    extra["idle_memory"] = memory(p.pid)
                    cpu_start = (time.monotonic(), cpu_ticks(p.pid))
                elif cpu_start and time.monotonic() - cpu_start[0] >= cpu_window:
                    secs = time.monotonic() - cpu_start[0]
                    ticks = cpu_ticks(p.pid) - cpu_start[1]
                    extra["idle_cpu_percent"] = 100.0 * ticks / CLK_TCK / secs
                    extra["idle_cpu_window_s"] = secs
                    idle_done = True
            stop.wait(0.5)

    th = threading.Thread(target=sampler, daemon=True)
    th.start()
    try:
        out, err = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        p.kill()
        out, err = p.communicate()
        stop.set()
        return {"ok": False, "error": "timeout", "stderr": err[-2000:]}
    stop.set()
    th.join()
    report = None
    for line in out.splitlines():
        if line.startswith("BENCH_JSON "):
            report = json.loads(line[len("BENCH_JSON "):])
    if p.returncode != 0 or report is None:
        return {"ok": False, "error": f"exit {p.returncode}", "stderr": err[-2000:]}
    report["ok"] = True
    report["launch_epoch_ms"] = launch_ms
    report["launch_to_first_frame_ms"] = report["first_frame_epoch_ms"] - launch_ms
    if samples:
        report["load_memory_samples"] = samples
    report.update(extra)
    return report


# ------------------------------------------------------------------ stats

def pct(xs, q):
    xs = sorted(xs)
    if not xs:
        return None
    i = (len(xs) - 1) * q
    lo, hi = int(i), min(int(i) + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (i - lo)


def summary(xs):
    xs = [x for x in xs if x is not None]
    if not xs:
        return None
    return {"n": len(xs), "median": statistics.median(xs), "p95": pct(xs, 0.95),
            "min": min(xs), "max": max(xs)}


def du(path):
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            fp = os.path.join(root, f)
            if not os.path.islink(fp):
                total += os.path.getsize(fp)
    return total


def gz_size(path, tmpdir):
    out = Path(tmpdir) / "size-probe.tar.gz"
    with tarfile.open(out, "w:gz", compresslevel=6) as t:
        t.add(path, arcname=Path(path).name)
    n = out.stat().st_size
    out.unlink()
    return n


def disk(app, repo, tmpdir):
    if app == "guiraffe":
        p = repo / "bench/electron/guiraffe-app/target/release/guiraffe-compare"
    else:
        p = repo / "bench/electron/electron-app/dist/linux-unpacked"
        if not p.exists():
            return {"note": "run `pnpm package` for an installable directory"}
    if not p.exists():
        return {"note": f"{p} missing"}
    return {"path": str(p), "installed_bytes": du(p) if p.is_dir() else p.stat().st_size,
            "download_tar_gz_bytes": gz_size(p, tmpdir)}


def aggregate(runs):
    ok = [r for r in runs if r.get("ok")]
    agg = {"runs": len(runs), "failed": len(runs) - len(ok)}
    agg["launch_to_first_frame_ms"] = summary([r["launch_to_first_frame_ms"] for r in ok])
    agg["first_frame_since_process_start_ms"] = summary(
        [r.get("first_frame_since_process_start_ms") for r in ok])
    frames = [f for r in ok for f in r.get("frames_ms", [])]
    if frames:
        agg["frame_ms"] = {"n": len(frames), "p50": pct(frames, .5), "p95": pct(frames, .95),
                           "p99": pct(frames, .99), "max": max(frames)}
        per_run_p95 = [pct(r["frames_ms"], .95) for r in ok if r.get("frames_ms")]
        agg["frame_p95_across_runs"] = summary(per_run_p95)
    keys = {k for r in ok for k in r.get("timings_ms", {}) if not k.endswith("frames_ms")}
    agg["timings_ms"] = {k: summary([statistics.median(r["timings_ms"][k]) for r in ok
                                     if r.get("timings_ms", {}).get(k)]) for k in sorted(keys)}
    if any("idle_memory" in r for r in ok):
        agg["idle_pss_mib"] = summary([r["idle_memory"]["pss_kib"] / 1024 for r in ok if "idle_memory" in r])
        agg["idle_rss_mib"] = summary([r["idle_memory"]["rss_kib"] / 1024 for r in ok if "idle_memory" in r])
        agg["idle_cpu_percent"] = summary([r.get("idle_cpu_percent") for r in ok])
    if any("load_memory_samples" in r for r in ok):
        agg["load_peak_pss_mib"] = summary(
            [max(s["pss_kib"] for s in r["load_memory_samples"]) / 1024 for r in ok if r.get("load_memory_samples")])
    return agg


# ------------------------------------------------------------------ report

def fmt(s, unit="", digits=1):
    if not s:
        return "–"
    f = lambda x: f"{x:.{digits}f}"
    return f"{f(s['median'])}{unit} (p95 {f(s['p95'])}, {f(s['min'])}–{f(s['max'])})"


def markdown(meta, results, disks):
    apps = list(results)
    L = []
    if meta["invalid"]:
        L += ["> **INVALID — NOT A MEASUREMENT.** " + meta["invalid"],
              "> These numbers only show that the pipeline runs end to end. Do not publish them.", ""]
    L += [f"# GUIraffe vs Electron + React: results ({meta['date']})", "",
          f"Machine: `{meta['uname']}`, {meta['cpus']} CPUs, load at start {meta['load']}. "
          f"Runs per scenario: {meta['runs']}. Cold start: {meta['cold_note']}.", "",
          "Cells: median (p95, min–max) across runs.", ""]

    def row(label, get, unit="", digits=1):
        L.append(f"| {label} | " + " | ".join(fmt(get(a), unit, digits) for a in apps) + " |")

    L += ["## Startup and resources", "", "| metric | " + " | ".join(apps) + " |",
          "|---|" + "---|" * len(apps)]
    st = lambda a, k, s="startup": (results[a].get(s) or {}).get(k)
    row("cold start → first frame (harness launch)", lambda a: st(a, "launch_to_first_frame_ms", "startup-cold"), " ms", 0)
    row("warm start → first frame (harness launch)", lambda a: st(a, "launch_to_first_frame_ms"), " ms", 0)
    row("warm start → first frame (since process start, /proc)", lambda a: st(a, "first_frame_since_process_start_ms"), " ms", 0)
    row("idle PSS, all processes, after 30 s", lambda a: st(a, "idle_pss_mib", "idle"), " MiB")
    row("idle RSS, all processes, after 30 s", lambda a: st(a, "idle_rss_mib", "idle"), " MiB")
    row("idle CPU (% of one core)", lambda a: st(a, "idle_cpu_percent", "idle"), " %", 2)
    row("peak PSS under load (grid fling + chat stream)", lambda a: st(a, "load_peak_pss_mib", "load"), " MiB")
    L.append("")
    L += ["## Disk", "", "| app | installed | download (tar.gz) |", "|---|---|---|"]
    for a in apps:
        d = disks.get(a, {})
        if "installed_bytes" in d:
            L.append(f"| {a} | {d['installed_bytes'] / 2**20:.1f} MiB | {d['download_tar_gz_bytes'] / 2**20:.1f} MiB |")
        else:
            L.append(f"| {a} | {d.get('note', '–')} | |")
    L += ["", "## Frame times (ms; p50 / p95 / p99 / max over all frames of all runs)", "",
          "| scenario | " + " | ".join(apps) + " |", "|---|" + "---|" * len(apps)]
    for s in ["grid-fling", "chat", "resize", "load"]:
        cells = []
        for a in apps:
            fm = (results[a].get(s) or {}).get("frame_ms")
            cells.append(f"{fm['p50']:.1f} / {fm['p95']:.1f} / {fm['p99']:.1f} / {fm['max']:.1f}" if fm else "–")
        L.append(f"| {s} | " + " | ".join(cells) + " |")
    L += ["", "## Interactions: time to settled frame (median of each run's median)", "",
          "| action | " + " | ".join(apps) + " |", "|---|" + "---|" * len(apps)]
    for s, k in [("sort", "sort_ms"), ("sort", "unsort_ms"), ("filter", "filter_ms"),
                 ("filter", "clear_ms"), ("theme", "theme_ms"), ("grid-fling", "total_ms")]:
        row(f"{s}: {k}", lambda a: ((results[a].get(s) or {}).get("timings_ms") or {}).get(k), " ms")
    fails = [(a, s, r["failed"]) for a in apps for s, r in results[a].items() if r and r["failed"]]
    if fails:
        L += ["", "Failed runs: " + ", ".join(f"{a}/{s}: {n}" for a, s, n in fails)]
    L += ["", "Raw data: `" + meta["raw_dir"] + "`. Method: `bench/electron/README.md`."]
    return "\n".join(L) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--smoke", action="store_true")
    ap.add_argument("--invalid", default="")
    ap.add_argument("--runs", type=int)
    ap.add_argument("--apps", default="guiraffe,electron")
    ap.add_argument("--scenarios", default=",".join(SCENARIOS))
    a = ap.parse_args()
    repo = Path(a.repo)
    runs = a.runs or (2 if a.smoke else 20)
    idle_secs, idle_at, cpu_window = (12, 4, 6) if a.smoke else (95, 30, 60)
    invalid = a.invalid or ("--smoke run" if a.smoke else "")
    if a.smoke and "smoke" not in invalid:
        invalid = "--smoke run; " + invalid
    date = dt.datetime.now().strftime("%Y-%m-%d_%H%M%S")
    base = repo / "bench/electron/results"
    out = base / (f"smoke-{date}" if a.smoke else f"invalid-{date}" if invalid else date)
    (out / "raw").mkdir(parents=True, exist_ok=True)
    tmpdir = os.environ.get("TMPDIR", str(out))

    apps = a.apps.split(",")
    scenarios = a.scenarios.split(",")
    can_drop = drop_caches()
    cold_note = ("page cache dropped before each cold run (sudo)" if can_drop else
                 "NOT a true cold start: no passwordless sudo to drop the page cache; "
                 "'cold' = first launch of the series")
    results = {app: {} for app in apps}
    for app in apps:
        for s in scenarios:
            series = [s] if s != "startup" else ["startup-cold", "startup"]
            for name in series:
                rs = []
                for i in range(1 if (name == "startup-cold" and not can_drop) else runs):
                    if name == "startup-cold" and can_drop:
                        drop_caches()
                    timeout = idle_secs + 60 if s == "idle" else 120
                    r = run_once(app, s, repo, idle_secs, idle_at, cpu_window, timeout)
                    r["run"] = i
                    rs.append(r)
                    status = "ok" if r.get("ok") else f"FAIL {r.get('error')}"
                    print(f"[{app}] {name} {i + 1}/{runs}: {status}", file=sys.stderr, flush=True)
                    time.sleep(0.5 if a.smoke else 2)
                (out / "raw" / f"{app}-{name}.json").write_text(json.dumps(rs))
                results[app][name] = aggregate(rs)
    disks = {app: disk(app, repo, tmpdir) for app in apps}
    meta = {"date": date, "invalid": invalid, "uname": " ".join(os.uname()), "cpus": os.cpu_count(),
            "load": open("/proc/loadavg").read().split()[0], "runs": runs, "cold_note": cold_note,
            "raw_dir": str(out.relative_to(repo) / "raw")}
    (out / "summary.json").write_text(json.dumps({"meta": meta, "results": results, "disk": disks}, indent=1))
    md = markdown(meta, results, disks)
    (out / "SUMMARY.md").write_text(md)
    if not invalid:
        (repo / "bench/electron/RESULTS.md").write_text(md)
        print(f"wrote bench/electron/RESULTS.md and {out}", file=sys.stderr)
    else:
        print(f"INVALID run: wrote {out}/SUMMARY.md only (RESULTS.md untouched)", file=sys.stderr)
    print(md)
    return 1 if any(r and r["failed"] for x in results.values() for r in x.values()) else 0


if __name__ == "__main__":
    sys.exit(main())
