# Calendar views

Owner: OMP. New crate: `crates/calendar`, package `guiraffe-calendar`.
Status: **proposed — foreman review required before implementation**.

## Scope

One Control with four views over one event model:

- **Year**: twelve month mini-grids. Every date has a number; dates with events have a
  theme-resolved intensity tint and discrete markers. Selecting a month opens Month view;
  selecting a date opens Day view.
- **Month**: a stable six-week grid with adjacent-month dates, day numbers, dot markers and
  event chips. Cell capacity determines how many chips render; the remaining count is an
  operable `+N more` affordance. Overflow reports the date and reveals its events without
  losing the current navigation context.
- **Week**: seven day columns over a virtualized 24-hour grid, an all-day lane, overlapping
  timed-event blocks, and a now-line when the current instant intersects the visible range.
- **Day**: the same hour grid and all-day lane restricted to one full-width day column.

Shared behavior: previous/next/today navigation, direct view switching, stable event identity,
optional editing, locale-aware week start, explicit display time zone, keyboard operation,
semantic feedback, and light/dark themes on both GPUI backends.

### Non-goals

- Replacing Kit's `Calendar`, `DatePicker`, or `TimeField`.
- Recurrence expansion, calendar-server synchronization, reminders, invitations, or a built-in
  event editor dialog. Hosts expand recurrence into ordinary items and own persistence/dialogs.
- Implicit system-locale or system-time-zone discovery. Applications supply these policies.

## Public shape

Follow `spec/00-api-conventions.md`: an interaction-owning `CalendarState` entity plus a
`Calendar::new(&state)` element. `CalendarProps` holds serializable visual/behavioral props;
callbacks are builder arguments, not serialized props. `init(cx)` is idempotent, registers the
`GrCalendar` actions/key context, and forwards the `gpui-fast` feature to `gpui-kit`.

Primary names:

| API | Responsibility |
| --- | --- |
| `CalendarViewKind::{Year, Month, Week, Day}` | Visible projection; navigation step is year, month, week, or day respectively. |
| `CalendarItem` | Immutable host event with `EventId`, title, `ItemTime`, optional color, and lock flag. |
| `EventId(u64)` | Stable identity used for selection, editing, focus restoration, and callbacks. |
| `ItemTime` | Validated timed or all-day range, defined below. |
| `CalendarModel` | Shared, revisioned item collection and its indexed lookup snapshot. |
| `CalendarLocale` | Localized labels plus the locale's first weekday; `week_start` can override only that policy. |
| `CalendarState` | Visible date, view kind, model, display zone, selected date/item, focused cursor, transient drag, and injectable now value. |
| `Calendar` | Element builder for props and convenience callbacks. |
| `CalendarEvent` | Typed emitted navigation, selection, overflow, and edit-request events. |
| `CalendarChange` | Proposed create/move/resize payload. Hosts validate and apply it; a preview is not persistence. |

Navigation is observable through `CalendarEvent::Navigated`. Editing is controlled by the host:
`CalendarEvent::EditRequested(CalendarChange)` is emitted on commit and the calendar renders a
new arrangement only after the host supplies the changed model. `CalendarState` also exposes
model/navigation mutators for applications that keep this state as their source of truth.
Read-only mode and per-item locks suppress edits without disabling navigation.

Time inputs use `chrono`. Named IANA zones use `chrono_tz::Tz`; UTC and fixed-offset values are
accepted through one display-zone enum. `serde` support on data types is opt-in through the same
chrono/chrono-tz features. No GPUI type appears in the pure model or layout modules.

## Event and time invariants

`ItemTime` is an enum, so all-day dates cannot be mixed accidentally with instants:

- `Timed { start: DateTime<Utc>, end: DateTime<Utc> }`: an absolute half-open interval
  `[start, end)`. `end > start`; instants are not rewritten when the display zone changes.
- `AllDay { start: NaiveDate, end_exclusive: NaiveDate }`: a civil-date half-open range in the
  display calendar. `end_exclusive > start`; these dates have no hidden UTC offset.

Constructors validate ranges and return a crate error rather than panicking. Titles and other
text use `SharedString`; model storage/index snapshots use `Arc` so cloning a view snapshot does
not clone event payloads. Item colors select `ColorName`/semantic variants, never RGB values.

Timed events are converted to the display zone at the model-index boundary. An event spanning
midnight appears in every intersected day with continuation cues. DST transitions use the actual
instant timeline: repeated local times display both occurrences in chronological order and the
nonexistent hour is skipped. Rendering never interprets a local `NaiveDateTime` ambiguously.
All-day events continue to mean the same civil dates when the zone changes.

`now` is an injectable `DateTime<Utc>` value. Applications may advance it on their own timer;
pure tests and showcase fixtures supply fixed values. The now-line is computed only for visible
dates and does not cause a background notify loop while unchanged.

The locale supplies translated month/day/accessibility labels and the default first weekday.
The explicit week-start override accepts every `chrono::Weekday`. Month grids and week navigation
use the resolved start consistently, including weeks that cross month/year boundaries.

## Model lookup and pure layout

The model owns events once. An immutable index snapshot is rebuilt/published only when the model,
visible zone, or relevant range changes; `render` does not scan or sort all events. Sorted ranges
and day bucketing provide bounded lookup by visible range. Multi-day bucket storage is proportional
to event-day intersections, not to the duration of unbounded recurrence. The host must expand only
the recurrence window it wants to display.

Pure modules own and test:

- Week/month/year calendar arithmetic, adjacent-month cells, leap years, and week-start overrides.
- Zone conversion, midnight spanning, and DST fold/gap behavior.
- Visible-range lookup and stable event ordering (`start`, longer duration, then `EventId`).
- Month chip capacity and `+N` calculations at zero/small/variable cell sizes.
- Overlap clustering and lanes: connected overlapping intervals form one cluster; assign the first
  available lane in stable event order. Lane index/cluster width determine left/width.
- Minute-to-y and y-to-minute conversion, visible hour ranges, bounded overscan, and clamping at
  day boundaries.
- Fifteen-minute edit quantization applied to the original drag start plus current delta, avoiding
  cumulative rounding drift. Moves preserve duration; resizes preserve the opposite endpoint.
- Visible keyboard cursors and focus restoration across navigation, virtualization, and model changes.

Layout is recomputed only for a changed visible range/model revision. Pointer movement updates a
small transient preview entity; it does not rebuild the complete model or notify an owning shell.

## View behavior

### Year

Each mini-month uses the resolved locale week start and blanks unrelated-month cells rather than
rendering duplicate dates. Event count maps to a bounded set of theme intensity steps; zero is
distinct from nonzero. Day labels expose the count as text/accessibility metadata, so tint is not
the only signal. A small marker row disambiguates consecutive intensity steps and high-contrast
themes. Navigation changes one year; Today retains Year view and selects the current date.

### Month

A fixed six-row grid prevents layout jumps between adjacent months. Today and selected dates use
separate semantic shape/border cues. Adjacent-month dates remain operable and select/navigate on
activation. Events render as chips when at least one full chip row fits, otherwise as numbered
markers. `+N more` is always computed from actual clipped content and opens an accessible list or
popover supplied by the calendar's overlay host; it never silently discards hidden events.

Dragging across empty dates previews an all-day range. Moving a chip preserves its all-day/multi-day
span; edge gestures resize by whole dates. A click without meaningful pointer travel emits
selection rather than creating an item. Month editing has no timed/fifteen-minute interpretation.

### Week and Day

The all-day lane stays outside the hour scroller. Its multi-day items use the same whole-date
create/move/resize rules as Month; bounded chips/overflow keep its height independent of item
count. The hour grid virtualizes rows with bounded overscan and draws only visible hour lines,
labels, day intersections, and event blocks. Day reuses exactly the Week controller with one
visible day, not a separate editing implementation.

Timed blocks show title and accessible start/end text; content reduces to a compact label before
it becomes illegible. Zero-height rendering is not allowed: short valid events receive a minimum
visual target while preserving their actual time semantics. Clamped off-screen portions display
continuation cues. A dense overlap cluster whose lane count exceeds available pixels collapses
the remaining lanes into an operable count affordance instead of allocating 10,000 sub-pixel
children. Day view uses the full available column width.

Drag sessions use GUIraffe-managed pointer gestures, not native GPUI drag:

- Empty timed grid: drag to create; a simple click proposes a one-hour item.
- Event body: move in fifteen-minute steps, across day columns in Week.
- Top/bottom handles: resize in fifteen-minute steps, with a fifteen-minute minimum duration.
- All-day lane: create/move/resize by whole dates.
- Edge proximity auto-scrolls the virtual hour list while preserving the original time anchor.
- Escape, pointer-capture loss, view navigation, theme/backend changes that invalidate geometry,
  or item/model replacement cancels cleanly. Cancel never emits an edit request.
- A committed gesture emits one proposed change. Locked/read-only items have no resize handles,
  grab cursor, or edit preview.

## Keyboard, focus, and accessibility

The toolbar composes Kit buttons for previous/today/next and the existing GUIraffe/Kit view-switch
controls. Buttons have explicit accessible names (`Previous week`, `Next year`, and so on).
Pointer activation does not capture focus by default. `.focusable(true)` or a containing GUIraffe
focus scope opts the calendar grid and actionable chips/overflow into Tab navigation. Focus rings
use the shared overlay helper and do not change layout.

Within the grid, one roving cursor limits Tab stops:

| Key | Date/grid behavior |
| --- | --- |
| Left/Right | Previous/next date in Year/Month; fifteen-minute cursor movement in Week/Day timed grid. |
| Up/Down | Same-weekday movement in Year/Month; fifteen-minute movement in Week/Day. |
| Shift+Left/Right | Move the all-day cursor by date; adjust an active all-day resize endpoint. |
| Shift+Up/Down | Adjust an active timed resize endpoint by fifteen minutes. |
| PageUp/PageDown | Previous/next visible navigation unit. |
| Home | Move the cursor to Today without changing the selected view kind. |
| Enter | Activate a date, item, or overflow affordance; commit a keyboard-created item. |
| Space | Pick up/drop the selected editable item. Arrows move it; shift+arrows resize it. |
| Escape | Cancel an edit preview or dismiss overflow; restore the pre-gesture value and focus. |

Keyboard create starts from the focused empty date/time with the same default/minimum durations as
pointer create. All pointer operations have this keyboard equivalent. Disabled/locked controls
suppress commands as well as pointer callbacks.

Expose grid/grid-cell or the closest supported GPUI roles, item names including localized date and
time, selected/disabled/expanded state, row/column position, and full virtualized set counts where
the backend supports them. Overflow uses expanded state and returns focus to its originating cell
on close. Edit commits/cancellations provide a host-visible announcement event; any missing live-
region or virtualized-position support must be recorded in `KNOWN_ISSUES.md`, not claimed as done.
Tests cover accessible nodes where GPUI exposes inspection, and record remaining native screen-
reader verification separately.

Interaction tests follow §12: pointer use without focus capture, disabled/read-only suppression,
keyboard activation inside a form scope, focus-ring geometry invariance, Escape recovery, and
focus retention after item updates/virtualization.

## Styling and density

All color, border, radius, typography, spacing, marker intensity, selected/today/now states, drag
preview, hover/pressed/disabled/focus, and overflow appearance resolve through `GuiraffeTheme`,
`StyleContext`, `StateKind`, and `Variant`. There are no fixed RGB values, raw-opacity disabled
states, or widget-owned outer margins. Empty, selected, today, adjacent-month, continuation, and
locked states remain distinguishable without relying only on hue. Reduced motion removes animated
scroll/view transitions; density changes row/chip targets without changing time math.

## Performance budgets and probes

Reference dataset: 10,000 deterministic items in the visible year, including midnight-spanning,
all-day, short, and overlapping events, with a fixed time zone and viewport. Additional adversarial
fixtures place all items in one month and overlap them in one week. Measure both backends in a
release build; record commit, rustc, CPU/GPU, scale, viewport, dataset seed, and machine load.

Per-view gate:

| Operation | Month | Week |
| --- | ---: | ---: |
| Cold mount to first useful frame with 10,000 items | ≤ 200 ms | ≤ 200 ms |
| p99 input-to-frame work during scripted navigation, scrolling, selection, hover, and edit preview | ≤ 8 ms | ≤ 8 ms |
| Any measured frame above 16 ms | failure | failure |

The general Year/Day interactions also target ≤ 8 ms p99, but the explicit foreman gate is Month
and Week. Warm repaint and hour scrolling allocate zero after warm-up on paths covered by the
counting allocator. Recompute allocations are bounded and recorded. Resident UI elements and event
blocks are `O(viewport + visible/aggregated items)`, never `O(total items)`. Model/index memory is
reported independently from event payload storage.

A PR-scale headless probe checks mount, scroll/navigation/edit preview, resident-element bounds,
warm allocations, and frame percentiles. A nightly 30-minute soak repeats these interactions and
samples RSS/PSS every 10 seconds; post-warm-up drift must stay ≤ 2%. No mutex/lock acquisition,
all-event sort, recurrence expansion, or large layout build is permitted on the render/drag hot
path. Render-count tests prove unrelated shell state and drag movement do not redraw sibling
showcase content.

## Showcase and verification

Add four showcase entries (Year, Month, Week, Day) using one deterministic fixture and fixed now:

- Knobs: editable/read-only, locale/first weekday, display zone including DST fixtures, density,
  event density/overlap, all-day items, now-line on/off, and selected/view date.
- Default, empty, overflow, locked, midnight-spanning, DST fold/gap, and dense-overlap states.
- Light and dark goldens for every view on upstream GPUI and GPUI Fast: at least sixteen images.
  Inspect every generated PNG; include focused, overflow-open, drag-preview, and dense cases needed
  to cover the knobs rather than relying only on default snapshots.
- Pure tests for every arithmetic/layout/invariant above and randomized event operations against a
  small naive reference model.
- GPUI `TestAppContext` tests for all pointer/keyboard gestures, callbacks, focus, accessibility,
  cancellation, render counts, and zero-size viewport recovery.
- `cargo test -p guiraffe-calendar`, crate doctests, strict Clippy, formatting, workspace/allocation
  gates, and `scripts/gate.sh` on both backends.

## Implementation order

1. Foreman approves this specification.
2. Pure model/index/calendar arithmetic and property tests.
3. Shared Control/navigation and static Year/Month views.
4. Virtualized Week/Day grid, overlap layout, all-day lane, now-line, and editing previews.
5. Keyboard/accessibility completion, showcase knobs, snapshots, and render-count gates.
6. Ten-thousand-event probes/soak, performance fixes with before/after evidence, full gate,
   PNG review, and report to the foreman. Never merge or push from the implementation worktree.
