## Notes

Rows still marked `-` are mostly getters (`selected`, `sizes`, `stats`, `viewport`), test hooks
(`input_for_test`, `press_with`, `drag_to`, `grab`, `paint_timings`), pixel-variant setters
(`gap_px`, `padding_px`) or per-breakpoint resolvers (`cols_at`, `span_at`) that the live
examples exercise indirectly. Every user-facing option is a knob or static example.

Event logs (`src/events.rs`, `EventLog::watch`) are shown for: Splitter, AppShell,
SegmentedControl, ActionIcon, Pill, ChipGroup, TagsInput, RangeSlider, Dropzone, overlays
(toasts/modal/confirm/drawer), FlowState, Markdown links/tasks, Chat, Gantt, Tracks, Dope sheet,
Curve and Bar editors, DataGrid.

## Kit controls / charts (hand audit)

| Section | Knobs | Static |
|---|---|---|
| Button | variant, size, icon, loading, disabled | groups |
| Text input | size, disabled, cleanable, validation (none/required/error/read-only) | password, search prefix, number, textarea, form field |
| Select | size, cleanable, disabled | — |
| Toggles | size, disabled | checkbox, radio, switch |
| Line chart | curve, dots, grid | — |
| Bar / area / pie charts | — | static only (gap: knobs for stacked/labels) |

## Gaps for owners (options a component should have but lacks)

- components: `ActionIcon` has no `disabled`/`loading`; `Pill` has no size/color/disabled;
  `Indicator` has no position/offset/size/processing; `Highlight`/`Mark` have no color.
- inputs: `TagsInput` has no `disabled`, `error`, `required` or size; `ChipGroup`/`RangeSlider`
  have no size or error state; no shared invalid/required state across GUIraffe inputs.
- layout: `Divider::vertical()` renders no visible rule inside `Group` (height collapses);
  `ScrollArea` has no scrollbar visibility/offset options.
- overlays: `Toast` has no color/variant/icon/title; `FloatingTooltip` has no arrow option.
- flow: no read-only / non-editable option on `FlowState`; options are construction-only
  (no `set_options`), so the showcase rebuilds the canvas per knob change; no edge-type default.
- mermaid: no theme override independent of scheme; no public selection API.
- markdown: no heading-anchor/TOC or theme options.
- chat: `ChatProps` only has title/subtitle (no avatar, density, read-only/disabled composer).
- timeline: Gantt has no snap-to-grid for drag (only keyboard `set_key_step`).
- datagrid: no built-in filter UI (filters are programmatic only).
