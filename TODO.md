# TODO (tracked per crate; also see spec/)
- [x] core: palettes, sizes, variants, theme global + Controls bridge (foreman)
- [ ] flow-core: model, viewport, spatial index, history, serde (codex)
- [ ] flow: canvas, handles, connect, selection, minimap, controls, bench (codex)
- [ ] mermaid: port mer, async render cache, pan/zoom (omp)
- [ ] markdown: GFM viewer, highlighting, streaming, mermaid feature (omp)
- [ ] layout: Stack/Group/Flex/Grid/SimpleGrid/Container/Center/AspectRatio/AppShell (kilo)
- [x] windows: registry, lifecycle, bus, persistence (foreman, wave 2)
- [ ] overlays: stack, modals, drawer, toasts, spotlight, desktop notify (wave 2)
- [ ] components: see spec/03 (wave 2)
- [ ] gallery app (wave 2)
- [ ] agent chat UI (wave 2)

## Follow-ups found in review
- [ ] windows: X11 restores windows 2px right of request (see ../gui KNOWN_ISSUES #127) — compensate when platform is X11
- [ ] windows: tear-off panels between windows (dock-host port), parent/child + modal windows
- [ ] windows: example app (examples/multi.rs)
- [ ] core: style helper traits (.p_size, .radius_size, .shadow_size) once layout lands
