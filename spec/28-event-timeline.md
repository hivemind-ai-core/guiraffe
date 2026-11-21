# Multi-track event timeline

Roadmap 0.2 §E. An event timeline lets a reader follow parallel histories and
identify where work branched or merged. Git histories and concurrent job runs
use the same data and interaction model.

## Placement and API

Ship in `guiraffe-timeline`, with a pure `event_layout` module and a GPUI
`event_view` module, integrated through the existing crate initializer. Gantt
and Tracks establish the pattern for clipped canvas geometry, viewport-bounded
elements, and scroll and focus state. Event positions are ordered categories with
variable group lengths, so the continuous, horizontal `Axis` and editing
`PlotSurface` would give the wrong geometry. Borrow the single Timeline's
bullet, rail, label and secondary-content treatment using the shared core theme.

- `EventTrack` has a stable ID, display label and optional `ColorName`.
- `TimelineEvent` has a stable ID, track ID, signed integer position, label,
  optional secondary text and optional `ColorName`. Callers can supply an index
  or an integer timestamp and format display labels in their own locale or zone.
- `EventLink` has source and destination IDs and a branch or merge kind. A
  checked model constructor validates IDs, track membership and links. Invalid
  replacements leave the previous model available. Links run forward in
  rendered order or laterally within a slot and connect distinct tracks.
- `EventTimeline::new(&state)` uses an `EventTimelineState` entity holding the
  immutable model, scroll, cursor and selection. Expose typed selection and
  activation events through the state. Props cover orientation, size, color,
  accessible label and focus opt-in. Use the repository's data-props conventions
  for serializable visual settings. Model data and callbacks are separate.
- A content factory receives the event and GPUI context and returns an element
  for its content region. Only visible and overscanned events invoke it. Reserve
  a theme-sized content area so slot geometry is independent of measurement.
  Document its size and offer the default secondary-text renderer.

## Layout and appearance

Tracks are columns with vertical chronology and rows with horizontal chronology.
Keep text upright. The leading header names each track and follows cross-axis
scroll. Use a dot and continuous rail on each track, with label and content
grouped next to the dot. The item's theme color overrides the track color,
then the timeline default, then the theme primary. Keep text on semantic
foregrounds. Truncate or wrap within the reserved region and preserve the full
accessible label. Narrow viewports scroll on both axes.

Order positions ascending and keep input order for ties within one track. At
each shared position, align the first dot across tracks. Allocate as many slots
as the largest same-track group and stack the group's following events in
successive slots. Sparse timestamps occupy adjacent groups. Horizontal mode
transposes slot and track axes, preserving the chronology and ordering.

Draw links behind content with a distinct branch and merge treatment and a
direction cue. Route across tracks through a gap between successive slots,
or through the leading slot gutter for lateral links. Keep endpoints attached
to dots, clip to the viewport, and expose the relation in accessible item text.
Route geometry is deterministic when the data is rebuilt. Empty history shows
an empty state and preserves named tracks where present.

## Virtualization and interaction

Use ordered, compact per-track slot indexes and an ID lookup, built once per
model revision. Binary-search both axes and the events of each visible track.
Index links by chronology extent so a link can cross the viewport even if its
endpoints are offscreen. Bound dense crossing projections by coalescing links
with identical clipped geometry. Keep stable identity through data replacement,
use a deterministic neighbor fallback for removed cursor items, and clamp both
scroll offsets. Compute geometry at full logical precision and convert viewport
relative coordinates at the drawing boundary.

Opt-in focus gives the timeline one Tab entry. Clicking selects without grabbing
keyboard focus. Chronological arrows move within the current track, cross-axis
arrows move to the nearest event in the next nonempty track, and Home/End move
to the ends of the current track. Move the viewport to reveal the cursor before
painting. Enter and Space activate the cursor event once. Allow interactive slot
children to consume their own input. Expose the collection and visible items
through Kit/GPUI accessibility, with track, position, name, selection and link
information. Offscreen events become accessible as navigation reveals them.
Follow API conventions §12 for semantic hover, press and selection cues, and
use the shared focus overlay without shifting geometry.

## Acceptance

- Property tests compare stacking and visible windows against straightforward
  projections, and check connector endpoints, routing and clipped crossings.
- GPUI TestAppContext tests cover click focus preservation, keyboard traversal
  and activation, bounds stability, accessibility, replacement, content slot
  virtualization, and render isolation. Exercise both orientations.
- Showcase a branched git history and concurrent processes in both orientations,
  including same-position content and a custom slot. Capture and view each
  changed light/dark PNG on both backends.
- On both backends, run a release-mode 100k-event probe through mount and a
  scripted scroll/navigation soak. Record first useful frame including model
  preparation, p99 CPU frame time, empty-window baseline and peak memory delta,
  raw model bytes including owned string buffers, and post-warm-up memory drift.
  Require mount ≤200 ms, p99 ≤8 ms, memory delta ≤1.25× raw bytes + 64 MiB, and
  report soak duration and environment alongside measurements.
- Provide runnable Rustdoc and pass `scripts/gate.sh` using the isolated
  worktree target and `CARGO_BUILD_JOBS=4`.
