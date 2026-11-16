# Foreman review #1: guiraffe-layout. Must fix before merge

1. HIGH grid.rs placement(): offset+span is not limited to the column count, so it creates implicit extra columns and the row overflows past 100 %. The test at ~312 locks the bug in. Overflow should wrap to the next row (Mantine behaviour). Use saturating maths, no u16 overflow or i16 wrap.
2. HIGH: Col defaults to span 12 whatever the grid's column count. A default span should mean "full", resolved against Grid.columns at render.
3. HIGH: responsive props follow the window viewport, not the container. Measure the real container width (canvas/prepaint bounds → small entity, which re-renders only when the breakpoint *band* changes). Splitter and AppShell panes must give their children correct widths.
4. MED-HIGH: add a Base band below Xs (< 576). Test at 0 and 575 with an Xs override.
5. MED-HIGH Splitter: .persist_key() (serde SplitterSizes), max_size, theme default min, N panes, limit invalid mins (debug_assert). Test drag → parent re-render → size kept.
6. MED: RenderCounter-style tests. A drag with no size change must not notify or call on_resize, and must not re-render siblings. No Vec allocation per resize event.
7. MED AppShell: make it a Control (AppShellState entity). Keep the collapsed navbar mounted (hidden or width 0) so its state survives. Collapse must not re-render main (test it).
8. MED AppShell: resizable navbar/aside, aside collapse, breakpoint collapse (mobile/desktop), theme defaults rather than pixels. Tests must not hard-code 240 px.
9. MED Container: w_full + max_w, not a fixed w.
10. MED: grow should work as in Mantine. Group grow → children flex_1/min_w_0. Grid grow → the last row fills.
11. MED: no raw Hsla or pixel constants (Divider thickness, colour, font). Limit negative or NaN gaps with a debug_assert.
12. LOW AspectRatio: allow only finite ratios in 0.01..=100, plus .relative() and overflow_hidden.
13. LOW SimpleGrid: cols, spacing and vertical_spacing take Responsive values.
Tests to add: everything above, plus Container in a narrow window and Group grow filling the width.
Note: the XProps/derive(Props) work is phase 2 infrastructure and not required yet.
