# Markdown memory

## Diagnosis before implementation

The mounted RSS fixture uses 50,000 **lines**, about 10,205 top-level blocks.
Massif used the 64-frame scroll/append child path with the existing
integration development executables. These are diagnostic snapshots from
the integration worktree, with matching parser allocation stacks across
backends. Release comparisons use this worktree's before and after builds.

| Live requested heap at the component-loaded peak | Upstream | Fast |
|---|---:|---:|
| Total | 55,205,526 B | 58,721,439 B |
| Mark-vector growth in `push_merged` | 9,307,200 B | 9,307,200 B |
| Cold inline descriptor slices | 3,526,272 B | 3,526,272 B |
| List height index | 862,208 B | 862,208 B |

Massif separately estimated about 14 MB of allocation overhead on each
backend. The figures describe requested bytes and Massif overhead, while
the RSS gate measures real process residency relative to an empty window.

- **Parsed AST.** Transient mdast trees are consumed one section at a time.
  Their stacks fall below the 1% reporting threshold at the process heap
  peak. The largest persistent parse stacks are lowered inline marks,
  source maps (about 3 MB on the source-segment stack), cold run arrays,
  and eager list/table children.
- **Source.** Markdown and TextView maintain append buffers. The published
  renderer snapshot and the initial cold runs retain contiguous strings.
  The append-time format buffer accounts for another 3,522,298 B at the
  Fast heap peak.
- **Layout and shaping.** The height index retains one entry per block.
  Materialization and derived decorations share a 256-block bound. The
  per-thread shaped-text table holds up to 4,096 entries. Shaping stacks
  were below the per-stack reporting threshold at the heap peaks.
- **Retained elements.** List paint only builds visible blocks and
  overscan. On Fast, the Taffy slot map accounts for 1,296,896 B beneath
  the carried measured-text path. Dedicated Fast retained-subtree stacks
  each fell below 1%. Their aggregate remains unquantified.
- **Highlights.** The fence shape cache is bounded by 2,048 entries and
  256 KiB. Highlight stacks were below the reporting threshold.
- **Images and Mermaid.** The fixture includes neither, so this profile
  exercises no image downloads or diagram rasters. Markdown delegates
  image retention to the GPUI image cache configured by the host, with
  GPUI's app asset cache as the fallback. Mermaid's standalone render
  cache has a 16-entry / 64 MiB bound per engine.

Profile artifacts in this worktree are `target/dev-upstream.massif` and
`target/dev-fast.massif`. Both used `--tool=massif --time-unit=B
--peak-inaccuracy=0.1 --detailed-freq=5` and
`GUIRAFFE_MOUNTED_RSS_TEST_SCENARIO=markdown`.

The exact upstream release baseline (`5ce100e`) corroborated the development
profile. Its 64-frame run peaked at 54,542,135 requested heap bytes with
14,100,177 B of estimated overhead. The same mark stack accounted for
9,307,200 B, cold descriptors for 3,526,272 B and the list index for
867,328 B. Inline-flow highlight projection had an 83,200 B stack.
The exact Fast baseline peaked at 58,377,988 requested bytes with
14,207,348 B of estimated overhead. The equivalent final peaks were
38,538,698 B upstream and 41,754,490 B on Fast. The four exact release
Massif captures are `target/release-{before,after}-{upstream,fast}.massif`.

The current fixture's raw source is 1,761,032 B. Its RSS gate computes
67,686 KiB (66.10 MiB). The requested 15 MiB of headroom therefore
requires a peak delta of at most 52,326 KiB.

## Changes guided by the profiles

The first revision stored simple whole-run marks inline, made the
fallback mark slices exact, and stopped allocating linear source maps
that can be reconstructed from the existing source ranges. Table storage
is sized to the parsed rows and known column count. The parser borrows
pending input and creates the combined append source with exact capacity.

The initial Fast release experiment had a 54,952 KiB median delta over
five 128-frame paired runs, leaving 12.4 MiB below the budget. Its
64-frame Massif peak was 44,790,152 requested bytes. The mark stack
dropped to 857,520 B and the descriptor slices to 2,644,704 B. The profile
still showed 2,252,280 B allocated by inline-node construction. The next
revision removes per-run selection allocations for plain text, preserves
them for images and custom nodes, builds the persistent block index
directly, and reduces list overscan to 512 px for estimated-height views.

## Release replay

Five alternating empty/Markdown processes per row used
`SOAK_LIMIT_PROBE=markdown` for 128 frames, excluding the first eight
frames from p99. “Before” is `5ce100e`. “After” includes both changes
above. Each timing column is the median of five loaded runs. These
diagnostics ran on the shared machine.

| Backend | Peak RSS delta before → after | Headroom after | First useful frame before → after | p99 before → after |
|---|---:|---:|---:|---:|
| Upstream | 61,804 → 44,256 KiB | 22.88 MiB | 60.20 → 32.85 ms | 4.36 → 3.01 ms |
| Fast | 68,248 → 51,448 KiB | 15.86 MiB | 35.72 → 55.64 ms | 5.50 → 7.70 ms |

The Fast RSS batches ran at different points in the build. A subsequent
three-pair alternating timing replay brought the two binaries under
similar load. Its medians were 34.74 → 36.75 ms for first useful frame
and 5.68 → 5.47 ms p99. The final Fast 64-frame Massif profile measured
41,754,490 requested heap bytes. The 2.25 MB inline-node construction
stack dropped below the 0.1% reporting threshold.

The mounted 64-frame debug RSS gates passed at 47,688 KiB upstream
and 48,564 KiB Fast. On both backends, the unchanged draw allocated zero
requests and zero bytes at both 5,000 and 50,000 lines. The 48-byte storage gate checks that a source-backed, whole-run
emphasis descriptor has no separate mark/map allocation, and round-trips
the source mapping at a nonzero base.

The short soak measured two 10-second intervals on the shared machine.

| Backend | Before mount | After mount | Before p99 | After p99 |
|---|---:|---:|---:|---:|
| Upstream | 74.25 ms | 33.07 ms | 8.38–8.51 ms | 3.22–4.48 ms |
| Fast, back-to-back comparison | 35.93 ms | 34.91 ms | 6.92–6.99 ms | 6.74–6.86 ms |

An earlier Fast after-run during concurrent compilation reached
8.90–9.33 ms p99. The final short comparisons stayed within the RSS/PSS
drift threshold after warm-up. Foreman quiet-window timing and 30-minute
drift validation remain open.

The vendored Kit upstream text suite passed all 440 tests, including the
new storage assertion and the existing visible selection, source copy
and viewport eviction checks. The workspace's upstream test phase passed.

The supplemental vendored Fast text suite passed 436 tests, including
the new storage assertion. It reproduced the same five failures as
`5ce100e` in the reveal-range give-up, two long-press, ordered-list
marker, and table-corner tests. Those tests run outside the workspace
gate. Exact failure lists are in `target/kit-tests-fast-baseline.log`
and `target/kit-tests-fast.log`.

`scripts/gate.sh` passed. Each workspace backend passed 1,620 tests
with two ignored. All 33 per-crate doctest invocations, Clippy with
warnings denied, and formatting passed. No golden image changed.
