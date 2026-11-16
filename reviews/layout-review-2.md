# Foreman review #2: guiraffe-layout (blocks merge)

All 137 tests pass, but several fixes are incomplete or added defects. Fix everything below with real regression tests.

## Incomplete items from review 1
- 1: an offset after a sibling overlaps it (span 4 then span(4).offset(2) gives 3..7, overlapping 1..5). In Mantine an offset is a margin after the previous column (cursor + offset). Add a test.
- 3: the measurement test proves nothing, because a 600 px container and the default window are in the same band. Test a container narrower than 576 inside a wide window. Make Measured the default for Grid and SimpleGrid (or explain in the report why not), and make Splitter and AppShell panes give their children correct widths.
- 5: fix the debug_assert message (splitter.rs ~284) and assert in a pure validation fn that is tested. Add a test restoring a different pane count.
- 6: actually assert the sibling render count. Make the "no change" drag test fail if the compare logic is removed (it currently passes because of min-size clamping).
- 7: AppShellEvent via EventEmitter. Add a render-count test proving main doesn't re-render on collapse (cache it or isolate it).
- 8: a drag handle for resizable navbar/aside, aside breakpoint collapse, and collapse_below taking a Breakpoint/Size rather than raw px.
- 12: debug_assert on a bad ratio.

## New defects
- N1 HIGH: ContainerWidthState must store the exact width (for min_child_width) but notify only on a band change.
- N2 HIGH: sharing one ContainerWidthState between widgets of different widths flips the band every frame, an infinite re-render loop. Make it impossible: one owner per state, enforced or detected, with a debug_assert and a test.
- N3 HIGH: persistence only happens inside the on_resize branch. persist_key alone must persist. Add a test.
- N4 MED: the comparison buffer is a fresh Rc per render, so repeated on_resize and store writes during a drag. Keep it in the state entity. Add a test counting on_resize calls during a drag.
- N5 MED: validate restored sizes (NaN, negative, wrong count) and normalise them to the container total.
- N6 MED: no Mutex<HashMap> Global. Define a small `LayoutPersist` trait in the layout crate with an in-memory implementation, which the foreman will back with guiraffe-core::persist. Single-threaded, so no Mutex.
- N7 MED: auto-collapse only when the threshold is crossed, never overriding a manual toggle at every resize.
- N8 LOW-MED: avoid the stale first frame where possible (e.g. take the initial width from window bounds), and document what remains.
- N9 LOW: offset overflow on an empty row. Pick a sane behaviour and test it.
- N10 LOW: Splitter and AppShell emit typed events (conventions §3).
