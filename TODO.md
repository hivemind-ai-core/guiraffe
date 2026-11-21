# TODO (tracked per crate; also see spec/)
- [x] core: palettes, sizes, variants, theme global + Controls bridge (foreman)
- [ ] flow-core: model, viewport, spatial index, history, serde (codex)
- [ ] flow: canvas, handles, connect, selection, minimap, controls, bench (codex)
- [ ] mermaid: port mer, async render cache, pan/zoom (omp)
- [ ] markdown: GFM viewer, highlighting, streaming, mermaid feature (omp)
- [ ] layout: Stack/Group/Flex/Grid/SimpleGrid/Container/Center/AspectRatio/AppShell (kilo)
- [x] windows: registry, lifecycle, bus, persistence (foreman, wave 2)
- [ ] overlays: stack, modals, drawer, toasts, spotlight, desktop notify (wave 2)
- [ ] components: see spec/03 (wave 2) — shipped: Timeline, Loader, Spoiler, Burger, CopyButton, KbdGroup, ColorSwatch, AvatarGroup, Stepper
- [ ] gallery app (wave 2)
- [ ] agent chat UI (wave 2)

## Follow-ups found in review
- [ ] windows: X11 restores windows 2px right of request (see ../gui KNOWN_ISSUES #127) — compensate when platform is X11
- [ ] windows: tear-off panels between windows (dock-host port), parent/child + modal windows
- [ ] windows: example app (examples/multi.rs)
- [ ] core: style helper traits (.p_size, .radius_size, .shadow_size) once layout lands
- [ ] timeline follow-ups (review nits): Days ticks align to epoch (weeks start Thursday; "months" are 30d) — use calendar-aware steps; Gantt rows are not virtualized (fine < ~1k tasks); dope sheet has no ruler yet.

## Next (user request 2026-11-16)
- [ ] **Efficient spreadsheet / data sheet**: standalone and embedded, virtualized rows and columns, provider-backed incremental data, keyboard/range selection and clipboard, read-only and optional editing; acceptance and performance evidence in spec/16.
- [ ] **Showcase app** (`apps/showcase`): every component from every crate, one page each, real data, both themes; doubles as e2e/snapshot target. Gate: a test enumerates exported components and fails if any lacks a showcase page.
- [ ] **Tooling DX**: `guiraffe new --template {blank,dashboard,editor,chat}`, `guiraffe dev` (watch+rebuild+relaunch), `guiraffe add <crate>`, `guiraffe doctor` (toolchain, gpui-fast pin, system deps), `guiraffe snapshot`/`bench` wrappers, docs site generated from showcase.
- [ ] **After current work:** study the Refactoring UI book (in repo root, user-provided; do not commit it) — its component list and principles → design-quality styling + layout presets (theme recipes, spacing/type scales, hierarchy, color, depth) in core/layout, applied across showcase.
- [ ] Bugs found while building the showcase: flow `FlowState::new` is fallible inside `cx.new` (add infallible ctor from validated editor; flow/state.rs:39); chat tool-call header centred instead of left (chat/message.rs); overlays FloatingTooltip lacks border/shadow contrast (overlays/floating.rs:39); LoadingOverlay default fully opaque (overlays/widgets.rs:58); guiraffe-test Snapshotter needs a "settle" option so background-rendered content (mermaid) appears in goldens; regression test for mermaid width re-render.

## User feedback on showcase (2026-11-16)
- [ ] 1 ScrollArea doesn't scroll · 2 SegmentedControl needs two clicks (focus then select) · 3 RangeSlider track glitch · 4 FloatingTooltip never hides (focus/hover loss in general?)
- [ ] 5 Mermaid diagrams blank (live too; ../gui renders them — compare)
- [ ] 6 Flow glitches: edge end overshoots handle; handle/edge stub drawn past node edge; step-edge corners overshoot after zoom/pan · 7 minimap drag-to-pan
- [ ] 8/9 Gantt + tracks: lock x positions (per item / global) · 10 resize cursor on edges · 11 resize from left edge too (configurable: none/start/end/both)
- [ ] Integrate GPUI Controls (buttons, form inputs, selects, etc.) + GPUI Kit charts into showcase; every entry needs a live options panel exposing its configuration
- [ ] chat: Reasoning toggle header centred (same bug as tool header, chat/message.rs); 302/188 contrast-floor failures pinned in core state-token test → fix palette (with Refactoring UI study); §12 widget audit gaps in spec/18 (SegmentedControl/RangeSlider focus grab, bare-div ActionIcon/ChipGroup/Dropzone/sidebar, no a11y roles, timeline pointer-only)
- [ ] code viewer polish: minimap is a solid block when whole file fits (draw line strokes + light viewport box); trace curve crosses text (route in gutter/margin); inlay hint double space; VALIDATION.md cites stale release hash. data-ui: nested repeat defaults to window height (default to container).
- [ ] **Showcase completeness audit** (user 2026-11-17): every component and every public option demonstrated (knobs where it makes sense), across all crates — after feat-code-demos and feat-datagrid merge (they touch the showcase). Produce a component×option matrix in the showcase README and fill the gaps.
- [ ] code demos polish: trace endpoint dots overlap first glyph; cross-viewer trace crosses line numbers; showcase meta line 'guiraffe-code ·' has empty covers list for code entries.
- [ ] Owner gaps from showcase audit (crates/showcase/COVERAGE.md): ActionIcon disabled/loading; Pill size/color/disabled; Indicator position/offset/size; Highlight/Mark color; TagsInput disabled/error/required/size + shared input invalid/required state; ScrollArea scrollbar options; Toast color/icon/title; FloatingTooltip arrow; flow read-only + runtime options; mermaid theme override/selection; markdown heading anchors/TOC; chat props; Gantt snap; datagrid filter UI; Kit charts knobs (superseded by guiraffe-charts).
- [ ] compare app: Document page wraps a plain MarkdownView in a scroll container because scrollable mode used to collapse — now fixed (fix-user-bugs-3); switch it to MarkdownView scrollable. Add input-latency and RAPL energy to the harness.
- [ ] catalogue recipes polish: error helper text should use error colour + smaller size; DataUi input height a bit short vs Kit inputs
- [x] palette polish: light-mode tinted and subtle text preserves the palette hue when contrast requires going beyond its darkest shade.

## DnD showcase polish (foreman golden review, 2026-11-18)
- Tree/flame demo polish landed in feat-flame-tree; DataGrid/CodeViewer, Flow/Timeline/Charts wall-clock budgets pending the quiet-machine run.
