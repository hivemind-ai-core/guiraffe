# Production readiness

## Charts

The baseline is release/0.2 at `65e854a`. The mounted quiet-machine results are in [PERF-0.2.md](plans/PERF-0.2.md). The table below uses the same chart data in a renderer-neutral Rust probe on a busy machine. Each run compiles a scene for a 1280 × 760 viewport, then picks against the source points. Timing and allocation counts cover 300 scene rebuilds and picks. Callgrind counts instructions across three such frames at 100k points, excluding model preparation.

| Workload | Peak process RSS | Scene and pick p99 | Allocation calls per scene and pick | Callgrind instructions |
|---|---:|---:|---:|---:|
| Line, 10M points | 1,893.5 → 216.4 MiB | 26.49 → 7.82 ms | 92.1 → 92.1 | 11.41M → 11.05M |
| Scatter, 1M points across 100 series | 192.7 → 27.8 MiB | 85.06 → 9.00 ms | 182.1 → 277.1 | 44.22M → 8.11M |

Samply recorded 1,149 → 229 filtered frame samples for line and 5,789 → 599 for scatter. In line, the previous nearest and anchor functions accounted for 41.9% and 7.1% of self samples. Indexed picking accounted for 24.5% after the change. In scatter, generation accounted for 59.4% → 51.8% and coordinate projection for 35.4% → 9.9%. The mounted baseline also attributed 27.8% of scatter self samples to GPUI's bounds tree. The renderer-neutral probe excludes GPUI painting.

### Storage and reuse

| Structure | Bound and invalidation |
|---|---|
| Prepared line, point, and bubble coordinates | Two `f64` values per accepted datum. Each run stores an ID origin and stride when indices and keys are regular. Irregular IDs add up to one `(usize, u64)` pair per point. Color, size, and text columns allocate only when a datum uses a nondefault value. |
| Line LOD | Successive levels retain roughly one in eight indices. Levels share the original coordinates and are replaced with the model. |
| Exact picking | A min/max-y hierarchy stores two `f64` values per node with one leaf per 256 source points. Nodes refer to sorted x ranges and require no separate point permutation. Picking preserves last-wins ties. Overlapping bounds can still force traversal of every candidate. |
| Dense scatter | Each panel divides 8,192 candidate projections among its visible scatter runs, with at least one candidate per run. Each run retains one representative per occupied screen cell on a grid capped at 128 × 64. Series can overlap in the same cell. Consecutive representatives with the same series and color share one paint path. |
| Recent scene | One previous x domain per chart, capped at 64 panels, 8,192 draw records, 16,384 geometry entries and 64 KiB of panel and tick labels and tick metadata. A size, model or visibility change clears both scenes. |
| Streaming publication | The view retains the current model while one current spec request prepares a replacement. New requests cancel older work, and obsolete results cannot publish. The preparation still rebuilds the supplied rolling window. |

The chart tests cover irregular ID and style preservation, storage capacity, indexed versus linear picks, constant dense scatter candidate count at 10k through 10M points, reuse of the previous scene, and superseded 500-point uploads. Mounted RSS, paint behavior, interaction timing, and warm repaint allocations require separate measurements. Scene compilation still allocates on viewport cache misses. Dense scatter samples each series within the viewport, so its screen-cell bound applies per series.

## Shared GPUI repaint storage

| Storage | Bound and invalidation |
|---|---|
| Tab-stop order, element states and access logs | **OPEN high-water capacity.** Each of the two alternating frames can retain storage proportional to its historical peak. Clearing a frame drops the entries. See the ignored `alternating_frames_retain_peak_storage` probe in [frame_capacity.rs](bench/tests/frame_capacity.rs). |
| Taffy layout scratch | Up to 768 KiB per layout tree, plus buffer headers. Three pools each retain up to sixteen buffers of at most 16 KiB. Returning a buffer clears its contents. |
| Upstream stable scene sort | Up to 65,536 sort entries per scene, or 1 MiB on 64-bit systems. The two window scenes reuse their own scratch. A larger primitive batch uses the standard stable sort for that frame. |
| Upstream Kit root | One cache record per window root for the preceding viewport render. View invalidation, refresh, and changes to the viewport or render context rebuild the record. An active accessibility tree uses live root prepaint. |
| Key contexts | One shared entry buffer per live context value and one shared empty value per process. Mutations detach and released dispatch frames drop their references. |
| Fast rounded-clip scene reuse | Uses the two existing frame scenes. A complete replay of the preceding scene with matching layer metadata can transfer the compiled scene to the new frame. |

Mounted allocation measurements and validation limits are recorded in the
[warm repaint audit](bench/measurements/warm-repaint.md).


### Frame storage retention probe

`alternating_frames_retain_peak_storage` draws two frames at each workload
size, then measures both frame buffers at 100 keyed focusable elements.
Vectors report capacity bytes. Map figures are usable entry capacity times
entry size and exclude hash-table control bytes and slack. Neither figure
includes retained element payloads or the renderer's other storage.

| Backend | Prior peak | Frame | Element-state map | Access log | Tab history | Tab lookup | Tab order |
|---|---:|---|---:|---:|---:|---:|---:|
| Upstream | 10,000 | Both, each | 1,032,192 | 1,310,720 | 524,288 | 1,146,880 | 1,179,648 |
| Upstream | 100,000 | Rendered | 7,956,288 | 20,971,520 | 4,194,304 | 9,175,040 | 9,437,184 |
| Upstream | 100,000 | Next | 8,257,536 | 20,971,520 | 4,194,304 | 9,175,040 | 9,437,184 |

### Proposed idle trim for frame storage

Arm one cancellable maintenance job after both alternating frames have reported
at most one quarter of retained capacity for each candidate buffer throughout
a sustained low-water interval. Recheck at execution and shrink each qualifying
buffer toward twice its live high-water over that interval, subject to a small
floor. An intervening larger frame cancels that job. If the window stays idle it
performs no recurring work. Trim in the maintenance callback outside draw, with
unchanged-frame allocation and deallocation gates on both backends. Until a
trim runs, each buffer can retain its full historical peak. The current
implementation retains the peak indefinitely.
