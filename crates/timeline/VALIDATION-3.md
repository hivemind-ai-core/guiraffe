# Timeline review fixes — 2026-11-17

Branch `feat-timeline-2`, rebased onto release `9ea7d4d`. No merge or push. This report supersedes the review status and test scope in `VALIDATION-2.md`, which describes the first handoff before release integration.

Review commits after the rebased implementation:

- `4f1ca36`: reconcile release keyboard/selection/accessibility behavior with one lock-style mechanism.
- `0c61ae0`: aggregate-key display, fractional bar weighting, screen-metric aligned tangents and regressions.
- `e93b92e`: shared focus rings, pointer-focus suppression, stable drag/box coordinates, foreground value labels and bounded badges.
- `73d1538`: polished timeline scenes and all showcase/timeline goldens, including the new bar editor.

## Review disposition

1. Rebase conflicts were reconciled with release's keyboard actions, selected-item outlines, AccessKit metadata and focus behavior preserved. Default per-item `LockStyle::Badge` resolves disabled theme background/border tokens; global locking defaults to `None`. The old forced lock-style branch is gone. Styling does not change edit admission.
2. DopeSheet uses `guiraffe_core::focus_ring`; CurveView and BarCurveView also use the shared overlay outside their clipped content. All three have named AccessKit roles and Focusable handles, inherit opt-in Tab policy, and suppress GPUI's automatic pointer-focus transfer. Native tests verify roles/labels, no click focus, keyboard focus, a visible shared ring and unchanged content bounds.
3. Both full showcase suites ran. All 232 gallery PNGs were regenerated (58 entries × two themes × two backends), including all four bar-curve-editor goldens. The four combined timeline fixtures were also refreshed. All final tests compared goldens with updates disabled.
4. Summary rows deliberately use aggregate diamonds with no value hold bars, rather than indexing a group's own key metadata by aggregate indices. Selected child times highlight their summaries. A pure regression includes group-owned keys whose indices differ from their aggregate times.
5. Box anchors store time and content-space row coordinates. Scrolling during a drag no longer moves the anchor. A native regression scrolls two rows mid-drag and verifies selection still starts on the original row.
6. Key dragging accumulates a snapped delta from the press position, preserving the grab offset. A seconds-based native regression presses five pixels off-center and moves one pixel, checking that only one pixel of retiming occurs.
7. The dope showcase has square/circle/diamond key types, selected child/summary highlights, channel swatches, ghost icon toggles and a fully visible current-frame badge. Badge geometry clamps its complete rectangle inside the ruler. Curve and bar value labels paint in a foreground pass with scheme-aware backing, so curves/bars cannot obscure them.
8. Bar interpolation samples actual fractional pointer coordinates rather than rounded index endpoints; a reverse-direction regression checks fractional samples. Aligned tangents preserve the opposite handle's length in screen space using x/y scale factors, including default displayed handles. Pure metric tests cover anisotropic axes.

## Final validation

At source/demo revision `73d1538`, with no snapshot-update environment variable:

- `cargo test --locked -p guiraffe-timeline -p guiraffe-showcase`: **83 passed** on upstream.
- Same command with `--features gpui-kit/gpui-fast`: **83 passed** on fast.
- Each backend: timeline **72** (55 unit, 13 interaction, three release keyboard tests, one light/dark snapshot); full showcase **11** (unit, coverage, interaction, knobs, loading visual and all-entry rendering). No failures or ignored tests. The one all-entry render test captures 116 scenes per backend; these are not counted as 116 Rust tests.
- `cargo clippy --locked -p guiraffe-timeline -p guiraffe-showcase --all-targets -- -D warnings`, both backends: passed.
- `cargo fmt -p guiraffe-timeline -p guiraffe-showcase -- --check` and `git diff --check`: passed.

All 232 showcase images were visually scanned in 16 labelled contact sheets, with close inspection of the changed timeline scenes; all four native timeline fixtures were inspected together. Outside the five timeline demos, the 106 upstream images differ only in the header-count rectangle (114×22 pixels at +436,+39). On fast, 98 of 106 are also header-only; eight have additional subpixel raster differences whose maximum channel delta is one out of 255. Inspection found no corresponding layout change. Existing snapshot tolerances were retained.

The earlier native-input rendering boundary remains: actual editor/API updates retain the cached shell on fast, but platform pointer dispatch can rebuild cached ancestors. No stronger native-pointer shell-retention claim is made. No desktop visual session or new timing/allocation benchmark was run. Validation used stable Rust 1.98.0 and the shared `feat-chat/target` cache.
