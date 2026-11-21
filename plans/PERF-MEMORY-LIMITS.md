# PERF-0.2 memory limits: raw data size and quiet-window RSS per scenario

Foreman side task. For every scenario in `bench/examples/soak.rs` and `bench/examples/production_soak.rs`
this document computes the **raw data size** the user hands the component, derives the **RSS limit**
`1.25 x raw + 64 MiB` from it, and compares that limit with the **quiet-window RSS** measured on
`release/0.2` (`6ef0ad6`, upstream and GPUI-Fast backends).

Budget rule (from `plans/PERF-0.2.md`): *peak RSS at scale <= 1.25 x raw data + 64 MiB*.

## Scope

The per-component scale scenarios of `bench/examples/soak.rs` and `bench/examples/production_soak.rs`:
each component at its spec/23 scale target, one per row.

- `soak empty` is the floor probe (no dataset) and sets the harness floor, not a budget.
- `soak code-annotations` (spec/25) was merged in `release/0.2` `f71d552`; it is listed with its raw
  size and limit, and is not yet measured because it postdates the quiet window.
- The limits probes add `soak mixed` and move `grid`/`code` into the soak binary. `mixed` is a composite
  stress scenario - it rotates flow (5,000 nodes), gantt (10,000 tasks), tree (10,000 branches), property
  grid (1,000 groups), grid and code (1,000,000 rows), markdown (5,000 lines), chat (1,000 messages),
  chart (1,000 points) and timeline (1,000 keys) through one window - so it has no single raw-data
  budget and is not in this table. The `grid` and `code` rows cover both binaries, because the datasets
  are identical.

> Revision 3 (2026-11-21 11:40): the limit is now evaluated the way the harness enforces it - against
> the **peak RSS increase over the empty window**, not against absolute process RSS (see `production_soak.rs`,
> nightly `code` check). Adds `code-annotations` and the `mixed` scope note.
> Revision 2 (2026-11-21 09:40) replaced the 01:19 baseline RSS column with the quiet-window
> measurement and subtracted the empty-window floor. Revision 1 used the 01:19 baseline table.

## Method

**Raw data size** = the bytes of the source data the user hands the component, counted per record as
*declared scalar field widths + UTF-8 bytes of user-supplied text*:

| element | counted as |
|---|---|
| scalar field | declared width (`u64` 8, `u32` 4, `usize` 8, `f64` 8, `f32` 4, `bool` 1, unit enum 1) |
| user text | UTF-8 byte length of the string the user supplies (labels, message text, source text) |
| container header | not counted (`String`/`Vec`/`Option`/`Arc<str>` headers are component overhead, not user data) |
| shared/interned payload | counted once per distinct payload (styles and payloads are interned, so the text is stored once) |
| component-internal bookkeeping | not counted (generated ids, decor/derived caches, layout caches) |

Text byte counts are exact, not estimated: the deterministic generators of `soak.rs` (`document()`,
the Mermaid source builder, the chat message templates) were executed and their `len()` measured, so
`markdown`, `mermaid` and `chat` carry exact byte counts.

**RSS limit** = `1.25 x raw_bytes + 64 MiB`.

**What the limit is compared against**: the harness enforces the rule on the **peak RSS increase over
the empty window** (`production_soak.rs`, nightly `code` check: `peak_delta <= 1.25 x raw + 64 MiB`).
This document therefore compares the same quantity: `RSS - empty-window RSS` against the limit.
Absolute process RSS is listed next to it, since it is what an operator sees.

**Harness floor**: an empty window costs 18.0 MiB (upstream) / 19.7 MiB (fast) in `soak`
(`soak empty`), and 98.0 MiB (upstream) / 99.9 MiB (fast) in `production_soak`
(`empty_window_rss_kib`, the headless renderer plus assets).

## Raw data size and limit per scenario

| scenario (scale) | dataset | raw bytes | arithmetic | raw MiB | limit (1.25x raw + 64 MiB) |
|---|---|---|---|---|---|
| grid (production_soak) | 10,000,000 rows x 50 i64 columns (virtual `FnSource`) | 4,000,000,000 | `10,000,000x50x8` | 3814.697 | 4832.4 MiB |
| flamegraph (10M frames) | 10,000,000 frames (id u64, parent Option<u64>, self_value f64, total f64, label, package) | 501,250,000 | `10,000,000x50.125` | 478.029 | 661.5 MiB |
| chart-line (10M points) | 10,000,000 points, x/y as f64 | 160,000,000 | `10,000,000x16` | 152.588 | 254.7 MiB |
| tree (1M nodes, 100k expanded) | 1,000,001 nodes (id u64 + label) + 100,000 branch badges | 14,850,000 | `1,000,000x14.75 + 100,000x1` | 14.162 | 81.7 MiB |
| property-grid (1M rows) | 1 root + 100,000 group rows + 900,000 property rows (id, label, value, default) | 22,405,036 | `8x1,000,001 ids + labels + values + defaults + 20 choice bytes` | 21.367 | 90.7 MiB |
| code-annotations (1M lines) | 1,000,000-line document + 176,001 annotations + 50,000 gutter marks + 2,000 blame commits | 39,402,096 | `30,000,000 doc + 176,001x41 ann + 55 payload + 50,000x40 gutter + 2,000x93 blame` | 37.577 | 111.0 MiB |
| chart-scatter (1M points) | 1,000,000 points, x/y as f64 | 16,000,000 | `1,000,000x16` | 15.259 | 83.1 MiB |
| code (production_soak) | 20,000,000-byte source document | 20,000,000 | `2x10,000,000 - 1` | 19.073 | 87.8 MiB |
| chat (100k messages + streaming) | 100,000 messages (text + MessageId u64 + Role) | 10,317,780 | `9,417,780 text + 100,000x9` | 9.840 | 76.3 MiB |
| curve-editor (100k keys) | 100,000 keys (t f64, v f64, interp, in/out handles) | 4,900,000 | `100,000x49` | 4.673 | 69.8 MiB |
| flow (50k nodes / 100k edges) | 50,000 nodes (id u64 + pos 2x f32 + label), 100,000 edges (id + source + target) | 3,688,890 | `50,000x16 + 488,890 labels + 100,000x24` | 3.518 | 68.4 MiB |
| gantt-chain (100k tasks) | 100,000 tasks, chain dependencies (99,999 ids) | 3,388,886 | `100,000x20 + 988,890 names + 399,996 dep ids` | 3.232 | 68.0 MiB |
| gantt-fan-out (100k tasks) | 100,000 tasks, fan-out dependencies (99,999 ids) | 3,388,886 | `100,000x20 + 988,890 names + 399,996 dep ids` | 3.232 | 68.0 MiB |
| gantt (100k tasks, sparse deps) | 100,000 tasks (id u32 + not_before f64 + duration f64 + name + after[]) | 3,348,890 | `100,000x20 + 988,890 names + 360,000 dep ids` | 3.194 | 68.0 MiB |
| tracks (100 tracks x 1000 clips) | 100,000 clips (start f64 + len f64) + 100 track names | 1,600,800 | `100x1,000x16 + 100x8` | 1.527 | 65.9 MiB |
| markdown (50k lines + streaming) | 50,002-line GFM document (1,761,032 B) plus streamed chunks | 1,761,032 | `document(50_000).len()` | 1.679 | 66.1 MiB |
| timeline / dope sheet (100x1000 keys) | 100 channels (name + 1,000 f64 keys each) | 803,390 | `100x1,000x8 + 3,390 name bytes` | 0.766 | 65.0 MiB |
| bar-editor (100k bars) | 100,000 f64 bar values | 800,000 | `100,000x8` | 0.763 | 65.0 MiB |
| chart-stream (10k pts/s, 10k window) | retained window of 10,000 points (x/y f64); source is 10,000 u32 | 160,000 | `10,000x16` | 0.153 | 64.2 MiB |
| mermaid (500 nodes) | flowchart source for 500 nodes + edges | 16,654 | `source.len()` | 0.016 | 64.0 MiB |
| empty (floor probe) | no dataset | 0 | `0` | 0.000 | 64.0 MiB |

Dataset changes since revision 1: `property-grid` is now 100,000 groups x 9 properties
(1,000,001 rows, was 10,000 x 10 = 110,001 rows), and `gantt` was split into three dependency shapes
(`gantt` sparse, `gantt-fan-out`, `gantt-chain`).

## Quiet-window memory vs the limit

Quiet window 2026-11-21 08:12-08:25, release build, `soak <x> --short` / `production_soak`,
upstream and GPUI-Fast backends. `delta` is the peak RSS minus the empty-window floor, which is what
the limit applies to; `RSS` is the absolute process RSS.

| scenario | delta upstream | delta fast | RSS upstream | RSS fast | limit | verdict |
|---|---|---|---|---|---|---|
| markdown | 139.4 MiB | 233.9 MiB | 157.4 MiB | 253.7 MiB | 66.1 MiB | **OVER +167.8 MiB** |
| chat | 106.0 MiB | 116.4 MiB | 124.0 MiB | 136.1 MiB | 76.3 MiB | **OVER +40.1 MiB** |
| flow | 73.6 MiB | 93.0 MiB | 91.7 MiB | 112.8 MiB | 68.4 MiB | **OVER +24.6 MiB** |
| gantt-fan-out | 32.3 MiB | 54.0 MiB | 50.4 MiB | 73.7 MiB | 68.0 MiB | OK |
| gantt-chain | 31.0 MiB | 36.5 MiB | 49.0 MiB | 56.2 MiB | 68.0 MiB | OK |
| gantt | 30.3 MiB | 35.7 MiB | 48.3 MiB | 55.5 MiB | 68.0 MiB | OK |
| tree | 44.4 MiB | 46.1 MiB | 62.5 MiB | 65.8 MiB | 81.7 MiB | OK |
| curve-editor | 8.3 MiB | 27.1 MiB | 26.3 MiB | 46.9 MiB | 69.8 MiB | OK |
| chart-stream | 3.5 MiB | 21.3 MiB | 21.6 MiB | 41.1 MiB | 64.2 MiB | OK |
| property-grid | 45.6 MiB | 47.3 MiB | 63.7 MiB | 67.0 MiB | 90.7 MiB | OK |
| timeline | 15.1 MiB | 21.2 MiB | 33.2 MiB | 40.9 MiB | 65.0 MiB | OK |
| mermaid | 15.5 MiB | 16.0 MiB | 33.5 MiB | 35.8 MiB | 64.0 MiB | OK (memory; mount is the gap) |
| code | 37.6 MiB | 37.7 MiB | 135.3 MiB | 137.6 MiB | 87.8 MiB | OK |
| chart-scatter | 24.2 MiB | 25.5 MiB | 42.3 MiB | 45.2 MiB | 83.1 MiB | OK |
| tracks | 5.8 MiB | 7.4 MiB | 23.9 MiB | 27.2 MiB | 65.9 MiB | OK |
| bar-editor | 2.8 MiB | 3.2 MiB | 20.8 MiB | 22.9 MiB | 65.0 MiB | OK |
| chart-line | 170.4 MiB | 180.7 MiB | 188.4 MiB | 200.4 MiB | 254.7 MiB | OK |
| code-annotations | pending | pending | - | - | 111.0 MiB | not yet measured |
| flamegraph | 40.6 MiB | 42.5 MiB | 58.6 MiB | 62.3 MiB | 661.5 MiB | OK |
| grid | 10.3 MiB | 12.7 MiB | 108.4 MiB | 112.6 MiB | 4832.4 MiB | OK (virtual source) |

**3 of the 19 measured scenarios are over the memory budget** (markdown, chat, flow); 16 are inside
it, and `code-annotations` still has to be measured. Every scenario the 01:19 baseline called a memory
failure (gantt 6.66 GiB, chat 4.49 GiB, tracks 721 MiB, markdown 799 MiB, flamegraph 729 MiB, code
345 MiB, chart-line 1.82 GiB, chart-scatter 219 MiB, flow 316-412 MiB) is now inside the limit except
chat, markdown and flow.

## Fix targets (still over budget)

| scenario | delta upstream / fast | limit | must shed (up / fast) | note |
|---|---|---|---|---|
| markdown | 139.4 / 233.9 MiB | 66.1 MiB | 73.3 / 167.8 MiB | 139.4 MiB of it is data-dependent for a 1.68 MiB document; the Fast backend also drifted (211,852 -> 259,780 KiB in the window) |
| chat | 106.0 / 116.4 MiB | 76.3 MiB | 29.7 / 40.1 MiB | 106.0 MiB data-dependent for 9.84 MiB of messages |
| flow | 73.6 / 93.0 MiB | 68.4 MiB | 5.2 / 24.6 MiB | 73.6 MiB data-dependent for 3.52 MiB of nodes and edges; the old 316 -> 412 MiB drift is gone |

## Component view (delta vs the 1.25x data allowance)

How much memory the component keeps for the data it was handed. Scenarios with a tiny raw size are
dominated by their fixed per-view cost, so the 64 MiB allowance - not the 1.25x factor - is what
matters for them.

| scenario | delta (up) | 1.25x raw | ratio | over allowance |
|---|---|---|---|---|
| mermaid | 15.5 MiB | 0.0 MiB | 782x | 15.5 MiB (fixed cost, 16 KiB of source) |
| markdown | 139.4 MiB | 2.1 MiB | 66x | 137.3 MiB |
| chart-stream | 3.5 MiB | 0.2 MiB | 18.5x | 3.3 MiB |
| timeline | 15.1 MiB | 1.0 MiB | 15.8x | 14.2 MiB |
| flow | 73.6 MiB | 4.4 MiB | 16.7x | 69.2 MiB |
| chat | 106.0 MiB | 12.3 MiB | 8.6x | 93.7 MiB |
| gantt-fan-out | 32.3 MiB | 4.0 MiB | 8.0x | 28.3 MiB |
| gantt-chain | 31.0 MiB | 4.0 MiB | 7.7x | 27.0 MiB |
| gantt | 30.3 MiB | 4.0 MiB | 7.6x | 26.3 MiB |
| tracks | 5.8 MiB | 1.9 MiB | 3.1x | 3.9 MiB |
| bar-editor | 2.8 MiB | 1.0 MiB | 2.9x | 1.9 MiB |
| tree | 44.4 MiB | 17.7 MiB | 2.5x | 26.7 MiB |
| property-grid | 45.6 MiB | 26.7 MiB | 1.7x | 18.9 MiB |
| code | 37.6 MiB | 23.8 MiB | 1.6x | 13.8 MiB |
| curve-editor | 8.3 MiB | 5.8 MiB | 1.4x | 2.4 MiB |
| chart-scatter | 24.2 MiB | 19.1 MiB | 1.3x | 5.2 MiB |
| chart-line | 170.4 MiB | 190.7 MiB | 0.9x | inside |
| flamegraph | 40.6 MiB | 597.5 MiB | 0.07x | inside |
| grid | 10.3 MiB | 4768.4 MiB | 0.00x | inside (virtual source) |

The remaining "over allowance" rows are dominated by fixed per-view cost rather than data volume:
`gantt` keeps 30 MiB for 3.2 MiB of tasks, `timeline` 15 MiB for 0.77 MiB of keys. Those are the
costs the 64 MiB allowance exists to cover; the ones worth attacking are `markdown` (139 MiB for a
1.7 MiB document) and `chat` (106 MiB for 9.8 MiB of messages).

## Per-scenario notes

- **flow** - `Graph::from_parts` takes 50,000 `Node`s (id `u64`, `Point` = 2 x `f32`, label) and 100,000
  `Edge`s (id + source + target node ids; the `"out"`/`"in"` endpoint sides are component defaults and are
  not counted). Node labels are the exact sum of `format!("Node {i}")` lengths: 488,890 B.
  The old 316 -> 412 MiB drift is gone in the quiet window (91.7 MiB upstream, no drift reported).
- **timeline / dope sheet** - 100 channels, each `Channel::new(name, keys)` with 1,000 `f64` keys; the
  `decor` vector the channel allocates per key is component-internal and excluded.
- **chart-line / chart-scatter** - one point = 16 B (`x`,`y` as `f64`). `chart-line` carries a single
  series, `chart-scatter` 100 groups (`i % 100`) with no extra label bytes; the chart title
  ("Observations · عربي 日本語 🎵", 39 B) is negligible. `chart-line` is now inside its limit
  (170.4 MiB delta against a 254.7 MiB limit) even though its data-dependent memory is close to its
  own data size.
- **chart-stream** - the source is 10,000 `u32` values (40,000 B) that the component expands to `f64`
  pairs; the resident window is `retain_last(10_000)`, i.e. 10,000 points = 160,000 B. Counted at 16 B
  per point for consistency with the other chart scenarios.
- **gantt** - 100,000 `Task`s: `id` `u32` + `not_before` `f64` + `duration` `f64` = 20 B fixed, plus
  `format!("Task {id}")` names (988,890 B exact) plus `after` dependency ids. The sparse shape links
  90,000 tasks (id % 10 != 0), the fan-out shape 99,999 tasks (all but the root) to task 0, the chain
  shape 99,999 tasks to their predecessor. Same tasks, different dependency density.
- **tracks** - 100,000 clips; the user supplies `start` and `len` (16 B). The stored `Clip` is 28 B
  (`id` u32 + `start` + `len` + `source_offset`); `id` and `source_offset` are component-managed and
  excluded, which is why this scenario is generous to the implementation.
- **curve-editor** - `Key` fields `t` 8 + `v` 8 + `interp` 1 + `in_handle` 16 + `out_handle` 16 = 49 B.
- **bar-editor** - `BarCurve` holds one `f64` per bar: 800,000 B.
- **flamegraph** - per frame: `id` 8 + `parent` `Option<u64>` 16 + `self_value` 8 + `total` 8 +
  `label` (mean 5.125 B over the 8 label words) + `package` "bench" 5 = 50.125 B. 10,000,000 frames =
  478 MiB, so this scenario is the only one whose raw data is itself near the limit; the component uses
  40.6 MiB against a 597.5 MiB data allowance.
- **tree** - 1,000,001 nodes (root + 100,000 branches + 900,000 leaves), counted as 1,000,000 records
  (root included); labels cycle over 8 file names (mean 6.75 B). Branch badges ("9") add 100,000 B.
  The `TreeSource` is a virtual source: ids are generated on demand, only labels are stored.
- **property-grid** - 1,000,001 rows: root ("Settings"), 100,000 group rows ("Group"), 900,000 property
  rows with a value and a default (the probe grew from 10,000 x 10 to 100,000 x 9). Values cycle
  Number (8 B) / Bool (1 B) / Text "Layer" (5 B) / Enum "Normal" (6 B); defaults differ only for every
  fifth row. The three choice strings ("Normal", "Multiply", "Screen") are shared by all rows and
  counted once (20 B).
- **code-annotations** - the document is `"let value = source(argument);\n".repeat(1_000_000)` =
  30,000,000 B (30 B per line). The decorations the user adds: 176,001 annotations
  (100,000 highlight "reference", 50,000 highlight "search match", 20,000 underline "diagnostic",
  5,000 bubble "review comment", 1,000 inline hint "type", 1 symbol "symbol") at 41 B each
  (`id` u64 8 + `SourceRange` 32 + `kind` 1) plus 55 B of payload text, 50,000 gutter marks at 40 B each
  (`id` + range) and 2,000 blame commits at 93 B each (`age_days` 4 + author 11 + short hash 7 +
  message 21 + date 10 + `id` 8 + range 32). Styles and payloads are interned, so their text is counted
  once. Not yet measured - it was merged after the quiet window.
- **markdown** - the source document is exactly 1,761,032 B (50,002 lines); the probe additionally
  streams chunks every fourth frame (8,958 B over the first 1,024 frames). 139.4 MiB of
  data-dependent memory for a 1.7 MiB document is the largest data-independent cost in the programme.
  The Fast backend drifted inside the window (211,852 -> 259,780 KiB, `drifted=true`).
- **mermaid** - the source is exactly 16,654 B; the diagram is 500 nodes plus edges. Memory is inside
  the limit (15.5 MiB delta); the open gap is the 2.8 s mount and the 15.6 ms zoom spike, not memory.
- **chat** - 100,000 messages, exact text total 9,417,780 B, plus `MessageId` (8 B) and `Role` (1 B)
  per message. 106.0 MiB of data-dependent memory for 9.8 MiB of messages (8.6x its allowance).
- **grid** - `FnSource` computes cells on demand, so the "raw data" is the logical dataset
  (10,000,000 rows x 50 `i64` columns = 4.0 GB) while the process only holds the materialised window
  (10.3 MiB above the 98.0 MiB floor). The limit is not binding; it should not be read as a memory
  target for the grid.
- **code** - `Arc::<str>::from("x\n".repeat(ROWS - 1) + "x")` = exactly 20,000,000 B, matching the
  "raw 20 MB" note in `plans/PERF-0.2.md`. The stored line index and shaping caches are what push the
  component to 37.6 MiB above the floor, 1.6x its 23.8 MiB allowance; the rest of the 135.3 MiB is the
  `production_soak` renderer window itself, which is why the rule has to be read on the delta.

## Caveats

- RSS is process RSS (`smaps_rollup`), so it includes the binary, the stack, GPUI/Kit allocations and
  any retained GPU-side buffers, not only the dataset.
- The rule is applied to the RSS increase over the empty window. For `production_soak` that floor is
  98.0 MiB (headless renderer plus assets), so the absolute RSS of `code` (135.3 MiB) exceeds its
  87.8 MiB limit while the delta (37.6 MiB) does not - the same reading the nightly check uses.
- The Fast backend is typically 2-12 MiB above the upstream backend. The outliers are markdown
  (96.3 MiB, the drift inside the window), gantt-fan-out (23.4 MiB), flow (21.1 MiB), curve-editor
  (20.6 MiB) and chart-stream (19.6 MiB); the limit is evaluated against the worse backend.
- Raw sizes exclude component-internal structures (layout caches, shaped-glyph caches, scene graphs,
  `decor` vectors, generated ids). Any of these can exceed the raw data.
- Provenance: the quiet-window numbers were measured on 2026-11-21 08:12-08:25 (load 0.36) on
  `release/0.2` at `6ef0ad6`, from `perf-run/measure2.log` in the `perf-baseline` worktree. That
  worktree was removed after the run, so the values are transcribed here; the next quiet window has to
  re-measure everything, including `code-annotations`.
- If a probe changes its dataset or the merge lands further work, the raw bytes and the measured RSS
  both change and this table has to be recomputed.
