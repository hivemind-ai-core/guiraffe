# Showcase feedback fixes — 2026-11-17

Combined source tree validated: `c132f2404836f29850e8c03032e9b045211a093b` on `verify-feedback-batch`, based on
`release/0.1` at `56e3a5a`. This is a separate validation checkout composed from the
branches below; release and main were not merged or pushed.

| Branch | Head | Change |
| --- | --- | --- |
| fix-mermaid-paint | 2fce85b | Earlier shared standalone/Markdown Mermaid visibility fix |
| fix-showcase-interactions | f09e191 | ScrollArea, indicator animation, slider, tooltips and loading overlay |
| fix-flow-feedback | 50c5d99 | Handle alignment, step routing, markers, minimap and safe construction |
| fix-chat-header | 597d201 | Tool-header alignment and stable highlighting in visual fixtures |
| feature-datasheet | dc0e010 | Required efficient spreadsheet/data-sheet specification |

## Behavior and regression evidence

- ScrollArea expanded to its content instead of filling the constrained host.
  It now fills that host while preserving explicit style overrides. The wheel
  regression failed before the fix (content stayed at y=0) and passes afterward.
- SegmentedControl selected on the first click, but its indicator retargeted in
  prepaint after render sampled the resting spring. A next-frame leaf notification
  starts animation without another input. GPUI tests cover selection, keyboard
  navigation and geometry; the native regression catches the actual paint stall.
  The native check failed against the release implementation (contrast -0.054902)
  and passed against the fix (+0.054902). Retained [before](validation/feedback-2026-11-17/segment-before.png)
  and [after](validation/feedback-2026-11-17/segment-after.png) captures.
- RangeSlider drew its focus border on the 4px rail, leaving only 2px of neutral
  fill. Focus now uses thumb halos; the rail stays solid and thumbs have solid
  accent fills. The pixel regression measured 4 physical rows before the fix and
  at least 7 afterward at 2x scale. Light/dark references were reviewed.
- The showcase mounted controlled FloatingTooltips permanently. Trigger hover
  now controls mounting, and tooltips have a border/shadow. The GPUI hover/leave
  regression fails with permanent mounting and passes with the fix.
- LoadingOverlay now uses a translucent backdrop. Its two-color pixel regression
  fails with the original opaque backdrop. Existing pointer and keyboard blocking
  tests remain green.
- Full layout validation exposed existing fast-backend failures. The reviewed
  first-measurement fix from `fix-layout-fast` is included: a real container width
  replaces the window-width guess even in the same breakpoint band. Existing grid
  assertions remain intact. The splitter counter test now compares only renders
  during its drag, excluding unequal backend-specific mount counts. The splitter
  implementation was not rewritten. Coordinate with that branch's author to avoid
  integrating its overlapping width fix twice.
- Flow handle dots were offset by the node's 1px border. Their centers now match
  model endpoints at 0.6x, 1x and 2x. The geometry test failed before the correction.
  Arrow tips end outside the fixed screen-size handle boundary, including stroke.
- Step routes included Bezier control points beyond their middle bends, creating
  reversals and protruding stubs. Forward facing-handle routes now use the actual
  bends directly. Step and smoothstep regressions fail on the original routes.
- Minimap input now tracks a drag through release, with an occluding surface above
  nodes and an outlined viewport. The real pointer regression failed with the old
  press-only camera update; it now reaches the expected camera without editing nodes.
  Inspected native [zoom/pan](validation/feedback-2026-11-17/flow-zoom-pan.png) and
  [zoom-out](validation/feedback-2026-11-17/flow-zoom-out.png) captures.
- `FlowState::from_editor` accepts an already validated editor. The showcase uses it
  instead of a second fallible constructor followed by an unreachable panic.
- Chat tool headers explicitly fill and left-align their native button content.
  A GPUI geometry regression verifies the content starts at button padding and fills
  the button. Keyboard toggle tests remain intact. Chat's test-only Kit dependency
  enables syntect so code-block references agree in standalone and batch builds.

## Combined validation

Ten crates were tested together: layout, components, inputs, overlays, flow,
flow-core, chat, mermaid, markdown and showcase.

| Check | Upstream | gpui-fast |
| --- | --- | --- |
| Unit, interaction, visual and doc checks | 484 passed | 484 passed |
| Full showcase, 37 entries × light/dark | passed | passed |
| All-target clippy with warnings denied | passed | passed |

No snapshot update environment variable was set for the successful combined runs.
The full showcase comparison covers 148 scenes across both backends. The standalone
Mermaid/Markdown pixel checks and focused-slider references also ran in the batch.
Changed Rust files pass rustfmt checks, and `git diff --check` passes.

Exact command pattern (with `ARGV0` unset and the shared target cache selected):

```sh
cargo test -p guiraffe-layout -p guiraffe-components -p guiraffe-inputs -p guiraffe-overlays -p guiraffe-flow -p guiraffe-flow-core -p guiraffe-chat -p guiraffe-mermaid -p guiraffe-markdown -p guiraffe-showcase
cargo clippy -p guiraffe-layout -p guiraffe-components -p guiraffe-inputs -p guiraffe-overlays -p guiraffe-flow -p guiraffe-flow-core -p guiraffe-chat -p guiraffe-mermaid -p guiraffe-markdown -p guiraffe-showcase --all-targets -- -D warnings
```

Repeat each with `--features gpui-kit/gpui-fast` before the clippy `--` separator.
Retained logs: [upstream tests](validation/feedback-2026-11-17/upstream-test.log),
[fast tests](validation/feedback-2026-11-17/fast-test.log),
[upstream clippy](validation/feedback-2026-11-17/upstream-clippy.log),
[fast clippy](validation/feedback-2026-11-17/fast-clippy.log).

Native release checks ran on a private Xvfb display with gpui-fast. ScrollArea
advanced from Row 1 to Row 8; tooltips appeared on hover and vanished on leave;
the slider and loading card were inspected in light/dark; flow was inspected after
zoom, background pan and minimap drag. Only owned app/display processes were stopped.
The repeatable first-click probe is
`crates/components/validation/first_click.py`; launch it under a 1400×900 Xvfb display
with the freshly built showcase binary path and a temporary evidence directory.

## Remaining feature work

Timeline x-position locks and configurable edge-resizing/cursors, Kit controls/chart
demos and live configuration panels remain on the feature queue. The spreadsheet /
data-sheet request is specified in spec/03, spec/16 and TODO.md, including two-axis
virtualization, provider-backed incremental data, navigation/selection/clipboard,
optional editing, standalone/embedded use and large-data performance acceptance.
No spreadsheet implementation is claimed here. Foreman integration is still required.
