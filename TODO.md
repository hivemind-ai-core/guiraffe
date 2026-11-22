# TODO

Living task list (foreman-maintained; updated 2026-11-21). Plans: plans/ROADMAP-0.2.md (sections
A–F), plans/PERF-0.2.md, specs in spec/. Branches merge into release/0.2 via foreman integration
batches; nothing goes to main without the user.

## In progress (0.2 is frozen for the final bench: no new work, user 2026-11-22)
- [ ] Batch 22: cache-audit fixes (`perf-cache-audit`) and calendar views (`feat-calendar`, spec/27).

## Performance goal — remaining (plans/PERF-0.2.md)
- [ ] Final quiet window (all agents paused): re-measure all scenarios on both backends; 30-minute
      mixed soak (≤2 % drift); stress-limits sweep (plans/PERF-LIMITS.md); both-sided matched-metric Electron
      comparison; commit a new full bench baseline.
- [ ] Confirm Chat and Markdown memory in the quiet window (provisional results are within budget).
- [ ] PRODUCTION.md performance rows ✅ for every component, linked to checks; merge the cache
      inventory into them.

## 0.2 small follow-ups
- [ ] Design Chat selection across virtualized history using logical message IDs and text
      offsets. Preserve rendered Markdown copy behavior when selected rows are offscreen.
- [ ] Kit gaps (report upstream): dark slider track contrast; dim checkbox label in dark; Calendar
      notifies on every render; day cells lack date/a11y hooks; calendar nav ignores min/max; no
      calendar keyboard nav; Button doesn't announce selected state; five stock gpui-fast Kit test
      failures.
- [ ] gpui-fast quirks: picker popover open/close re-renders the host once; host picks up popover
      reads after a forced frame.

- [ ] Remove the date pickers' "keep last non-default StyleContext" workaround (crates/inputs/src/dates/picker.rs) once fix-stylecontext-fast (b8ec14e) is merged.

## Release readiness
- [ ] CHANGELOG 0.2; KNOWN_ISSUES.md current; API docs coverage.
- [ ] Packaging notes: apps must copy the vendored `[patch.crates-io]` entries (vendor/README.md);
      `guiraffe new` template should include them.
- [ ] Upstream PR drafts ready (plans/GPUI-UPSTREAM.md, plans/KIT-UPSTREAM.md, mermaid) — user
      decides when to submit.
- [ ] 0.1: user's manual pass on a real display → merge release/0.1 to main, tag v0.1.0.
- [ ] 0.2: final full gate + golden review → merge to main, tag v0.2.0 (user approval).

## After release (roadmap F)
- [ ] Startup study: per-phase warm/cold breakdown, floor vs empty GPUI / bare wgpu window, levers
      (EGL deferral, shader cache, parallel init, lazy init, linking); cold start needs a page-cache
      drop (sudo).
- [x] (batches 16 and 19, plans/FLICKER.md) Flicker free component property changes. Many components flicker when chaninging their properties 
      (eg code viewer, toggling marks/inlays being enabled/disabled, property tree toggling alternating
      row colors, etc), likely because they paint an "empty state" before painting the new state. This
      flicker needs to be eliminated. Most pronounced in debug builds, but it seems to be because it
      paints an empty frame before the updated frame and in release builds the empty frame is so fast
      that its harder to notice. Most pronounced when toggling the code viewer minimap on and off.
- [x] (batch 16) In the showcase, the code viewer minimap has a strange red line through the middle. If its meant
      to highlight the gutter icons, then it doesn't match the gutter colours and is oddly positioned.
- [x] (spec/29, branch feat-layout-animation) Animate everything. An option to animate all layout changes, eg when something is added to a list,
      or a state is toggled, that causes the size to change (eg collapsing a property list group) it
      should (optionally) animate by lerping between the old and new size.
- [x] PropertyGrid options (user 2026-11-21): (a) `focus_row(bool)` — when disabled, a row is never
      focused/highlighted, even while its value is being edited; (b) reset affordance configurable
      (`reset_button(bool)` / per-property) — today "Reset" always shows when value ≠ default
      (crates/tree/src/view.rs:1412). Bundle with the PropertyGrid editing items below.
- [x] The property tree component visual bugs: when editing text in the showcase text value, the textboxes
      bottom border spills below the rows space. Additionally, the input should default to a single line
      input where pressing enter saves it (save button, tick/x icons should be optional settings).
      Need to be able to tab to the next property value (and shift+tab) to the previous.
      Multiline text should make the row expand to multiple lines, look at other GUI property lists for
      inspiration (godot, unity, blender, etc).
      Done on feat-property-grid-editing: `focus_row`, `reset_button` (+ `PropertyOptions::reset_button`),
      `edit_buttons(EditButtons)`; Enter/Escape, Tab/Shift+Tab, multi-line rows grow while editing.
      Follow-up: multi-line values still display on one line ("↵") when not being edited.

## 0.3 (plans/ROADMAP-0.3.md, decided 2026-11-21)
- [ ] **Core state model + control protocol** (spec/30): stable control ids, typed snapshots and
      actions, one action bus for pointer/keyboard/shortcut/Spotlight/ribbon/DataUi/MCP/scripts;
      zero cost at render time (allocation + scaling gates from day one).
- [ ] **Store-first `ui!`** (spec/31): `<=`/`<=>` bindings, `on_x => Event`, `store!`,
      `#[component(stateful)]`; the canonical structured path. The builder API stays for standalone UIs.
- [ ] **Command ribbon** (spec/32): port of ../gui/research/cmd onto guiraffe-commands + the bus.
- [ ] **Multi-window docking**: pop-out panels, cross-window drag, layout restore across monitors/DPI.
- [ ] Moved from 0.2 (user 2026-11-22): inspect/drive node ids; agent authoring rest (spec/21);
      `feat-mermaid-2` salvage; calendar agenda view; UI/UX backlog; DnD source-card ghost.
- [ ] Trim GPUI per-frame high-water buffers (user-accepted 0.2 exception, roadmap 0.3 §E).
- [ ] spec/30 phase 1 starts by extending the Catalogue 1.2 descriptors (feat-catalog-coverage,
      `49dbc28`) with optional actions and snapshot sections.

## Later / out of scope for now
- macOS/Windows CI and testing; crates.io publishing.
- windows: tear-off panels, parent/child + modal windows, examples/multi.rs; X11 2 px restore
  offset compensation.
- Tooling DX: templates for `guiraffe new`, `guiraffe dev` hot relaunch, docs site from showcase.
- Refactoring UI study → design recipes (book in repo root, do not commit it).
- compare app: input-latency and RAPL energy metrics.

## Verify and close (older review/feedback items, many likely fixed in the 0.1 bug batches)
- [ ] Showcase feedback 2026-11-16: ScrollArea scroll; SegmentedControl double-click; RangeSlider
      track glitch; FloatingTooltip hiding; Mermaid blank diagrams; Flow edge/handle overshoot and
      minimap drag-to-pan; Gantt/tracks x-lock, resize cursor, left-edge resize.
- [ ] Bugs from showcase build: FlowState::new fallible in cx.new; chat tool/reasoning headers
      centred; FloatingTooltip contrast; LoadingOverlay opacity; Snapshotter "settle" option;
      mermaid width re-render test.
- [ ] Code viewer polish: minimap when the whole file fits; trace curve crossing text; inlay hint
      double space; trace endpoint dots; showcase meta line empty covers for code entries.
- [ ] Showcase coverage gaps (crates/showcase/COVERAGE.md): ActionIcon disabled/loading, Pill,
      Indicator, Highlight colors, TagsInput states, ScrollArea options, Toast options, tooltip
      arrow, flow read-only, mermaid theme, markdown anchors/TOC, chat props, Gantt snap, datagrid
      filter UI; component×option matrix in the showcase README.
- [ ] Catalogue recipes polish (error helper text colour/size; DataUi input height).
- [ ] compare app Document page: use MarkdownView scrollable mode.
- [ ] Timeline nits: calendar-aware day/week/month ticks; dope sheet ruler.
- [ ] core: style helper traits (.p_size, .radius_size, .shadow_size).
