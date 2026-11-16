# Components review #1: merged after fixes
Fixed:
- Render-count test now asserts: on gpui-fast the SegmentedControl animation never re-renders the host.
- SegmentedControl:
  - Segments record their boxes during prepaint without notifying. One container probe computes the target once per frame, relative to the container, with an explicit laid_out state.
  - Layout vector resized to the item count; set_items clamps the selection; an empty list is safe.
  - Keyboard: ←/→/Home/End via actions and the GrSegmentedControl key context, with a focus ring.
- Theme: core gained semantic `track`/`raised`/`mark` surfaces and themed shadows (`shadow_strength`), with contrast tests. No ad-hoc dark-mode branches or raw palette lookups remain in components.
- Metrics derived from the Size scale (gap, pill height, dot, badge, track padding) replace pixel constants.
- Card stores its parts and lays sections out at render with the final padding. First and last sections bleed to the top and bottom edges.
- Highlight renders one StyledText run with highlight ranges, so it wraps naturally. Matching is a pure `highlight_ranges`.
- `Styled` on Paper, Card, CardSection, Text, Title, Highlight, Mark, ThemeIcon, ActionIcon, Blockquote, Indicator and Pill.
- API: `Text::color`, `RingProgress::diameter`, and Pill `on_remove(&ElementId, ..)`.
- RingProgress: thickness clamp keeps a positive radius, a full turn has no seam, the label is themed.
- Snapshots include SegmentedControl and a Card with a section, in both schemes. Inspected visually.
Follow-ups:
- `Styled` on RingProgress.
- XProps / derive(Props) (phase 2).
- Full case folding in Highlight.
- Icon snapshots (KNOWN_ISSUES #1).
- Indicator position matrix in the snapshot.
