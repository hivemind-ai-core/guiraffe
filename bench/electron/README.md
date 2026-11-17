# GUIraffe vs Electron + React: measurement harness

Plan: `plans/ELECTRON-COMPARISON.md`. Apps and their shared contract: `examples/compare/`.

```sh
bench/electron/run.sh                 # full run: builds both, 20 runs per scenario per app
bench/electron/run.sh --smoke         # pipeline check: 2 runs, 12 s idle; marked INVALID
bench/electron/run.sh --no-build --runs 5 --apps guiraffe --scenarios startup,sort
```

Requirements: Rust (stable), Node + pnpm, python3. For a true cold start, passwordless
`sudo` so the page cache can be dropped (`sync; echo 3 > /proc/sys/vm/drop_caches`); without it
the "cold" row is only the first launch of the series, and the summary says so.

## Before a real run

The harness **refuses to run** (unless `--smoke` or `--force`, both of which mark results
INVALID) when:
- 1-minute load average > 1.0;
- `DISPLAY` is an Xvfb server (`:9x`, or a running `Xvfb` for that display), or there's no display;
- the GPU info (`glxinfo -B`, `vulkaninfo --summary`) names a software renderer
  (llvmpipe, lavapipe, softpipe, swrast, SwiftShader), or neither tool is available;
- `LIBGL_ALWAYS_SOFTWARE` is set;
- the CPU governor isn't `performance`.

Also: AC power, the same monitor and scale for both apps, nothing else running, and leave the
windows alone (each run opens a window on the real display).

## What it measures

Each run is a fresh launch of `<app> --bench <scenario>` (see `examples/compare/README.md` for
the scenarios). The harness:

| metric | how |
|---|---|
| cold / warm start → first frame | harness launch time (epoch ms) to the app's first-presented-frame timestamp; also the app's own "since process start" from `/proc/self/stat` |
| idle memory | `idle` scenario: sum of `Rss` and `Pss` from `/proc/<pid>/smaps_rollup` over the whole process tree (Electron: main, zygotes, renderer, GPU, utility) 30 s after launch; per-kind PSS is in the raw data |
| idle CPU | `utime + stime` ticks of the process tree over the following 60 s, as % of one core |
| memory under load | `load` scenario: tree PSS sampled every 0.5 s while the grid is flung for 10 s and then chat streams for 10 s; peak reported |
| frame times | p50/p95/p99/max of frame intervals from each app's continuous frame-callback chain (`grid-fling`, `chat`, `resize`, `load`) |
| interaction cost | `sort`, `filter`, `theme`: time from issuing the action to the first frame presented with its result |
| disk | installed size (GUIraffe: the stripped release binary; Electron: `dist/linux-unpacked` from electron-builder `--dir`), and a tar.gz of it as the download-size proxy |

Output: `results/<date>/raw/<app>-<scenario>.json` (every run, every frame),
`results/<date>/summary.json`, `results/<date>/SUMMARY.md`; a valid run also writes
`bench/electron/RESULTS.md`. Smoke and forced runs go to `results/smoke-*` / `results/invalid-*`
(git-ignored) and never touch `RESULTS.md`.

Not yet covered from the plan: input latency (key → glyph needs a camera or compositor
timestamps) and RAPL energy; both need hardware access the scripted run doesn't assume.

## Caveats when reading the numbers

- Frame intervals measure how often each app's frame callback runs (rAF in Chromium,
  `on_next_frame` in GPUI). On a 60 Hz display both are capped near 16.7 ms; the interesting
  numbers are p95/p99/max.
- "Settled" in Electron is `flushSync` + two rAFs; in GUIraffe it's "grid not busy" + the
  next presented frame. Both include about one frame of latency by construction.
- GUIraffe keeps pages alive after the first visit (GPUI's idiomatic pattern); React unmounts
  them (idiomatic React). The scenarios only visit each page once per launch, so this doesn't
  affect timings, but it does affect memory after navigating around in interactive use.
