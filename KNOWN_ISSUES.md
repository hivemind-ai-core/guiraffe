# Known issues

1. **SVG icons are blank in offscreen visual snapshots.** GPUI 0.3.12's headless wgpu renderer
   (used by `guiraffe-test::snapshot`) draws no monochrome sprites on the software adapter in CI
   and dev sandboxes. Text and shapes render correctly, so snapshots still catch layout, colour and
   typography regressions, but not icon regressions.
   - Evidence: `crates/components/tests/icon_probe.rs` (ignored).
   - Plan: check icons through the Xvfb e2e path, which uses the real platform renderer. Report the issue upstream to gpui-kit/gpui, and re-run the probe on each gpui-kit upgrade.

2. **The flow editor doesn't fit its graph to the viewport on first show.** In the gallery, the right-most node is clipped until the user clicks Fit. Owner: flow follow-ups (reviews/flow-review-2.md).
