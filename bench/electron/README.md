# GUIraffe vs Electron + React: measurement harness

The two apps are in `guiraffe-app/` and `electron-app/`, their shared fixtures in `shared/`, and the contract both follow in [APPS.md](APPS.md).

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

Each run is a fresh launch of `<app> --bench <scenario>` (see `bench/electron/APPS.md` for
the scenarios). The harness:

| metric | how |
|---|---|
| cold / warm start → first frame | harness launch time (epoch ms) to the app's startup marker; also the app's own "since process start" from `/proc/self/stat` |
| idle memory | `idle` scenario: sum of `Rss` and `Pss` from `/proc/<pid>/smaps_rollup` over the whole process tree (Electron: main, zygotes, renderer, GPU, utility) 30 s after launch; per-kind PSS is in the raw data |
| idle CPU | `utime + stime` ticks of the process tree over the following 60 s, as % of one core |
| memory under load | `load` scenario: tree PSS sampled every 0.5 s while the grid is flung for 10 s and then chat streams for 10 s; peak reported |
| frame times | p50/p95/p99/max of frame intervals from each app's continuous frame-callback chain (`grid-fling`, `chat`, `resize`, `load`) |
| interaction cost | `sort`, `filter`, `theme`: time from issuing the action to the scenario's settled callback |
| disk | installed size (GUIraffe: the stripped release binary; Electron: `dist/linux-unpacked` from electron-builder `--dir`), and a tar.gz of it as the download-size proxy |

Output: `results/<date>/raw/<app>-<scenario>.json` (every run, every frame),
`results/<date>/summary.json`, `results/<date>/SUMMARY.md`; a valid run also writes
`bench/electron/RESULTS.md`. Smoke and forced runs go to `results/smoke-*` / `results/invalid-*`
(git-ignored) and never touch `RESULTS.md`.

Not yet covered from the plan: input latency (key → glyph needs a camera or compositor
timestamps) and RAPL energy; both need hardware access the scripted run doesn't assume.

## Caveats when reading the numbers

- Both apps sample a monotonic clock at callback entry (`performance.now()` inside Chromium
  rAF and `Instant::now()` inside GPUI `on_next_frame`). These intervals include callback
  dispatch jitter. Display presentation would require compositor feedback. The November 20
  Electron archive used the rAF timestamp argument, which tracks Chromium's animation timeline,
  so its frame percentiles use a different sampling point within each frame.
- The startup marker is GPUI's first next-frame callback, before that tick draws and submits.
  Electron records its marker after showing the window and waiting for two rAFs. To trace
  GUIraffe startup from main, run the app directly with `COMPARE_TRACE_STARTUP=1` and
  `--bench startup`.
- On native Wayland, run the apps directly with `WAYLAND_DEBUG=client` to inspect
  `wp_presentation_feedback.presented` and `discarded` events. GUIraffe additionally
  requires `GPUI_WAYLAND_PRESENTATION_TRACE=1`. Feedback timestamps describe committed
  buffers. Gaps can include deliberate pauses with no new buffer, and logging itself can
  affect callback timings.
- Electron waits for two rAFs after `flushSync`. GUIraffe waits until the grid is idle and
  then waits for one more next-frame callback for grid actions.
- GUIraffe keeps pages alive after the first visit (GPUI's idiomatic pattern); React unmounts
  them (idiomatic React). The scenarios only visit each page once per launch, so this doesn't
  affect timings, but it does affect memory after navigating around in interactive use.
