# GUIraffe vs Electron + React: the comparison apps

The same app built twice for the Electron comparison:

- `guiraffe-app/`: GUIraffe on gpui-kit `=0.9.1` with the `gpui-fast` backend. A standalone
  crate with its own `[workspace]`, as `guiraffe new` produces. Release profile: `lto = "thin"`.
- `electron-app/`: Electron (latest stable) + React 19 + Vite + Mantine + TanStack Table/Virtual
  + Recharts + react-markdown/Shiki.
- `shared/`: fixtures used by both (`document.md`, `chat.json`, made by `gen-fixtures.mjs`).

The measurement harness is `bench/electron/run.sh`; results go to `bench/electron/RESULTS.md`.

This file is the **contract** both apps follow. If you change behaviour in one app, change the
other and this file.

## Window

1280 × 800 logical pixels, title "Compare: GUIraffe" / "Compare: Electron". Initial route: Dashboard.
Light theme at start.

## Shell

Sidebar (router) with five routes, in order: Dashboard `/`, Data grid `/grid`, Document `/docs`,
Chat `/chat`, Form `/form`. Header: app title, current page title, theme toggle (light/dark).
Command palette on Ctrl+K (Cmd+K on macOS): commands "Go to <page>" for each route and
"Toggle theme".

## Data grid: the seeded generator

`mulberry32`, seed `42`, returning a `u32` per call:

```js
function mulberry32(a) {
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
}
```

Rust uses the same arithmetic with `u32::wrapping_*` (`a` as `u32`; `|0` is a wrapping add;
`Math.imul` is `wrapping_mul`; `>>>` is a logical shift on `u32`).

100,000 rows. For row `i` (0-based) draw, **in this order**, `r()` = next u32:

| draw | field |
|---|---|
| `FIRST[r() % 16]` | first name |
| `LAST[r() % 16]` | last name |
| `c = r() % 12` | `CITIES[c]`, `COUNTRIES[c]` |
| `DEPTS[r() % 8]` | department |
| `STATUS[r() % 4]` | status |
| `(r() % 1_000_000) / 100` | amount (float) |
| `r() % 500` | quantity (int) |
| `(r() % 10_000) / 100` | score (float) |
| `r() % 2 == 0` | active (bool) |
| `r() % 1500` | created = 2022-01-01 + that many days, `YYYY-MM-DD` (UTC) |

Derived: `id = i + 1`; `name = first + " " + last`;
`email = lowercase(first + "." + last + (i % 100) + "@example.com")`.

```
FIRST     = Ada Alan Grace Linus Barbara Ken Margaret Dennis Edsger Frances John Radia Donald Hedy Tim Katherine
LAST      = Lovelace Turing Hopper Torvalds Liskov Thompson Hamilton Ritchie Dijkstra Allen Backus Perlman Knuth Lamarr Berners Johnson
CITIES    = Amsterdam Berlin Cairo Dakar Lima Oslo Seoul Tokyo Toronto Sydney Nairobi Madrid
COUNTRIES = Netherlands Germany Egypt Senegal Peru Norway Korea Japan Canada Australia Kenya Spain
DEPTS     = Engineering Sales Marketing Finance Support Legal Design Operations
STATUS    = active paused pending closed
```

Columns (12, in order): ID, Name, Email, City, Country, Department, Status, Amount (`$1234.56`,
right-aligned), Quantity, Score (2 decimals), Active (`yes`/`no`), Created.
Header click sorts (asc → desc → none; Shift adds a key). A filter input above the grid filters
the **Name** column by case-insensitive substring. Multi-row selection (click, Ctrl/Cmd+click,
Shift+click). A status line shows "N rows · M selected". The data is generated when the grid
page is first opened, not at startup.

Self-check: row 0 must be identical in both apps (both print it with `--print-row0`).

## Dashboard: the simulated feed

`mulberry32` seed `1234`. Each tick, in order: `users = 1000 + r() % 500`;
`revenue += (r() % 1000) / 10` (starts at 0); `latency = 20 + r() % 80` (ms);
`errors = (r() % 500) / 100` (%); throughput point `r() % 1000` appended to a 60-point window
(oldest dropped); then 12 region bars, `r() % 100` each (regions: `R1`..`R12`).
At startup run 60 ticks synchronously to fill the window, then tick every 1000 ms.
Four stat cards (Active users, Revenue `$…`, p50 latency `… ms`, Error rate `…%`), a line chart
of throughput (60 points) and a bar chart of the 12 regions. The feed runs only while the
dashboard is the current route.

## Document

`shared/document.md` (2,000 lines, 160+ fenced code blocks in rust/typescript/python) rendered
with syntax highlighting in a scrollable pane.

## Chat

History: the 200 messages of `shared/chat.json` (`history`). Composer at the bottom; sending a
message appends it and streams `reply` as the assistant's answer: split into tokens with
"split after each space" (`"a b c"` → `"a "`, `"b "`, `"c"`), one token every 20 ms (50 tokens/s),
rendered as Markdown while streaming. Stop button cancels.

## Form

15 fields; validate on submit, then live on every change; errors shown under the field; submit
shows a success notification when valid:

| # | field | rule |
|---|---|---|
| 1 | First name | required |
| 2 | Last name | required |
| 3 | Email | required, `^[^@\s]+@[^@\s]+\.[^@\s]+$` |
| 4 | Phone | optional; if set `^[+0-9 ()-]{7,20}$` |
| 5 | Company | optional |
| 6 | Job title | optional |
| 7 | Website | optional; if set starts with `http://` or `https://` |
| 8 | Age | required integer 18–120 |
| 9 | Country | required (select of the 12 COUNTRIES) |
| 10 | City | required |
| 11 | Postal code | required, `^[A-Za-z0-9 -]{3,10}$` |
| 12 | Password | ≥ 8 chars, at least one letter and one digit |
| 13 | Confirm password | equals Password |
| 14 | Bio | optional, ≤ 500 chars (textarea) |
| 15 | Accept terms | must be checked |

## Bench mode

`--bench <scenario>` (or env `COMPARE_BENCH=<scenario>`) runs a scripted scenario, prints
exactly one line `BENCH_JSON {…}` to **stdout**, and exits with status 0 (non-zero on failure).
Progress markers `BENCH_PHASE <name>` may be printed before it. No user input is needed.

```json
{
  "app": "guiraffe" | "electron",
  "scenario": "grid-fling",
  "first_frame_epoch_ms": 1731877000123.4,
  "process_start_epoch_ms": 1731877000000.0,
  "first_frame_since_process_start_ms": 123.4,
  "frames_ms": [16.6, 16.7],
  "timings_ms": { "sort_ms": [12.0, 11.5] },
  "info": { "rows": 100000 }
}
```

- `first_frame_epoch_ms`: wall clock (Unix epoch ms) when the first frame of the window was
  presented. GUIraffe: an `on_next_frame` callback registered when the window is built (runs right
  after the first frame is drawn). Electron: the window is created `show: false`; on
  `ready-to-show` main shows it and tells the renderer, which waits two `requestAnimationFrame`s
  and reports `performance.timeOrigin + performance.now()`.
- `process_start_epoch_ms`: from `/proc/self/stat` field 22 (start time, clock ticks since boot,
  `CLK_TCK` = 100) plus `btime` from `/proc/stat`; 10 ms resolution. For Electron this is the
  **main** process. The harness also records its own launch timestamp and prefers that.
- `frames_ms`: intervals between consecutive presented frames during the measured phase, from a
  continuous frame-callback chain (GUIraffe: `Window::on_next_frame` re-armed each frame; Electron:
  `requestAnimationFrame` re-armed each frame, sent to main over IPC at the end).
- "settled": time from issuing an action to the first frame presented after its result is
  visible (GUIraffe: after the grid reports not busy and the view re-rendered; Electron: after
  React committed, i.e. two rAFs after the state update).

Scenarios (each starts from a fresh launch):

| scenario | what it does | output |
|---|---|---|
| `startup` | exit right after the first frame | first-frame fields |
| `idle` | open Form (static page), stay `BENCH_IDLE_SECS` (default 95) s, exit | — |
| `grid-fling` | open grid, wait for data + 500 ms, then scroll one viewport page per frame, 200 pages | `frames_ms`, `timings_ms.total_ms` |
| `sort` | open grid, settle; 5× (sort Amount desc → settled; clear sort → settled) | `sort_ms[5]`, `unsort_ms[5]` |
| `filter` | open grid, settle; 5× (Name filter "ada" → settled; clear → settled) | `filter_ms[5]`, `clear_ms[5]` |
| `theme` | on dashboard, 10 theme toggles, each → settled | `theme_ms[10]` |
| `chat` | open chat, settle 500 ms, send "Explain the render pipeline.", record frames for 10 s while the reply streams | `frames_ms`, `info.tokens` |
| `resize` | 120 frames, each frame set window width `1000 + 280·|sin(k/10)|`, height 800 | `frames_ms` |
| `load` | `BENCH_PHASE grid`: fling the grid (wrapping) for 10 s; `BENCH_PHASE chat`: stream chat 10 s | `frames_ms` |

`--print-row0` prints row 0 of the grid data as JSON and exits (generator parity check).
