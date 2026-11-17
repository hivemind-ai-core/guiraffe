# GUIraffe vs Electron + React: comparison plan

Goal: publishable, reproducible numbers showing where GUIraffe stands against the stack it aims to
replace. Same app, same features, same data, both built for release, measured on the same machine
under the same conditions. Report wins *and* losses honestly.

## The app (identical in both stacks)
A realistic "rich app" exercising what Electron apps actually do:
1. **Shell:** sidebar navigation (router), header, theme toggle (light/dark), command palette (⌘K).
2. **Data grid:** 100k rows × 12 columns, sortable, filterable, row selection (GUIraffe DataGrid vs
   TanStack Table + TanStack Virtual).
3. **Dashboard:** 4 stat cards + 2 charts updating every second from a simulated feed
   (Kit charts vs Recharts).
4. **Markdown + code:** a 2,000-line document with highlighted code blocks (guiraffe-markdown vs
   react-markdown + Shiki).
5. **Agent chat:** streaming reply at ~50 tokens/s into a 200-message history.
6. **Form:** 15 fields with validation (Kit inputs vs Mantine).
React side uses Mantine (closest look and API), Vite build, Electron latest stable, no dev tools.

## Metrics
| Metric | How |
|---|---|
| Cold start → first frame | launch to first painted window (timestamped marker), median of 20 |
| Warm start | same, second launch |
| Idle memory | total RSS/PSS of all processes after 30 s idle (Electron: main + renderer + GPU + utility) |
| Memory under load | PSS while grid is scrolled and chat is streaming |
| Idle CPU | % over 60 s idle (should be ~0 for both if well-built) |
| Frame time | p50/p95/p99 frame time while (a) flinging the grid, (b) streaming chat, (c) resizing the window |
| Input latency | key press → glyph painted in the form (high-speed camera or platform frame timestamps) |
| Interaction cost | sort 100k rows; filter; theme switch: time to settled frame |
| Disk | installed size; download size |
| Battery proxy | package energy (RAPL) over a scripted 2-minute session |

## Method
- Quiet machine: fixed CPU governor (`performance`), no other load (check `uptime` < 1), AC power,
  same display and scale, GPU rendering on for both (no software Vulkan/Xvfb).
- Scripted session driven identically (xdotool/ydotool or platform automation) with a fixed seed.
- 20 runs per metric, report median and p95 with spread; publish raw data + scripts in
  `bench/electron/`.
- Both apps reviewed for "fair effort": idiomatic code a competent developer would write, no
  deliberate pessimisation; React app uses memoisation and virtualisation where idiomatic.

## Deliverables
- `examples/compare/guiraffe-app` and `examples/compare/electron-app` (+ build scripts)
- `bench/electron/run.sh` (measurement harness) and `bench/electron/RESULTS.md`
- A docs page summarising results with charts.

## Prerequisites
DataGrid (in progress), command palette (Spotlight exists), Kit charts (exist), markdown/code (exist),
chat (exists). Schedule after 0.1 is tagged; needs a quiet machine with a real GPU.
