# GUIraffe performance limits

Pending the foreman's quiet-window run. The runner writes the measured table to
this file by default.

```sh
CARGO_BUILD_JOBS=4 cargo build --release -p guiraffe-bench --example soak --example limits
target/release/examples/soak mixed --short
target/release/examples/soak mixed
target/release/examples/limits
```

The mixed run is 30 minutes of replay after setup. The first 10-second interval
establishes the RSS baseline. Short mode uses two intervals and applies the
same 50 ms frame and 2% RSS drift thresholds. The runner keeps every view in
one window and rotates the visible workload. Per-component timings include tab
switches, background work drained by the deterministic dispatcher, and the
continuous chart feed. The headless window measures CPU work through paint.

Every report includes process RSS/PSS and component state counters, including
chart retention, the grid pool, Markdown source bytes, Chat conversation
growth, and outstanding overlays. Chat intentionally retains streamed replies.
The state counters help interpret a drift failure. They do not measure each
component's private heap. Markdown resets to its seed every 256 chunks, chart
retains 10,000 points, and the sample buffers hold one interval of timings.

For a scaled smoke run, set `SOAK_SCALE=1000` on the mixed short run and use
`target/release/examples/limits --start 16 --max 32 --output <smoke-report.md>`.
Limits uses fresh child processes, a small warm-up followed by 120 replay
samples, and Linux process peak RSS. The default cap is 16,777,216 size units.
A cap or probe error leaves the upper limit unmeasured. Use a separate output
path for indicative runs.

## Shared-machine smoke evidence

These release-mode runs exercised the harness. The full thresholds are
pending quiet-window measurement.

| Check | Before this change | After this change |
|---|---|---|
| Mixed short, 1,000× smaller fixtures | No mixed runner | Both intervals passed. Peak frame 28.68 ms. RSS 33,632 → 34,028 KiB (+1.18%). |
| Mixed short, default fixture sizes | No mixed runner | Both intervals reported. Markdown-active frame 52.51 ms triggered the long-frame failure. RSS fell from 88,968 to 86,320 KiB. |
| Isolated limits, start 16 and cap 32 | No geometric sweep | All 19 scenarios produced results. Four failed the p99 budget and the others reached the smoke cap. |
| Instruction counts and repaint allocations | Existing component gates | Widget implementations were outside this change. No new comparison was collected. |

The scaled 360-frame baseline Flow profile attributed 17.18% of self samples
to an unsymbolized function, 12.24% to path vertex cloning, and 10.35% to scaled
path vertex collection. This profile measures the standalone baseline. The
mixed run includes different work, so the percentages do not form a before
and after comparison.
