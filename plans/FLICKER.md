# Component property changes

The starting revision for this pass, `9baac73`, already includes the first flicker batch through `948b6b2`. The historical diagnoses below come from comparing that batch with its parent (`95b80d6^`) and tracing the current entry points in the showcase.

| Entry | Mechanism before the first batch | Current implementation |
| --- | --- | --- |
| CodeViewer and Code annotations | The knob event invalidated the preview, then rendering the preview applied the options and annotations. The text canvas published fresh geometry in prepaint, but the overlay entity built its layout from the prior geometry and requested another render via a deferred notification. Minimap changes affect usable text width and overlay positioning. Marks and inlays also replaced prepared analysis, dropping retained widget measurements and bubble entities even for unchanged blocks. | Apply the options and prepared analysis in the knob event. The overlay uses the current text geometry in prepaint. Compatible measurements and bubble entities survive analysis updates, and review widgets have stable keys. |
| Property grid and Tree view | Preview rendering deferred `set_row_style`, expansion, and row-height setters. The next paint could use the prior style. Alternating colors are calculated from the row index and style, so the row renderer has no async color cache to rebuild. | The shared showcase callback applies setters to the existing tree entity before notifying the preview. |
| Flame graph, Chart grammar, and other stateful showcase controls | Setters were called during preview rendering, some in a deferred callback. | The first batch moved these setters to the same showcase callback. Frame tests sample Flame graph, Chart grammar, and SegmentedControl. |

The shared timing fix lives in `crates/showcase/src/knobs.rs`, where the knob event and preview notification meet. CodeViewer's geometry and measurement fixes live in the code crate. The highlighting pipeline is independent of these knob changes. CodeViewer prepares analysis before publishing it, and only reuses measurements when the logical block identity and payload agree.

A search of remaining `defer` and `Applied` uses found separate paths worth distinguishing. Flow's live showcase intentionally constructs its graph from builder options and replaces the entity synchronously. Its layout demo requests layout work whose existing graph stays visible. Date picker demos apply window-dependent setters during render, and Spotlight builds a replacement from knobs during render. DataGrid's zebra setting already applies from the knob event. Its deferred synchronization responds to measured geometry and is separate from that setter. The audit does not establish a blank intermediate paint for these entries.

## Minimap markers and goldens

The old red line was an overview of an entire decoration layer. It used an arbitrary lane color and mapped bins across the full minimap height even when the miniature source strokes occupied only its top portion. The marker lanes now represent gutter marks, take color from the same mapping as the gutter, and place marks on the same source scale as the minimap strokes. Large files continue to use a bounded overview.

Reviewed all eight inherited changes as before/after pairs. This pass does not change any PNGs.

| Goldens | Reviewed difference |
| --- | --- |
| Code annotations fixture, upstream dark and light | The red stripe down the left of the minimap is gone. Compact red, blue, and green marks sit by the miniature source rows. Gutter and review content remain visible. |
| Code annotations fixture, gpui-fast dark and light | The same stripe removal and gutter-aligned marker placement appears in both themes. |
| Showcase Code annotations, upstream dark and light | The left edge of the minimap shows discrete colored gutter markers alongside the corresponding source strokes. The longer miniature retains its viewport outline and source strokes. |
| Showcase Code annotations, gpui-fast dark and light | The same marker and viewport treatment appears in both themes. The review card and annotated code remain visible. |

## Frame checks

The showcase regression fixture instantiates the actual catalog demo. It drives the segmented control's selection event and captures every completed GPUI scene, including automatic draws. Captured frames are compared to the settled preview and the test also requires the toggle to visibly change the preview. The test operates on paint output and does not enforce a wall-clock render budget. The fixture excludes the outer knob row so control interaction paint cannot obscure a preview transition.
