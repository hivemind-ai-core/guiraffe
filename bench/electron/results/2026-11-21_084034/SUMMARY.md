# GUIraffe vs Electron + React: results (2026-11-21_084034)

Machine: `Linux helium 7.2.2-arch1-1 #1 SMP PREEMPT_DYNAMIC Fri, 28 Aug 2026 16:09:39 +0000 x86_64`, 16 CPUs, load at start 1.11. Runs per scenario: 20. Cold start: NOT a true cold start: no passwordless sudo to drop the page cache; 'cold' = first launch of the series.

Cells: median (p95, min–max) across runs.

## Startup and resources

| metric | guiraffe |
|---|---|
| cold start → first frame (harness launch) | 153 ms (p95 153, 153–153) |
| warm start → first frame (harness launch) | 156 ms (p95 160, 152–160) |
| warm start → first frame (since process start, /proc) | 171 ms (p95 177, 164–179) |
| idle PSS, all processes, after 30 s | 97.6 MiB (p95 97.7, 97.0–97.8) |
| idle RSS, all processes, after 30 s | 127.2 MiB (p95 127.4, 126.5–127.5) |
| idle CPU (% of one core) | 0.02 % (p95 0.03, 0.00–0.03) |
| peak PSS under load (grid fling + chat stream) | 151.2 MiB (p95 151.7, 150.3–151.7) |

## Disk

| app | installed | download (tar.gz) |
|---|---|---|
| guiraffe | 50.4 MiB | 18.9 MiB |

## Frame times (ms; p50 / p95 / p99 / max over all frames of all runs)

| scenario | guiraffe |
|---|---|
| grid-fling | 16.7 / 17.2 / 17.5 / 18.2 |
| chat | 16.4 / 17.1 / 17.5 / 19.5 |
| resize | 16.7 / 16.9 / 17.3 / 21.3 |
| load | 16.6 / 17.2 / 17.5 / 20.2 |

## Interactions: time to settled frame (median of each run's median)

| action | guiraffe |
|---|---|
| sort: sort_ms | 21.8 ms (p95 23.2, 21.4–24.9) |
| sort: unsort_ms | 21.7 ms (p95 22.1, 21.1–22.5) |
| filter: filter_ms | 22.2 ms (p95 29.3, 21.7–29.7) |
| filter: clear_ms | 21.5 ms (p95 22.2, 21.0–22.3) |
| theme: theme_ms | 16.3 ms (p95 16.6, 15.7–16.7) |
| grid-fling: total_ms | 3305.4 ms (p95 3306.1, 3304.3–3306.3) |

Raw data: `bench/electron/results/2026-11-21_084034/raw`. Method: `bench/electron/README.md`.
