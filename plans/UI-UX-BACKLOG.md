# UI/UX backlog → implementation slices

Source: `docs/ui-ux/guiraffe-recommendations.md` @ 91f3161 (branch docs/ui-ux-package, baseline
ac49cc5; awaiting foreman integration). Audited against release/0.1 @ f1c6a8f on 2026-11-18.
Lead: Claude (ui-ux-implementation). Never merged by the implementer; foreman integrates.

## Audit: already done on release (do not redo)
- Press / opt-in focus / focus-visible ring / AccessKit roles: SegmentedControl, ChipGroup,
  RangeSlider, Dropzone, ActionIcon (+ `loading`), Pill remove ("Remove X"), showcase sidebar,
  Spotlight rows. Tests: components/tests/{focus_scope,segmented_focus,interaction}.rs,
  inputs/tests/controls.rs (pointer keeps editor focus, Tab, Enter/Space, arrows, disabled, roles).
- Overlay focus chain + restore (overlays/src/host.rs).
- Kit semantic colours contrast (danger/success/warning/info) in `contrast_checks`.
- spec/18 "Widget gaps" table is stale (predates the §12 merge) → slice S2 updates it.

## Slices
| id | crate(s) | status | branch / owner |
|---|---|---|---|
| S1 semantic contrast: surface×state pairings, focus on selection, neutral vs destructive, tinted bgs | core | **review-ready** c577308 | ui-ux-contrast-pairings / Claude |
| S2 spec/18 refresh + toolbar focus policy doc (pointer keeps editor focus; when a control must take focus) | spec | todo | Claude |
| S3 overlay dismissal + focus-restoration contract & interaction checks | overlays | **review-ready** 89a3130 (gates re-run by Claude) | ui-ux-overlay-focus / Codex |
| S4 inactive-window presentation (theme state + window-activation signal) | core + windows/ui | **needs foreman decision** (new cross-crate API) | — |
| S5 contrast captures: settings screen + dense toolbar, light/dark/high | showcase | todo (after S1) | — |
| S6 layout defaults pilot: settings/toolbar/inspector via Recipes+Density; overflow at narrow widths; overrides win; nested providers | layout + core | todo | — |
| S7 showcase workflows: searchable list, edit form, command/overlay flow; deterministic data; evidence metadata (rev, backend, scale, theme, viewport, state) | showcase + test | todo (after S3) | — |
| S8 timeline keyboard (pointer-only today) — follow-up batch | timeline | later | — |
| follow-ups: DataGrid/Flow keyboard journeys, motion recipe + reduced motion, app example, token exchange | various | later | — |

## Open decisions for the foreman
- Standard-contrast selected fill (Filled shade) is 1.9–2.8:1 vs body for 11 primaries; pinned as
  an exemption in core tests. Fixing it recolours every filled control (palette decision).
- S1 changes the dark focus-ring shade and High-contrast selected fill → showcase goldens with a
  visible ring/selection may need regeneration in the batch run.
- Platform evidence: only Linux (Arch, X11/Wayland, software Vulkan under Xvfb) is available here;
  no macOS/Windows accessibility inspection possible → manual platform record still open.
